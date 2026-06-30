//! HTTP `GET /health` endpoint for elan-central.

use axum::{Json, Router, routing::get};
use elan_common::types::api::HealthResponse;

pub fn router() -> Router {
    Router::new().route("/health", get(health))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".into(),
    })
}
