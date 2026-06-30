//! One-shot startup registration with elan-central.
//!
//! Sends a `RegisterRequest` for the coordinator itself, then a
//! `DatasetRegistration` for each dataset in the config.  Schema inference
//! runs as part of [`to_registration`] in the dataset module.

use crate::config::CoordinatorConfig;
use crate::dataset::to_registration;
use elan_common::proto::coordinator::{
    coordinator_service_client::CoordinatorServiceClient, RegisterRequest,
};
use tonic::transport::Channel;
use tracing::info;

/// Register this coordinator and all its datasets with elan-central.
pub async fn register(
    cfg: &CoordinatorConfig,
    client: &mut CoordinatorServiceClient<Channel>,
) -> anyhow::Result<()> {
    info!(
        coordinator_id = %cfg.coordinator.id,
        central = %cfg.central.endpoint,
        "registering with central"
    );

    client
        .register(RegisterRequest {
            coordinator_id: cfg.coordinator.id.clone(),
            environment: cfg.coordinator.environment.clone(),
            hostname: cfg.coordinator.hostname.clone(),
            executor_endpoint: cfg.executor.endpoint.clone(),
        })
        .await?;

    info!("registering {} dataset(s)", cfg.datasets.len());

    for dataset_cfg in &cfg.datasets {
        let registration = to_registration(dataset_cfg, &cfg.coordinator.id).await?;
        info!(
            name = %dataset_cfg.name(),
            namespace = %dataset_cfg.namespace(),
            "registering dataset"
        );
        client.register_dataset(registration).await?;
    }

    info!("registration complete");
    Ok(())
}
