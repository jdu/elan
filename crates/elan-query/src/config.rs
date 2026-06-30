//! Configuration loading for elan-query.
//!
//! Settings are layered: built-in defaults < config file < `ELAN_QUERY__*`
//! environment variables.

use serde::Deserialize;

/// Top-level configuration for elan-query.
#[derive(Debug, Clone, Deserialize)]
pub struct QueryConfig {
    pub http_addr: String,
    pub central_endpoint: String,
    pub kafka_brokers: Option<String>,
    pub instance_name: String,
    pub iam_refresh_secs: u64,
}

impl Default for QueryConfig {
    fn default() -> Self {
        Self {
            http_addr: "0.0.0.0:3000".into(),
            central_endpoint: "http://localhost:50051".into(),
            kafka_brokers: None,
            instance_name: "query-node-1".into(),
            iam_refresh_secs: 60,
        }
    }
}

/// Load configuration, optionally from a file path, then override with env vars.
pub fn load(config_path: Option<&str>) -> anyhow::Result<QueryConfig> {
    let mut builder = config::Config::builder()
        .set_default("http_addr", "0.0.0.0:3000")?
        .set_default("central_endpoint", "http://localhost:50051")?
        .set_default("instance_name", "query-node-1")?
        .set_default("iam_refresh_secs", 60)?;

    if let Some(path) = config_path {
        builder = builder.add_source(config::File::with_name(path));
    }
    builder = builder
        .add_source(config::Environment::with_prefix("ELAN_QUERY").separator("__"));

    Ok(builder.build()?.try_deserialize()?)
}
