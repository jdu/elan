use crate::db::catalog_store::CatalogStore;
use elan_common::proto::coordinator::{
    coordinator_service_server::CoordinatorService, DatasetRegistration,
    DatasetRegistrationResponse, HeartbeatRequest, HeartbeatResponse, RegisterRequest,
    RegisterResponse, UnregisterDatasetRequest,
};
use prost_types::Timestamp;
use std::sync::Arc;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tonic::{Request, Response, Status, Streaming};
use tracing::{info, warn};

pub struct CoordinatorSvc {
    store: Arc<CatalogStore>,
}

impl CoordinatorSvc {
    pub fn new(store: Arc<CatalogStore>) -> Self {
        Self { store }
    }
}

#[tonic::async_trait]
impl CoordinatorService for CoordinatorSvc {
    type HeartbeatStream = ReceiverStream<Result<HeartbeatResponse, Status>>;

    async fn register(
        &self,
        request: Request<RegisterRequest>,
    ) -> Result<Response<RegisterResponse>, Status> {
        let req = request.into_inner();
        info!(
            coordinator_id = %req.coordinator_id,
            environment = %req.environment,
            executor = %req.executor_endpoint,
            "coordinator registered"
        );

        self.store
            .upsert_coordinator(
                &req.coordinator_id,
                &req.environment,
                &req.hostname,
                &req.executor_endpoint,
            )
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(RegisterResponse { accepted: true }))
    }

    async fn heartbeat(
        &self,
        request: Request<Streaming<HeartbeatRequest>>,
    ) -> Result<Response<Self::HeartbeatStream>, Status> {
        let mut stream = request.into_inner();
        let store = self.store.clone();

        let (tx, rx) = tokio::sync::mpsc::channel(8);

        tokio::spawn(async move {
            while let Some(result) = stream.next().await {
                match result {
                    Ok(hb) => {
                        if let Err(e) = store.heartbeat(&hb.coordinator_id).await {
                            warn!(error = %e, "heartbeat store error");
                        }
                        let _ = tx.send(Ok(HeartbeatResponse { alive: true })).await;
                    }
                    Err(e) => {
                        warn!(error = %e, "heartbeat stream error");
                        break;
                    }
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn register_dataset(
        &self,
        request: Request<DatasetRegistration>,
    ) -> Result<Response<DatasetRegistrationResponse>, Status> {
        let req = request.into_inner();
        info!(
            coordinator_id = %req.coordinator_id,
            namespace = %req.namespace,
            name = %req.name,
            "dataset registered"
        );

        self.store
            .upsert_dataset(
                &req.coordinator_id,
                &req.dataset_id,
                &req.name,
                &req.namespace,
                &req.source_type,
                // Executor endpoint comes from the coordinator record
                &req.coordinator_id, // placeholder — resolved in store via JOIN
                &req.arrow_schema_ipc,
                if req.metadata_json.is_empty() {
                    None
                } else {
                    Some(&req.metadata_json)
                },
            )
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(DatasetRegistrationResponse { accepted: true }))
    }

    async fn unregister_dataset(
        &self,
        request: Request<UnregisterDatasetRequest>,
    ) -> Result<Response<()>, Status> {
        let req = request.into_inner();
        self.store
            .deactivate_dataset(&req.dataset_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(()))
    }
}
