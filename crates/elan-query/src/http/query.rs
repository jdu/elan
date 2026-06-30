//! HTTP query and catalog handlers for elan-query.
//!
//! - `POST /api/v1/query` — execute SQL for an authenticated user; emits
//!   `QuerySubmitted` / `QueryCompleted` / `QueryFailed` audit events.
//! - `GET /api/v1/catalog` — return the IAM-filtered namespace/dataset tree.
//!
//! Authentication is PoC-only: `Authorization: Bearer <username>` maps the
//! bearer token directly to a user ID with no signature verification.

use crate::session::SessionFactory;
use axum::{
    extract::{Extension, State},
    http::{HeaderMap, StatusCode},
    Json, Router,
    routing::get,
    routing::post,
};
use elan_audit::{
    event::{AuditSubject, QueryCompletedPayload, QueryFailedPayload, QuerySubmittedPayload},
    AuditEvent, AuditPayload, AuditSink,
};
use elan_common::types::api::{
    CatalogResponse, ErrorResponse, NamespaceInfo, DatasetSummary, QueryRequest, QueryResponse,
};
use elan_iam::Subject;
use std::{collections::HashMap, sync::Arc, time::Instant};
use tracing::{error, info, instrument};
use uuid::Uuid;

/// Shared state injected into every HTTP handler via Axum's `State` extractor.
pub struct AppState {
    pub session_factory: Arc<SessionFactory>,
    pub audit: Arc<dyn AuditSink>,
    pub instance_name: String,
}

/// Build the Axum router with all elan-query HTTP endpoints.
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/v1/query", post(handle_query))
        .route("/api/v1/catalog", get(handle_catalog))
        .route("/health", get(health))
        .with_state(state)
}

async fn health() -> Json<elan_common::types::api::HealthResponse> {
    Json(elan_common::types::api::HealthResponse { status: "ok".into() })
}

/// Extract the requesting user's identity from the `Authorization` header.
/// Falls back to `"anonymous"` if the header is absent or malformed.
fn extract_subject(headers: &HeaderMap) -> Subject {
    // PoC auth: "Authorization: Bearer <username>"
    let user_id = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .unwrap_or("anonymous")
        .to_string();

    Subject {
        user_id,
        groups: vec![],
    }
}

#[instrument(skip(state, headers, body), fields(user_id))]
async fn handle_query(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<QueryRequest>,
) -> Result<Json<QueryResponse>, (StatusCode, Json<ErrorResponse>)> {
    let subject = extract_subject(&headers);
    tracing::Span::current().record("user_id", &subject.user_id);

    let query_id = Uuid::new_v4();
    let session_id = body.session_id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
    let audit_subject = AuditSubject {
        user_id: subject.user_id.clone(),
        groups: subject.groups.clone(),
        session_id: Some(session_id.clone()),
    };

    info!(query_id = %query_id, sql = %body.sql, "query submitted");

    let ctx = state
        .session_factory
        .build_for_user(subject.clone())
        .await
        .map_err(|e| err500("session build failed", &e.to_string()))?;

    // Publish QuerySubmitted audit event
    let _ = state
        .audit
        .publish(AuditEvent::new(
            "elan-query",
            &state.instance_name,
            audit_subject.clone(),
            AuditPayload::QuerySubmitted(QuerySubmittedPayload {
                query_id,
                sql: body.sql.clone(),
                resolved_tables: vec![],
                executors: HashMap::new(),
            }),
        ))
        .await;

    let start = Instant::now();

    let df_result = match ctx.sql(&body.sql).await {
        Ok(df) => df.collect().await,
        Err(e) => Err(e),
    };

    let duration_ms = start.elapsed().as_millis() as u64;

    match df_result {
        Ok(batches) => {
            let rows_returned: usize = batches.iter().map(|b| b.num_rows()).sum();

            // Publish QueryCompleted
            let _ = state
                .audit
                .publish(AuditEvent::new(
                    "elan-query",
                    &state.instance_name,
                    audit_subject,
                    AuditPayload::QueryCompleted(QueryCompletedPayload {
                        query_id,
                        duration_ms,
                        rows_returned,
                        bytes_scanned: 0, // TODO: wire up actual bytes from executor
                    }),
                ))
                .await;

            let response = batches_to_response(query_id, batches, duration_ms)
                .map_err(|e| err500("response serialization failed", &e.to_string()))?;
            Ok(Json(response))
        }
        Err(e) => {
            error!(error = %e, query_id = %query_id, "query failed");

            let _ = state
                .audit
                .publish(AuditEvent::new(
                    "elan-query",
                    &state.instance_name,
                    audit_subject,
                    AuditPayload::QueryFailed(QueryFailedPayload {
                        query_id,
                        duration_ms,
                        error_kind: "DataFusionError".into(),
                        error_msg: e.to_string(),
                    }),
                ))
                .await;

            let is_access_denied = e.to_string().contains("AccessDenied")
                || e.to_string().contains("permission denied");
            if is_access_denied {
                Err((
                    StatusCode::FORBIDDEN,
                    Json(ErrorResponse {
                        error: "AccessDenied".into(),
                        detail: Some(e.to_string()),
                    }),
                ))
            } else {
                Err(err500("query execution failed", &e.to_string()))
            }
        }
    }
}

async fn handle_catalog(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Json<CatalogResponse> {
    let subject = extract_subject(&headers);
    let ctx = state
        .session_factory
        .build_for_user(subject)
        .await
        .unwrap_or_else(|_| datafusion::prelude::SessionContext::new());

    let mut namespaces = vec![];
    if let Some(catalog) = ctx.catalog("elan") {
        for ns_name in catalog.schema_names() {
            if let Some(schema) = catalog.schema(&ns_name) {
                let datasets = schema
                    .table_names()
                    .into_iter()
                    .map(|name| DatasetSummary {
                        name,
                        source_type: "unknown".into(),
                        coordinator_id: String::new(),
                    })
                    .collect();
                namespaces.push(NamespaceInfo { name: ns_name, datasets });
            }
        }
    }

    Json(CatalogResponse { namespaces })
}

fn batches_to_response(
    query_id: Uuid,
    batches: Vec<arrow_array::RecordBatch>,
    duration_ms: u64,
) -> anyhow::Result<QueryResponse> {
    if batches.is_empty() {
        return Ok(QueryResponse {
            query_id,
            columns: vec![],
            rows: vec![],
            duration_ms,
        });
    }

    let schema = batches[0].schema();
    let columns: Vec<String> = schema.fields().iter().map(|f| f.name().clone()).collect();

    let mut rows: Vec<Vec<serde_json::Value>> = vec![];
    for batch in &batches {
        for row_idx in 0..batch.num_rows() {
            let row: Vec<serde_json::Value> = (0..batch.num_columns())
                .map(|col_idx| arrow_col_to_json(batch.column(col_idx), row_idx))
                .collect();
            rows.push(row);
        }
    }

    Ok(QueryResponse {
        query_id,
        columns,
        rows,
        duration_ms,
    })
}

fn arrow_col_to_json(col: &dyn arrow_array::Array, row: usize) -> serde_json::Value {
    use arrow_array::Array;
    if col.is_null(row) {
        return serde_json::Value::Null;
    }

    // Cast to string via DataFusion's scalar value conversion
    // This is a simplification — production code would handle each type properly
    match arrow_cast::cast(col, &arrow_schema::DataType::Utf8) {
        Ok(cast) => {
            let s = cast
                .as_any()
                .downcast_ref::<arrow_array::StringArray>()
                .and_then(|a| if a.is_null(row) { None } else { Some(a.value(row).to_string()) })
                .unwrap_or_else(|| "null".to_string());
            serde_json::Value::String(s)
        }
        Err(_) => serde_json::Value::String(format!("<{}>", col.data_type())),
    }
}

fn err500(error: &str, detail: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            error: error.to_string(),
            detail: Some(detail.to_string()),
        }),
    )
}
