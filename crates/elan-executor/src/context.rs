use crate::config::{DatasetMount, ExecutorConfig};
use datafusion::catalog::TableProvider;
use datafusion::prelude::*;
use std::sync::Arc;
use tracing::info;

/// Build a DataFusion context with all configured datasets registered, then
/// extract the providers so they can be re-registered synchronously into
/// Ballista session contexts (which require a sync SessionBuilder).
pub async fn build_providers(
    cfg: &ExecutorConfig,
) -> anyhow::Result<Vec<(String, Arc<dyn TableProvider>)>> {
    let ctx = SessionContext::new();

    for mount in &cfg.datasets {
        match mount {
            DatasetMount::Parquet { table_name, path } => {
                info!(table = %table_name, path = %path, "loading parquet schema");
                ctx.register_parquet(table_name, path, ParquetReadOptions::default())
                    .await?;
            }
            DatasetMount::Csv {
                table_name,
                path,
                has_header,
            } => {
                info!(table = %table_name, path = %path, "loading csv schema");
                ctx.register_csv(
                    table_name,
                    path,
                    CsvReadOptions::default().has_header(*has_header),
                )
                .await?;
            }
        }
    }

    // Deregister each table to get the Arc<dyn TableProvider> back (sync operation).
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
