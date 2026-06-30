//! HTTP SQL service: `POST /sql` → Arrow IPC stream.
//!
//! This is the primary query path for elan-query.  `RemoteTableScanExec`
//! posts plain-text SQL here and receives Arrow IPC bytes in response.
//! A new `SessionContext` is created per request so sessions are stateless.

use crate::config::ObjectStoreConfig;
use crate::context::register_object_store;
use arrow_ipc::writer::StreamWriter;
use axum::{
    Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::post,
};
use datafusion::catalog::TableProvider;
use datafusion::prelude::SessionContext;
use std::sync::Arc;
use tracing::{debug, error};

/// Shared state for the SQL HTTP service.
///
/// Holds the pre-built table providers and, when object storage is configured,
/// the S3 connection details needed to re-register the store on every new
/// `SessionContext`.  The store must be registered on the context that actually
/// *executes* queries — not just the one used to build the providers at startup.
#[derive(Clone)]
pub struct SqlServiceState {
    providers: Arc<Vec<(String, Arc<dyn TableProvider>)>>,
    object_store: Option<Arc<ObjectStoreConfig>>,
}

impl SqlServiceState {
    pub fn new(
        providers: Arc<Vec<(String, Arc<dyn TableProvider>)>>,
        object_store: Option<Arc<ObjectStoreConfig>>,
    ) -> Self {
        Self { providers, object_store }
    }

    async fn session(&self) -> anyhow::Result<SessionContext> {
        let ctx = SessionContext::new();
        // Re-register the object store so DataFusion can resolve s3:// paths
        // during execution (not just during provider construction at startup).
        if let Some(s3_cfg) = &self.object_store {
            register_object_store(&ctx, s3_cfg)?;
        }
        for (name, provider) in self.providers.iter() {
            ctx.register_table(name.as_str(), Arc::clone(provider))?;
        }
        Ok(ctx)
    }
}

/// POST /sql
/// Body: plain-text SQL
/// Response: Arrow IPC stream bytes (application/vnd.apache.arrow.stream)
async fn handle_sql(
    State(state): State<SqlServiceState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let sql = match std::str::from_utf8(&body) {
        Ok(s) => s.to_string(),
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("invalid UTF-8: {e}")).into_response();
        }
    };

    debug!(sql = %sql, "executor received SQL");

    let ctx = match state.session().await {
        Ok(c) => c,
        Err(e) => {
            error!(error = %e, "failed to build session");
            return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        }
    };

    let batches = async {
        let df = ctx.sql(&sql).await?;
        df.collect().await
    }
    .await;

    match batches {
        Err(e) => {
            error!(error = %e, sql = %sql, "SQL execution failed");
            (StatusCode::BAD_REQUEST, e.to_string()).into_response()
        }
        Ok(batches) => {
            if batches.is_empty() {
                // Return empty IPC stream with schema
                let schema = Arc::new(datafusion::arrow::datatypes::Schema::empty());
                let mut buf = Vec::new();
                if let Ok(mut w) = StreamWriter::try_new(&mut buf, &schema) {
                    let _ = w.finish();
                }
                return (
                    [(header::CONTENT_TYPE, "application/vnd.apache.arrow.stream")],
                    buf,
                )
                    .into_response();
            }

            let schema = batches[0].schema();
            let mut buf = Vec::new();
            let mut writer = match StreamWriter::try_new(&mut buf, &schema) {
                Ok(w) => w,
                Err(e) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
                }
            };

            for batch in &batches {
                if let Err(e) = writer.write(batch) {
                    return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
                }
            }

            if let Err(e) = writer.finish() {
                return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
            }

            (
                [(header::CONTENT_TYPE, "application/vnd.apache.arrow.stream")],
                buf,
            )
                .into_response()
        }
    }
}

pub fn router(state: SqlServiceState) -> Router {
    Router::new()
        .route("/sql", post(handle_sql))
        .with_state(state)
}
