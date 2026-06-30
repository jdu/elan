//! Configuration types and loading for elan-coordinator.
//!
//! Configuration is read from a TOML file (path supplied via `--config`).
//! The `datasets` array declares every data source this coordinator should
//! register with elan-central.

use serde::Deserialize;

/// Top-level coordinator configuration (parsed from TOML).
#[derive(Debug, Clone, Deserialize)]
pub struct CoordinatorConfig {
    pub coordinator: CoordinatorMeta,
    pub central: CentralConfig,
    pub executor: ExecutorConfig,
    pub http: HttpConfig,
    #[serde(default)]
    pub datasets: Vec<DatasetConfig>,
}

/// Identity fields sent to elan-central on registration.
#[derive(Debug, Clone, Deserialize)]
pub struct CoordinatorMeta {
    pub id: String,
    pub environment: String,
    pub hostname: String,
}

/// gRPC endpoint for elan-central.
#[derive(Debug, Clone, Deserialize)]
pub struct CentralConfig {
    pub endpoint: String,
}

/// HTTP endpoint of the co-located elan-executor, advertised to elan-central
/// so that elan-query can dispatch SQL to it.
#[derive(Debug, Clone, Deserialize)]
pub struct ExecutorConfig {
    pub endpoint: String,
}

/// Coordinator's own HTTP server configuration (auth-check endpoint).
#[derive(Debug, Clone, Deserialize)]
pub struct HttpConfig {
    #[serde(default = "default_http_addr")]
    pub addr: String,
}

fn default_http_addr() -> String {
    "0.0.0.0:8081".to_string()
}

/// Descriptor for a single dataset this coordinator should register.
///
/// The `type` field (TOML inline table `type = "parquet"` etc.) is used as
/// the serde tag to select the variant.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DatasetConfig {
    Parquet {
        name: String,
        namespace: String,
        path: String,
        description: Option<String>,
    },
    Csv {
        name: String,
        namespace: String,
        path: String,
        #[serde(default = "default_delimiter")]
        delimiter: char,
        #[serde(default = "default_true")]
        has_header: bool,
    },
    Postgres {
        name: String,
        namespace: String,
        host: String,
        #[serde(default = "default_pg_port")]
        port: u16,
        database: String,
        schema: String,
        table: String,
        username: String,
        password: String,
    },
    Delta {
        name: String,
        namespace: String,
        path: String,
    },
}

fn default_delimiter() -> char {
    ','
}
fn default_true() -> bool {
    true
}
fn default_pg_port() -> u16 {
    5432
}

impl DatasetConfig {
    pub fn name(&self) -> &str {
        match self {
            DatasetConfig::Parquet { name, .. } => name,
            DatasetConfig::Csv { name, .. } => name,
            DatasetConfig::Postgres { name, .. } => name,
            DatasetConfig::Delta { name, .. } => name,
        }
    }

    pub fn namespace(&self) -> &str {
        match self {
            DatasetConfig::Parquet { namespace, .. } => namespace,
            DatasetConfig::Csv { namespace, .. } => namespace,
            DatasetConfig::Postgres { namespace, .. } => namespace,
            DatasetConfig::Delta { namespace, .. } => namespace,
        }
    }

    pub fn source_type(&self) -> &str {
        match self {
            DatasetConfig::Parquet { .. } => "parquet",
            DatasetConfig::Csv { .. } => "csv",
            DatasetConfig::Postgres { .. } => "postgres",
            DatasetConfig::Delta { .. } => "delta",
        }
    }

    pub fn metadata_json(&self) -> serde_json::Value {
        match self {
            DatasetConfig::Parquet { path, .. } => serde_json::json!({ "path": path }),
            DatasetConfig::Csv { path, delimiter, has_header, .. } => serde_json::json!({
                "path": path,
                "delimiter": delimiter.to_string(),
                "has_header": has_header,
            }),
            DatasetConfig::Postgres { host, port, database, schema, table, username, .. } => {
                serde_json::json!({
                    "host": host,
                    "port": port,
                    "database": database,
                    "schema": schema,
                    "table": table,
                    "username": username,
                })
            }
            DatasetConfig::Delta { path, .. } => serde_json::json!({ "path": path }),
        }
    }
}

/// Parse coordinator configuration from a TOML file.
pub fn load(config_path: &str) -> anyhow::Result<CoordinatorConfig> {
    let content = std::fs::read_to_string(config_path)?;
    let cfg: CoordinatorConfig = toml::from_str(&content)?;
    Ok(cfg)
}
