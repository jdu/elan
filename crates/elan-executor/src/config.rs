//! Configuration types and loading for elan-executor.
//!
//! The executor binds two ports derived from a single `bind_port`:
//! - `bind_port` — Ballista scheduler/executor gRPC (kept for compatibility).
//! - `bind_port + 1` — Arrow IPC HTTP SQL service used by elan-query.

use serde::Deserialize;

/// Top-level executor configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct ExecutorConfig {
    pub bind_host: String,
    pub bind_port: u16,
    pub coordinator_url: String,
    #[serde(default)]
    pub datasets: Vec<DatasetMount>,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            bind_host: "0.0.0.0".into(),
            bind_port: 50055,
            coordinator_url: "http://localhost:8081".into(),
            datasets: vec![],
        }
    }
}

/// A local data file mounted into the executor's DataFusion context.
/// The `type` field selects the variant (e.g. `type = "parquet"`).
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
