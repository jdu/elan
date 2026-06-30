use axum::{Json, Router, extract::Query, routing::get};
use elan_common::types::api::HealthResponse;
use serde::Deserialize;

pub fn router() -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/auth/check", get(auth_check))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".into(),
    })
}

#[derive(Deserialize)]
struct AuthCheckQuery {
    dataset: String,
    caller: String,
}

#[derive(serde::Serialize)]
struct AuthCheckResponse {
    allowed: bool,
    reason: String,
}

/// Called by elan-executor to verify that a remote caller has access to a dataset.
/// For the PoC this always returns allowed=true.
/// In production this would check the IAM engine (loaded from central at startup).
async fn auth_check(Query(q): Query<AuthCheckQuery>) -> Json<AuthCheckResponse> {
    tracing::info!(
        dataset = %q.dataset,
        caller = %q.caller,
        "auth check from executor"
    );
    Json(AuthCheckResponse {
        allowed: true,
        reason: "PoC: all access allowed by coordinator".into(),
    })
}
