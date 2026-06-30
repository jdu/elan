use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct QueryRequest {
    pub sql: String,
    pub session_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QueryResponse {
    pub query_id: Uuid,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub duration_ms: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub detail: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CatalogResponse {
    pub namespaces: Vec<NamespaceInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NamespaceInfo {
    pub name: String,
    pub datasets: Vec<DatasetSummary>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DatasetSummary {
    pub name: String,
    pub source_type: String,
    pub coordinator_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
}
