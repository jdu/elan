use serde::Deserialize;

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

pub fn load(config_path: Option<&str>) -> anyhow::Result<ExecutorConfig> {
    match config_path {
        Some(path) => {
            let content = std::fs::read_to_string(path)?;
            Ok(toml::from_str(&content)?)
        }
        None => Ok(ExecutorConfig::default()),
    }
}
