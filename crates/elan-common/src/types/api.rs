//! HTTP API request/response shapes for elan-query's public REST endpoints.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Body for `POST /api/v1/query`.
#[derive(Debug, Serialize, Deserialize)]
pub struct QueryRequest {
    pub sql: String,
    pub session_id: Option<String>,
}

/// Successful response from `POST /api/v1/query`.
/// Rows are serialized as nested JSON arrays for simplicity; a future version
/// may return Arrow IPC directly.
#[derive(Debug, Serialize, Deserialize)]
pub struct QueryResponse {
    pub query_id: Uuid,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub duration_ms: u64,
}

/// Error body returned on non-2xx responses.
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub detail: Option<String>,
}

/// Response from `GET /api/v1/catalog`.
#[derive(Debug, Serialize, Deserialize)]
pub struct CatalogResponse {
    pub namespaces: Vec<NamespaceInfo>,
}

/// A namespace and its visible datasets within the catalog response.
#[derive(Debug, Serialize, Deserialize)]
pub struct NamespaceInfo {
    pub name: String,
    pub datasets: Vec<DatasetSummary>,
}

/// Lightweight dataset descriptor used in catalog listings (no schema bytes).
#[derive(Debug, Serialize, Deserialize)]
pub struct DatasetSummary {
    pub name: String,
    pub source_type: String,
    pub coordinator_id: String,
}

/// Response body for all `GET /health` endpoints.
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
}
