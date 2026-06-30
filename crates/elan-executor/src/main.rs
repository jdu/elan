//! elan-executor: local query execution engine deployed alongside the coordinator.
//!
//! Starts two services on the configured host:
//! - **Ballista scheduler + executor** on `bind_port`: handles distributed plan
//!   execution.  The Ballista executor is kept alive for API-level compatibility
//!   but is **never called** for actual queries — elan-query bypasses it via the
//!   HTTP SQL service.  Removing Ballista would require Cargo-level changes.
//! - **HTTP SQL service** on `bind_port + 1` (i.e. port 50056): `POST /sql`
//!   accepts plain-text SQL and returns Arrow IPC bytes.  This is the endpoint
//!   that `RemoteTableScanExec` in elan-query calls.
//!
//! `ctrl_c` is required to keep the process alive because
//! `new_standalone_executor()` spawns background tasks and returns immediately.

mod auth;
mod config;
mod context;
mod sql_service;

use ballista_core::config::TaskSchedulingPolicy;
use ballista_core::extension::SessionConfigExt;
use ballista_core::serde::protobuf::scheduler_grpc_client::SchedulerGrpcClient;
use ballista_core::serde::BallistaCodec;
use ballista_core::utils::{
    GrpcServerConfig, create_grpc_server, default_config_producer, default_session_builder,
};
use ballista_executor::new_standalone_executor;
use ballista_scheduler::cluster::BallistaCluster;
use ballista_scheduler::config::SchedulerConfig;
use ballista_scheduler::metrics::default_metrics_collector;
use ballista_scheduler::scheduler_server::{SchedulerServer, SessionBuilder};
use axum;
use datafusion::catalog::TableProvider;
use datafusion::error::DataFusionError;
use datafusion::prelude::{SessionConfig, SessionContext};
use datafusion_proto::protobuf::{LogicalPlanNode, PhysicalPlanNode};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
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

    info!(
        host = %cfg.bind_host,
        port = cfg.bind_port,
        datasets = cfg.datasets.len(),
        "starting elan-executor (Ballista standalone)"
    );

    // Pre-build table providers once at startup (async schema inference).
    // These are then registered synchronously into every Ballista session context,
    // avoiding the need for block_in_place inside the sync SessionBuilder Fn.
    let providers: Vec<(String, Arc<dyn TableProvider>)> =
        context::build_providers(&cfg).await?;
    info!(count = providers.len(), "dataset providers ready");

    // Wrap in Arc so the closure can be cloned cheaply per session.
    let providers: Arc<Vec<(String, Arc<dyn TableProvider>)>> = Arc::new(providers);

    // Start HTTP SQL service on bind_port + 1 so elan-query can send SQL directly
    // and get Arrow IPC results back, bypassing Ballista's logical plan serialization.
    let http_port = cfg.bind_port + 1;
    let sql_state = sql_service::SqlServiceState::new(Arc::clone(&providers));
    let http_addr: std::net::SocketAddr = format!("{}:{}", cfg.bind_host, http_port).parse()?;
    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(http_addr).await.expect("sql service bind");
        info!(addr = %http_addr, "SQL HTTP service listening");
        axum::serve(listener, sql_service::router(sql_state))
            .await
            .expect("sql service failed");
    });

    let session_builder: SessionBuilder = Arc::new(move |session_config: SessionConfig| {
        let base_state = default_session_builder(session_config)?;
        let ctx = SessionContext::new_with_state(base_state);
        for (name, provider) in providers.iter() {
            ctx.register_table(name.as_str(), Arc::clone(provider))
                .map_err(|e| DataFusionError::External(Box::new(e)))?;
        }
        Ok(ctx.state())
    });

    // Start Ballista scheduler on the configured port so elan-query can connect via
    // df://elan-executor:{bind_port}
    let scheduler_addr =
        start_scheduler_on_port(&cfg.bind_host, cfg.bind_port, session_builder).await?;
    info!(scheduler = %scheduler_addr, "Ballista scheduler started");

    // Connect to the scheduler and start the executor
    let channel = tonic::transport::Channel::from_shared(format!("http://{scheduler_addr}"))?
        .connect()
        .await?;
    let scheduler_client = SchedulerGrpcClient::new(channel);

    info!("Ballista executor starting");
    new_standalone_executor(scheduler_client, 1, BallistaCodec::default())
        .await
        .map_err(|e| anyhow::anyhow!("executor error: {e}"))?;

    info!("elan-executor ready");

    // new_standalone_executor spawns background tasks and returns Ok(()) immediately.
    // Keep the process alive until SIGINT/SIGTERM.
    tokio::signal::ctrl_c().await?;
    info!("elan-executor shutting down");
    Ok(())
}

async fn start_scheduler_on_port(
    host: &str,
    port: u16,
    session_builder: SessionBuilder,
) -> anyhow::Result<SocketAddr> {
    let bind_addr = format!("{host}:{port}");
    let scheduler_name = format!("{host}:{port}");

    let cluster = BallistaCluster::new_memory(
        &scheduler_name,
        session_builder,
        Arc::new(default_config_producer),
    );

    let metrics_collector = default_metrics_collector()
        .map_err(|e| anyhow::anyhow!("metrics error: {e}"))?;

    let mut scheduler_server: SchedulerServer<LogicalPlanNode, PhysicalPlanNode> =
        SchedulerServer::new(
            scheduler_name,
            cluster,
            BallistaCodec::default(),
            Arc::new(
                SchedulerConfig::default()
                    .with_scheduler_policy(TaskSchedulingPolicy::PullStaged),
            ),
            metrics_collector,
        );

    scheduler_server
        .init()
        .await
        .map_err(|e| anyhow::anyhow!("scheduler init error: {e}"))?;

    let config = default_config_producer();
    let server = ballista_core::serde::protobuf::scheduler_grpc_server::SchedulerGrpcServer::new(
        scheduler_server,
    )
    .max_decoding_message_size(config.ballista_grpc_client_max_message_size())
    .max_encoding_message_size(config.ballista_grpc_client_max_message_size());

    let listener = TcpListener::bind(&bind_addr).await?;
    let addr = listener.local_addr()?;

    tokio::spawn(
        create_grpc_server(&GrpcServerConfig::default())
            .add_service(server)
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener)),
    );

    Ok(addr)
}
