use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetInfo {
    pub id: Uuid,
    pub name: String,
    pub namespace: String,
    pub source_type: SourceType,
    pub coordinator_id: String,
    pub executor_endpoint: String,
    /// IPC-serialized Arrow schema bytes
    pub schema_ipc: Vec<u8>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Parquet,
    Csv,
    Postgres,
    Delta,
}

impl std::fmt::Display for SourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            SourceType::Parquet => "parquet",
            SourceType::Csv => "csv",
            SourceType::Postgres => "postgres",
            SourceType::Delta => "delta",
        };
        write!(f, "{s}")
    }
}

impl TryFrom<&str> for SourceType {
    type Error = crate::ElanError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "parquet" => Ok(SourceType::Parquet),
            "csv" => Ok(SourceType::Csv),
            "postgres" => Ok(SourceType::Postgres),
            "delta" => Ok(SourceType::Delta),
            other => Err(crate::ElanError::Config(format!("unknown source type: {other}"))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorInfo {
    pub id: String,
    pub environment: String,
    pub hostname: String,
    pub executor_endpoint: String,
    pub is_alive: bool,
}
