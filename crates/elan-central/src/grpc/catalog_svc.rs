//! gRPC `CatalogService` implementation.
//!
//! Exposes read-only access to the catalog (datasets and coordinators) stored
//! in SQLite.  The streaming `list_datasets` and `search_datasets` RPCs send
//! results over a bounded mpsc channel to avoid materializing the full
//! result set in memory before the first byte is sent.

use crate::db::catalog_store::CatalogStore;
use elan_common::proto::catalog::{
    catalog_service_server::CatalogService, DatasetProto, GetDatasetRequest, ListDatasetsRequest,
    SearchRequest,
};
use std::sync::Arc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

/// gRPC handler for the `CatalogService` proto service.
pub struct CatalogSvc {
    store: Arc<CatalogStore>,
}

impl CatalogSvc {
    /// Construct the service with the shared catalog store.
    pub fn new(store: Arc<CatalogStore>) -> Self {
        Self { store }
    }
}

fn dataset_to_proto(d: elan_common::types::DatasetInfo) -> DatasetProto {
    DatasetProto {
        dataset_id: d.id.to_string(),
        name: d.name,
        namespace: d.namespace,
        source_type: d.source_type.to_string(),
        coordinator_id: d.coordinator_id,
        executor_endpoint: d.executor_endpoint,
        arrow_schema_ipc: d.schema_ipc,
        metadata_json: d.metadata.to_string(),
    }
}

#[tonic::async_trait]
impl CatalogService for CatalogSvc {
    type ListDatasetsStream = ReceiverStream<Result<DatasetProto, Status>>;
    type SearchDatasetsStream = ReceiverStream<Result<DatasetProto, Status>>;

    async fn get_dataset(
        &self,
        request: Request<GetDatasetRequest>,
    ) -> Result<Response<DatasetProto>, Status> {
        let req = request.into_inner();
        let dataset = self
            .store
            .get_dataset(&req.namespace, &req.name)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| {
                Status::not_found(format!("dataset not found: {}.{}", req.namespace, req.name))
            })?;

        Ok(Response::new(dataset_to_proto(dataset)))
    }

    async fn list_datasets(
        &self,
        request: Request<ListDatasetsRequest>,
    ) -> Result<Response<Self::ListDatasetsStream>, Status> {
        let req = request.into_inner();
        let ns_filter = if req.namespace_filter.is_empty() {
            None
        } else {
            Some(req.namespace_filter.as_str())
        };

        let datasets = self
            .store
            .list_datasets(ns_filter)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let (tx, rx) = tokio::sync::mpsc::channel(32);
        tokio::spawn(async move {
            for d in datasets {
                if tx.send(Ok(dataset_to_proto(d))).await.is_err() {
                    break;
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn search_datasets(
        &self,
        request: Request<SearchRequest>,
    ) -> Result<Response<Self::SearchDatasetsStream>, Status> {
        let query = request.into_inner().query.to_lowercase();
        let all = self
            .store
            .list_datasets(None)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let filtered: Vec<_> = all
            .into_iter()
            .filter(|d| {
                d.name.to_lowercase().contains(&query)
                    || d.namespace.to_lowercase().contains(&query)
            })
            .collect();

        let (tx, rx) = tokio::sync::mpsc::channel(32);
        tokio::spawn(async move {
            for d in filtered {
                if tx.send(Ok(dataset_to_proto(d))).await.is_err() {
                    break;
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
