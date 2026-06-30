//! Uploads local dataset files to an S3-compatible object store (MinIO).
//!
//! The coordinator calls [`upload_datasets`] at startup before registering
//! with elan-central.  Executor replicas — which have no local data mount —
//! can then read the files from the shared bucket.
//!
//! S3 key convention: `{namespace}/{filename}`, e.g. `crm/customers.csv`.
//! Executors are configured with matching `s3://bucket/namespace/filename` paths.

use crate::config::{DatasetConfig, ObjectStoreConfig};
use object_store::{aws::AmazonS3Builder, ObjectStore, ObjectStoreExt, PutPayload, path::Path};
use tracing::{info, warn};

/// Build an S3 client from the coordinator's object store config.
fn build_store(cfg: &ObjectStoreConfig) -> anyhow::Result<impl ObjectStore> {
    Ok(AmazonS3Builder::new()
        .with_endpoint(&cfg.endpoint)
        .with_access_key_id(&cfg.access_key)
        .with_secret_access_key(&cfg.secret_key)
        .with_bucket_name(&cfg.bucket)
        // MinIO requires a region value even though it ignores it.
        .with_region("us-east-1")
        .with_allow_http(cfg.allow_http)
        .build()?)
}

/// Upload every file-backed dataset to the configured object store.
///
/// Non-file datasets (Postgres, Delta) are skipped with a warning.
/// Upload failures are also warned rather than fatal so that registration
/// can still proceed for datasets that are already in the bucket.
pub async fn upload_datasets(
    datasets: &[DatasetConfig],
    cfg: &ObjectStoreConfig,
) -> anyhow::Result<()> {
    let store = build_store(cfg)?;

    for dataset in datasets {
        let (local_path, namespace) = match dataset {
            DatasetConfig::Csv { path, namespace, .. } => (path.as_str(), namespace.as_str()),
            DatasetConfig::Parquet { path, namespace, .. } => (path.as_str(), namespace.as_str()),
            other => {
                warn!(
                    dataset = %other.name(),
                    "skipping object-store upload for non-file dataset type"
                );
                continue;
            }
        };

        // Derive the S3 key from namespace + filename, e.g. crm/customers.csv
        let filename = std::path::Path::new(local_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(dataset.name());
        let s3_key = format!("{}/{}", namespace, filename);

        match upload_file(local_path, &s3_key, &store).await {
            Ok(bytes) => info!(
                dataset = %dataset.name(),
                key = %s3_key,
                bytes,
                "uploaded to object store"
            ),
            Err(e) => warn!(
                dataset = %dataset.name(),
                key = %s3_key,
                error = %e,
                "upload failed — executor may not be able to read this dataset"
            ),
        }
    }

    Ok(())
}

async fn upload_file(
    local_path: &str,
    s3_key: &str,
    store: &impl ObjectStore,
) -> anyhow::Result<usize> {
    let data = tokio::fs::read(local_path).await?;
    let bytes = data.len();
    store
        .put(&Path::from(s3_key), PutPayload::from(data))
        .await?;
    Ok(bytes)
}
