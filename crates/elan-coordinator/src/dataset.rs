use crate::config::DatasetConfig;
use arrow_ipc::writer::{IpcWriteOptions, StreamWriter};
use arrow_schema::{DataType, Field, Schema};
use datafusion::prelude::*;
use elan_common::proto::coordinator::DatasetRegistration;
use std::io::Cursor;
use uuid::Uuid;

/// Build a DatasetRegistration proto from coordinator config, inferring the
/// real Arrow schema from the source file so elan-central's catalog is accurate.
pub async fn to_registration(
    cfg: &DatasetConfig,
    coordinator_id: &str,
) -> anyhow::Result<DatasetRegistration> {
    let dataset_id = Uuid::new_v4().to_string();
    let schema = infer_schema(cfg).await;
    let schema_ipc = serialize_schema(&schema)?;

    Ok(DatasetRegistration {
        coordinator_id: coordinator_id.to_string(),
        dataset_id,
        name: cfg.name().to_string(),
        namespace: cfg.namespace().to_string(),
        source_type: cfg.source_type().to_string(),
        arrow_schema_ipc: schema_ipc,
        metadata_json: cfg.metadata_json().to_string(),
    })
}

async fn infer_schema(cfg: &DatasetConfig) -> Schema {
    match cfg {
        DatasetConfig::Csv { path, has_header, delimiter, .. } => {
            infer_csv_schema(path, *has_header, *delimiter as u8)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!(path = %path, error = %e, "CSV schema inference failed, using placeholder");
                    placeholder_schema()
                })
        }
        DatasetConfig::Parquet { path, .. } => {
            infer_parquet_schema(path)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!(path = %path, error = %e, "Parquet schema inference failed, using placeholder");
                    placeholder_schema()
                })
        }
        // Postgres and Delta schema inference not yet implemented
        _ => placeholder_schema(),
    }
}

async fn infer_csv_schema(path: &str, has_header: bool, delimiter: u8) -> anyhow::Result<Schema> {
    let ctx = SessionContext::new();
    ctx.register_csv(
        "_schema_probe",
        path,
        CsvReadOptions::default()
            .has_header(has_header)
            .delimiter(delimiter),
    )
    .await?;
    let provider = ctx.table_provider("_schema_probe").await?;
    Ok(provider.schema().as_ref().clone())
}

async fn infer_parquet_schema(path: &str) -> anyhow::Result<Schema> {
    let ctx = SessionContext::new();
    ctx.register_parquet("_schema_probe", path, ParquetReadOptions::default())
        .await?;
    let provider = ctx.table_provider("_schema_probe").await?;
    Ok(provider.schema().as_ref().clone())
}

fn placeholder_schema() -> Schema {
    Schema::new(vec![Field::new("_placeholder", DataType::Utf8, true)])
}

fn serialize_schema(schema: &Schema) -> anyhow::Result<Vec<u8>> {
    let mut buf = Vec::new();
    let opts = IpcWriteOptions::default();
    let mut writer =
        StreamWriter::try_new_with_options(Cursor::new(&mut buf), schema, opts)?;
    writer.finish()?;
    Ok(buf)
}
