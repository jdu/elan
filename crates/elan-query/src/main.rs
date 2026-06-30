//! elan-query: the central SQL gateway for the elan federated query system.
//!
//! On startup elan-query:
//! 1. Loads all datasets from elan-central's `CatalogService`.
//! 2. Loads IAM policies from elan-central's `IamService`.
//! 3. Starts a background task to refresh the catalog every 30 seconds.
//! 4. Listens for HTTP queries on the configured address.
//!
//! For each incoming query a per-request `SessionContext` is built with:
//! - An `ElanCatalogProvider` populated with the IAM-filtered dataset list.
//! - An `IamFilterRule` physical optimizer rule that enforces access at scan time.
//!
//! Audit events (QuerySubmitted, QueryCompleted, QueryFailed) are sent to
//! elan-central via `CentralAuditSink`, falling back to a no-op sink if the
//! connection fails.

mod catalog;
mod config;
mod http;
mod planner;
mod session;

use crate::http::query::AppState;
use crate::session::SessionFactory;
use elan_audit::{CentralAuditSink, sink::NoOpAuditSink, AuditSink};
use std::sync::Arc;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args: Vec<String> = std::env::args().collect();
    let config_path = args
        .windows(2)
        .find(|w| w[0] == "--config")
        .map(|w| w[1].as_str());

    let cfg = config::load(config_path)?;
    info!(addr = %cfg.http_addr, "starting elan-query");

    let session_factory = SessionFactory::new(&cfg).await?;

    let audit: Arc<dyn AuditSink> = match CentralAuditSink::new(&cfg.central_endpoint).await {
        Ok(sink) => {
            info!(endpoint = %cfg.central_endpoint, "central audit sink enabled");
            Arc::new(sink)
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to connect central audit sink, using no-op");
            Arc::new(NoOpAuditSink)
        }
    };

    let state = Arc::new(AppState {
        session_factory,
        audit,
        instance_name: cfg.instance_name.clone(),
    });

    let app = http::query::router(state);

    let listener = tokio::net::TcpListener::bind(&cfg.http_addr).await?;
    info!("elan-query listening on {}", cfg.http_addr);
    axum::serve(listener, app).await?;

    Ok(())
}
