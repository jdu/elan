//! Configuration loading for elan-central.
//!
//! Settings are layered: built-in defaults < config file (TOML/YAML/JSON)
//! < `ELAN_CENTRAL__*` environment variables.

use serde::Deserialize;

/// Top-level configuration for the elan-central authority service.
#[derive(Debug, Deserialize, Clone)]
pub struct CentralConfig {
    pub grpc_addr: String,
    pub http_addr: String,
    pub database_url: String,
}

impl Default for CentralConfig {
    fn default() -> Self {
        Self {
            grpc_addr: "0.0.0.0:50051".into(),
            http_addr: "0.0.0.0:8080".into(),
            database_url: "sqlite://elan_central.db".into(),
        }
    }
}

/// Load configuration, optionally from a file path, then override with env vars.
pub fn load(config_path: Option<&str>) -> anyhow::Result<CentralConfig> {
    let mut builder = config::Config::builder()
        .set_default("grpc_addr", "0.0.0.0:50051")?
        .set_default("http_addr", "0.0.0.0:8080")?
        .set_default("database_url", "sqlite://elan_central.db")?;

    if let Some(path) = config_path {
        builder = builder.add_source(config::File::with_name(path));
    }

    builder = builder.add_source(
        config::Environment::with_prefix("ELAN_CENTRAL").separator("__"),
    );

    Ok(builder.build()?.try_deserialize()?)
}
