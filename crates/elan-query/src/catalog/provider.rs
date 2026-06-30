use crate::catalog::table::ElanTableProvider;
use arrow_ipc::reader::StreamReader;
use arrow_schema::SchemaRef;
use async_trait::async_trait;
use datafusion::catalog::{CatalogProvider, SchemaProvider};
use datafusion::error::{DataFusionError, Result as DfResult};
use elan_common::proto::catalog::{
    catalog_service_client::CatalogServiceClient, ListDatasetsRequest,
};
use elan_common::types::DatasetInfo;
use elan_iam::{catalog_filter, IamEngine, Subject};
use std::any::Any;
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Arc, RwLock};
use tokio::runtime::Handle;
use tonic::transport::Channel;
use tracing::{debug, warn};

#[derive(Debug)]
pub struct ElanCatalogProvider {
    schemas: Arc<RwLock<HashMap<String, Arc<ElanSchemaProvider>>>>,
}

impl ElanCatalogProvider {
    pub fn new(schemas: HashMap<String, Arc<ElanSchemaProvider>>) -> Arc<Self> {
        Arc::new(Self {
            schemas: Arc::new(RwLock::new(schemas)),
        })
    }
}

impl CatalogProvider for ElanCatalogProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema_names(&self) -> Vec<String> {
        self.schemas.read().unwrap().keys().cloned().collect()
    }

    fn schema(&self, name: &str) -> Option<Arc<dyn SchemaProvider>> {
        self.schemas
            .read()
            .unwrap()
            .get(name)
            .map(|s| Arc::clone(s) as Arc<dyn SchemaProvider>)
    }

    fn register_schema(
        &self,
        name: &str,
        schema: Arc<dyn SchemaProvider>,
    ) -> DfResult<Option<Arc<dyn SchemaProvider>>> {
        // Dynamically add a namespace
        let provider = schema
            .as_any()
            .downcast_ref::<ElanSchemaProvider>()
            .ok_or_else(|| DataFusionError::Plan("expected ElanSchemaProvider".into()))?;
        let mut schemas = self.schemas.write().unwrap();
        let prev = schemas.insert(name.to_string(), Arc::new(provider.clone()));
        Ok(prev.map(|p| p as Arc<dyn SchemaProvider>))
    }
}

#[derive(Clone, Debug)]
pub struct ElanSchemaProvider {
    pub namespace: String,
    datasets: Arc<RwLock<HashMap<String, Arc<ElanTableProvider>>>>,
    central_client: Arc<tokio::sync::Mutex<CatalogServiceClient<Channel>>>,
    iam_engine: Arc<dyn IamEngine>,
    subject: Subject,
}

impl ElanSchemaProvider {
    pub fn new(
        namespace: String,
        datasets: Vec<DatasetInfo>,
        central_client: Arc<tokio::sync::Mutex<CatalogServiceClient<Channel>>>,
        iam_engine: Arc<dyn IamEngine>,
        subject: Subject,
    ) -> Self {
        let mut map = HashMap::new();
        for ds in datasets {
            let schema = decode_schema(&ds.schema_ipc).unwrap_or_else(|e| {
                warn!(error = %e, dataset = %ds.name, "failed to decode schema");
                Arc::new(arrow_schema::Schema::empty())
            });
            map.insert(
                ds.name.clone(),
                Arc::new(ElanTableProvider::new(ds, schema)),
            );
        }
        Self {
            namespace,
            datasets: Arc::new(RwLock::new(map)),
            central_client,
            iam_engine,
            subject,
        }
    }
}

#[async_trait]
impl SchemaProvider for ElanSchemaProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn table_names(&self) -> Vec<String> {
        let datasets = self.datasets.read().unwrap();
        datasets
            .keys()
            .filter(|name| {
                catalog_filter::is_visible(
                    self.iam_engine.as_ref(),
                    &self.subject,
                    &self.namespace,
                    name,
                )
            })
            .cloned()
            .collect()
    }

    async fn table(&self, name: &str) -> DfResult<Option<Arc<dyn datafusion::datasource::TableProvider>>> {
        // IAM catalog filter — silently hide if denied
        if !catalog_filter::is_visible(
            self.iam_engine.as_ref(),
            &self.subject,
            &self.namespace,
            name,
        ) {
            debug!(
                user = %self.subject.user_id,
                namespace = %self.namespace,
                table = %name,
                "catalog filter: hidden by IAM"
            );
            return Ok(None);
        }

        // Check local cache first
        if let Some(provider) = self.datasets.read().unwrap().get(name) {
            return Ok(Some(Arc::clone(provider) as Arc<dyn datafusion::datasource::TableProvider>));
        }

        // Fetch from central catalog on cache miss
        let mut client = self.central_client.lock().await;
        let resp = client
            .get_dataset(elan_common::proto::catalog::GetDatasetRequest {
                namespace: self.namespace.clone(),
                name: name.to_string(),
            })
            .await;

        match resp {
            Ok(response) => {
                let proto = response.into_inner();
                let ds = DatasetInfo {
                    id: uuid::Uuid::parse_str(&proto.dataset_id)
                        .map_err(|e| DataFusionError::External(Box::new(e)))?,
                    name: proto.name.clone(),
                    namespace: proto.namespace.clone(),
                    source_type: elan_common::types::catalog::SourceType::try_from(
                        proto.source_type.as_str(),
                    )
                    .map_err(|e| DataFusionError::External(Box::new(e)))?,
                    coordinator_id: proto.coordinator_id,
                    executor_endpoint: proto.executor_endpoint,
                    schema_ipc: proto.arrow_schema_ipc,
                    metadata: serde_json::from_str(&proto.metadata_json).unwrap_or_default(),
                };

                let schema = decode_schema(&ds.schema_ipc).unwrap_or_else(|e| {
                    warn!(error = %e, "failed to decode schema from central");
                    Arc::new(arrow_schema::Schema::empty())
                });

                let provider = Arc::new(ElanTableProvider::new(ds, schema));
                self.datasets
                    .write()
                    .unwrap()
                    .insert(name.to_string(), Arc::clone(&provider));

                Ok(Some(provider as Arc<dyn datafusion::datasource::TableProvider>))
            }
            Err(status) if status.code() == tonic::Code::NotFound => Ok(None),
            Err(e) => Err(DataFusionError::External(Box::new(e))),
        }
    }

    fn table_exist(&self, name: &str) -> bool {
        self.datasets.read().unwrap().contains_key(name)
    }
}

fn decode_schema(ipc_bytes: &[u8]) -> anyhow::Result<SchemaRef> {
    if ipc_bytes.is_empty()
        || ipc_bytes == b"\x00\x00\x00\x00"
        || ipc_bytes.len() < 8
    {
        return Ok(Arc::new(arrow_schema::Schema::empty()));
    }
    let cursor = Cursor::new(ipc_bytes);
    let reader = StreamReader::try_new(cursor, None)?;
    Ok(reader.schema())
}
