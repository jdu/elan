mod config;
mod db;
mod grpc;
mod http;

use crate::grpc::{
    audit_svc::{AuditBroadcast, AuditSvc},
    catalog_svc::CatalogSvc,
    coordinator_svc::CoordinatorSvc,
    iam_svc::IamSvc,
};
use db::catalog_store::CatalogStore;
use db::iam_store::IamStore;
use elan_common::proto::{
    audit::audit_service_server::AuditServiceServer,
    catalog::catalog_service_server::CatalogServiceServer,
    coordinator::coordinator_service_server::CoordinatorServiceServer,
    iam::iam_service_server::IamServiceServer,
};
use elan_iam::SnapshotIamEngine;
use std::sync::Arc;
use tokio::sync::broadcast;
use tonic::transport::Server;
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

    info!(grpc = %cfg.grpc_addr, http = %cfg.http_addr, "starting elan-central");

    let pool = db::connect(&cfg.database_url).await?;
    let catalog_store = Arc::new(CatalogStore::new(pool.clone()));
    let iam_store = Arc::new(IamStore::new(pool));

    // Bootstrap IAM engine from persisted policies
    let initial_policies = iam_store.list_policies(None).await?;
    let iam_engine = SnapshotIamEngine::new(initial_policies);

    // Broadcast channel for live audit streaming to TUI clients
    let (broadcast_tx, _): (AuditBroadcast, _) = broadcast::channel(256);

    let grpc_addr: std::net::SocketAddr = cfg.grpc_addr.parse()?;
    let http_addr = cfg.http_addr.clone();

    let grpc_server = Server::builder()
        .add_service(CatalogServiceServer::new(CatalogSvc::new(
            catalog_store.clone(),
        )))
        .add_service(CoordinatorServiceServer::new(CoordinatorSvc::new(
            catalog_store,
        )))
        .add_service(IamServiceServer::new(IamSvc::new(
            iam_store.clone(),
            iam_engine,
        )))
        .add_service(AuditServiceServer::new(AuditSvc::new(
            iam_store,
            broadcast_tx,
        )))
        .serve(grpc_addr);

    let http_app = http::health::router();
    let http_listener = tokio::net::TcpListener::bind(&http_addr).await?;
    let http_server = axum::serve(http_listener, http_app);

    info!("elan-central ready");

    tokio::select! {
        result = grpc_server => result?,
        result = http_server => result?,
    }

    Ok(())
}
