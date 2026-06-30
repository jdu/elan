//! Configuration types and loading for elan-executor.
//!
//! The executor binds two ports derived from a single `bind_port`:
//! - `bind_port` — Ballista scheduler/executor gRPC (kept for compatibility).
//! - `bind_port + 1` — Arrow IPC HTTP SQL service used by elan-query.
//!
//! When `[object_store]` is present, the executor reads datasets from S3-compatible
//! object storage (e.g. MinIO) instead of local files.  This enables multiple
//! executor replicas behind a load balancer to all share the same data.

use serde::Deserialize;

/// Top-level executor configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct ExecutorConfig {
    pub bind_host: String,
    pub bind_port: u16,
    pub coordinator_url: String,
    /// When set, register this S3-compatible store before opening any datasets.
    /// Paths in `datasets` may then be `s3://bucket/...` URLs.
    #[serde(default)]
    pub object_store: Option<ObjectStoreConfig>,
    #[serde(default)]
    pub datasets: Vec<DatasetMount>,
}

/// Connection details for an S3-compatible object store (MinIO, AWS S3, GCS, etc.).
#[derive(Debug, Clone, Deserialize)]
pub struct ObjectStoreConfig {
    /// Full URL of the S3 endpoint, e.g. `http://minio:9000`.
    pub endpoint: String,
    pub access_key: String,
    pub secret_key: String,
    /// Bucket that contains all elan datasets.
    pub bucket: String,
    /// Allow plain HTTP connections — set to true for local MinIO.
    #[serde(default)]
    pub allow_http: bool,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            bind_host: "0.0.0.0".into(),
            bind_port: 50055,
            coordinator_url: "http://localhost:8081".into(),
            object_store: None,
            datasets: vec![],
        }
    }
}

/// A dataset the executor serves — path may be a local filesystem path or an
/// `s3://bucket/key` URL when object storage is configured.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DatasetMount {
    Parquet {
        table_name: String,
        path: String,
    },
    Csv {
        table_name: String,
        path: String,
        #[serde(default = "default_true")]
        has_header: bool,
    },
}

fn default_true() -> bool {
    true
}

impl DatasetMount {
    pub fn table_name(&self) -> &str {
        match self {
            DatasetMount::Parquet { table_name, .. } => table_name,
            DatasetMount::Csv { table_name, .. } => table_name,
        }
    }
}

/// Load configuration from an optional TOML file; use defaults if none provided.
pub fn load(config_path: Option<&str>) -> anyhow::Result<ExecutorConfig> {
    match config_path {
        Some(path) => {
            let content = std::fs::read_to_string(path)?;
            Ok(toml::from_str(&content)?)
        }
        None => Ok(ExecutorConfig::default()),
    }
}
