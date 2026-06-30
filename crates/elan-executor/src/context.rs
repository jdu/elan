//! DataFusion context setup for elan-executor.
//!
//! [`build_providers`] builds a temporary `SessionContext`, optionally
//! registering an S3-compatible object store (MinIO) so that dataset paths
//! can be `s3://bucket/key` URLs rather than local filesystem paths.  Multiple
//! executor replicas pointing at the same object store can then all serve
//! the same data, enabling horizontal scaling behind a load balancer.

use crate::config::{DatasetMount, ExecutorConfig, ObjectStoreConfig};
use datafusion::catalog::TableProvider;
use datafusion::prelude::*;
use object_store::aws::AmazonS3Builder;
use std::sync::Arc;
use tracing::info;
use url::Url;

/// Build and return `TableProvider` handles for all configured datasets.
///
/// If `[object_store]` is configured, an S3 store is registered with DataFusion
/// first so that `s3://` paths resolve correctly.  Providers are then extracted
/// via `deregister_table` (sync) so they can be handed to the HTTP SQL service
/// state, which re-registers them for each request.
pub async fn build_providers(
    cfg: &ExecutorConfig,
) -> anyhow::Result<Vec<(String, Arc<dyn TableProvider>)>> {
    let ctx = SessionContext::new();

    if let Some(s3_cfg) = &cfg.object_store {
        register_object_store(&ctx, s3_cfg)?;
    }

    for mount in &cfg.datasets {
        match mount {
            DatasetMount::Parquet { table_name, path } => {
                info!(table = %table_name, path = %path, "registering parquet dataset");
                ctx.register_parquet(table_name, path, ParquetReadOptions::default())
                    .await?;
            }
            DatasetMount::Csv { table_name, path, has_header } => {
                info!(table = %table_name, path = %path, "registering csv dataset");
                ctx.register_csv(
                    table_name,
                    path,
                    CsvReadOptions::default().has_header(*has_header),
                )
                .await?;
            }
        }
    }

    // Deregister each table to recover the Arc<dyn TableProvider> (sync operation).
    // The provider retains its reference to the object store, so S3 reads still
    // work after the temporary context is dropped.
    let mut providers = Vec::new();
    for mount in &cfg.datasets {
        let name = mount.table_name();
        if let Some(provider) = ctx.deregister_table(name)? {
            info!(table = %name, "provider ready");
            providers.push((name.to_string(), provider));
        }
    }

    Ok(providers)
}

/// Register an S3-compatible object store with the DataFusion context.
/// Public so `sql_service` can call it when building per-request sessions.
///
/// The store is keyed by `s3://{bucket}/` so DataFusion resolves any path
/// that begins with that scheme and bucket name through this store.
pub fn register_object_store(ctx: &SessionContext, cfg: &ObjectStoreConfig) -> anyhow::Result<()> {
    let store = AmazonS3Builder::new()
        .with_endpoint(&cfg.endpoint)
        .with_access_key_id(&cfg.access_key)
        .with_secret_access_key(&cfg.secret_key)
        .with_bucket_name(&cfg.bucket)
        // MinIO requires a region value even though it ignores it.
        .with_region("us-east-1")
        // Allow plain HTTP for local MinIO; disable in production.
        .with_allow_http(cfg.allow_http)
        .build()?;

    let url = Url::parse(&format!("s3://{}/", cfg.bucket))?;
    ctx.register_object_store(&url, Arc::new(store));
    info!(bucket = %cfg.bucket, endpoint = %cfg.endpoint, "S3 object store registered");
    Ok(())
}
