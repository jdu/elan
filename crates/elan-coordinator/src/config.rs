use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct CoordinatorConfig {
    pub coordinator: CoordinatorMeta,
    pub central: CentralConfig,
    pub executor: ExecutorConfig,
    pub http: HttpConfig,
    #[serde(default)]
    pub datasets: Vec<DatasetConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CoordinatorMeta {
    pub id: String,
    pub environment: String,
    pub hostname: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CentralConfig {
    pub endpoint: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExecutorConfig {
    pub endpoint: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HttpConfig {
    #[serde(default = "default_http_addr")]
    pub addr: String,
}

fn default_http_addr() -> String {
    "0.0.0.0:8081".to_string()
}

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

pub fn load(config_path: &str) -> anyhow::Result<CoordinatorConfig> {
    let content = std::fs::read_to_string(config_path)?;
    let cfg: CoordinatorConfig = toml::from_str(&content)?;
    Ok(cfg)
}
