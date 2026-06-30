//! elan-coordinator: remote-side agent that registers local data with elan-central.
//!
//! On startup the coordinator:
//! 1. Reads its TOML config (datasets, central endpoint, executor endpoint).
//! 2. If `[object_store]` is configured, uploads all local dataset files to the
//!    shared object store (MinIO) so executor replicas can read them.
//! 3. Registers itself and all configured datasets with elan-central via gRPC.
//! 4. Starts an HTTP server (`/health`, `/auth/check`) on the configured port.
//! 5. Runs a continuous heartbeat loop to keep its record alive in elan-central.

mod config;
mod dataset;
mod heartbeat;
mod http;
mod registration;
mod upload;

use elan_common::proto::coordinator::coordinator_service_client::CoordinatorServiceClient;
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
        .map(|w| w[1].clone())
        .unwrap_or_else(|| "config/coordinator.toml".to_string());

    let cfg = config::load(&config_path)?;
    info!(coordinator_id = %cfg.coordinator.id, "starting elan-coordinator");

    // Upload dataset files to shared object storage before registering, so
    // executor replicas can read them without needing a local data volume.
    if let Some(s3_cfg) = &cfg.object_store {
        info!(endpoint = %s3_cfg.endpoint, bucket = %s3_cfg.bucket, "uploading datasets to object store");
        upload::upload_datasets(&cfg.datasets, s3_cfg).await?;
    }

    // Connect to central gRPC
    let channel = tonic::transport::Channel::from_shared(cfg.central.endpoint.clone())?
        .connect()
        .await?;
    let mut client = CoordinatorServiceClient::new(channel.clone());

    // Register self + datasets
    registration::register(&cfg, &mut client).await?;

    // Start HTTP server for auth checks
    let http_app = http::health::router();
    let http_addr: std::net::SocketAddr = cfg.http.addr.parse()?;
    let http_server = tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(http_addr).await.unwrap();
        axum::serve(listener, http_app).await.unwrap();
    });

    // Run heartbeat in background
    let hb_client = CoordinatorServiceClient::new(channel);
    let coordinator_id = cfg.coordinator.id.clone();
    let hb = tokio::spawn(async move {
        heartbeat::run(coordinator_id, hb_client).await;
    });

    info!("elan-coordinator running");

    tokio::select! {
        _ = hb => {},
        _ = http_server => {},
    }

    Ok(())
}
