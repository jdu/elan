use arrow_ipc::reader::StreamReader;
use arrow_schema::SchemaRef;
use datafusion::{
    error::{DataFusionError, Result as DfResult},
    execution::TaskContext,
    physical_expr::EquivalenceProperties,
    physical_plan::{
        DisplayAs, DisplayFormatType, ExecutionPlan, Partitioning, PlanProperties,
        SendableRecordBatchStream,
    },
    prelude::Expr,
};
use datafusion_physical_plan::execution_plan::{Boundedness, EmissionType};
use elan_common::types::DatasetInfo;
use std::{any::Any, fmt, sync::Arc};
use tracing::debug;

/// A DataFusion ExecutionPlan node that dispatches query execution to a remote
/// Ballista executor for a specific dataset. The IAM optimizer rule looks for
/// this node type to apply row-level security.
#[derive(Debug)]
pub struct RemoteTableScanExec {
    dataset: DatasetInfo,
    schema: SchemaRef,
    pushed_filters: Vec<Expr>,
    limit: Option<usize>,
    properties: Arc<PlanProperties>,
}

impl RemoteTableScanExec {
    pub fn dataset_namespace(&self) -> &str {
        &self.dataset.namespace
    }

    pub fn dataset_name(&self) -> &str {
        &self.dataset.name
    }

    pub fn new(
        dataset: DatasetInfo,
        schema: SchemaRef,
        pushed_filters: Vec<Expr>,
        limit: Option<usize>,
    ) -> Self {
        let properties = Arc::new(PlanProperties::new(
            EquivalenceProperties::new(schema.clone()),
            Partitioning::UnknownPartitioning(1),
            EmissionType::Final,
            Boundedness::Bounded,
        ));
        Self {
            dataset,
            schema,
            pushed_filters,
            limit,
            properties,
        }
    }

    fn build_sql(&self) -> String {
        // Always SELECT * — the executor has the authoritative schema.
        // The stored schema in elan-central may be stale or a placeholder;
        // elan-query handles projection locally after receiving the batches.
        let table = &self.dataset.name;
        let mut sql = format!("SELECT * FROM \"{table}\"");

        // Filters have already been validated by the IAM optimizer at this point.
        // We convert them to SQL strings for the remote executor.
        if !self.pushed_filters.is_empty() {
            let filter_strs: Vec<String> = self
                .pushed_filters
                .iter()
                .filter_map(|e| expr_to_sql(e))
                .collect();
            if !filter_strs.is_empty() {
                sql.push_str(&format!(" WHERE {}", filter_strs.join(" AND ")));
            }
        }

        if let Some(lim) = self.limit {
            sql.push_str(&format!(" LIMIT {lim}"));
        }

        sql
    }
}

impl DisplayAs for RemoteTableScanExec {
    fn fmt_as(&self, _t: DisplayFormatType, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "RemoteTableScanExec: {}.{} @ {}",
            self.dataset.namespace, self.dataset.name, self.dataset.executor_endpoint
        )
    }
}

impl ExecutionPlan for RemoteTableScanExec {
    fn name(&self) -> &str {
        "RemoteTableScanExec"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn properties(&self) -> &Arc<PlanProperties> {
        &self.properties
    }

    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        vec![]
    }

    fn with_new_children(
        self: Arc<Self>,
        _children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> DfResult<Arc<dyn ExecutionPlan>> {
        Ok(self)
    }

    fn execute(
        &self,
        _partition: usize,
        _context: Arc<TaskContext>,
    ) -> DfResult<SendableRecordBatchStream> {
        let sql = self.build_sql();
        let endpoint = self.dataset.executor_endpoint.clone();
        let schema = self.schema.clone();

        debug!(
            sql = %sql,
            executor = %endpoint,
            "dispatching to executor HTTP SQL service"
        );

        // Parse host and port from endpoint (format: "host:port"), then derive
        // the HTTP SQL service port as Ballista port + 1.
        let (host, ballista_port) = parse_endpoint(&endpoint)?;
        let http_port = ballista_port + 1;

        let batches = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let url = format!("http://{host}:{http_port}/sql");
                let client = reqwest::Client::new();
                let resp = client
                    .post(&url)
                    .body(sql)
                    .send()
                    .await
                    .map_err(|e| DataFusionError::External(Box::new(e)))?;

                if !resp.status().is_success() {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    return Err(DataFusionError::External(
                        format!("executor SQL service error ({status}): {body}").into(),
                    ));
                }

                let ipc_bytes = resp
                    .bytes()
                    .await
                    .map_err(|e| DataFusionError::External(Box::new(e)))?;

                let cursor = std::io::Cursor::new(ipc_bytes);
                let reader = StreamReader::try_new(cursor, None)
                    .map_err(|e| DataFusionError::External(Box::new(e)))?;

                let mut batches = vec![];
                for batch in reader {
                    batches.push(batch.map_err(|e| DataFusionError::External(Box::new(e)))?);
                }
                Ok(batches)
            })
        })?;

        // Use the schema from the returned batches — the stored schema in elan-central
        // may be a placeholder. Fall back to the stored schema if batches are empty.
        let actual_schema = batches
            .first()
            .map(|b| b.schema())
            .unwrap_or(schema);

        let stream = datafusion::physical_plan::memory::MemoryStream::try_new(
            batches,
            actual_schema,
            None,
        )?;

        Ok(Box::pin(stream))
    }
}

fn parse_endpoint(endpoint: &str) -> DfResult<(String, u16)> {
    let parts: Vec<&str> = endpoint.rsplitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(DataFusionError::Plan(format!(
            "invalid executor endpoint: {endpoint}"
        )));
    }
    let port: u16 = parts[0]
        .parse()
        .map_err(|e| DataFusionError::Plan(format!("invalid port in endpoint: {e}")))?;
    Ok((parts[1].to_string(), port))
}

/// Best-effort Expr -> SQL string conversion for simple predicates.
fn expr_to_sql(expr: &Expr) -> Option<String> {
    use datafusion::logical_expr::Operator;
    use datafusion::prelude::*;

    match expr {
        Expr::BinaryExpr(be) => {
            let left = expr_to_sql(&be.left)?;
            let right = expr_to_sql(&be.right)?;
            let op = match be.op {
                Operator::Eq => "=",
                Operator::NotEq => "!=",
                Operator::Lt => "<",
                Operator::LtEq => "<=",
                Operator::Gt => ">",
                Operator::GtEq => ">=",
                Operator::And => "AND",
                Operator::Or => "OR",
                _ => return None,
            };
            Some(format!("({left} {op} {right})"))
        }
        Expr::Column(col) => Some(format!("\"{}\"", col.name)),
        Expr::Literal(scalar, _metadata) => Some(scalar.to_string()),
        Expr::Not(inner) => {
            let inner_sql = expr_to_sql(inner)?;
            Some(format!("NOT ({inner_sql})"))
        }
        _ => None,
    }
}
