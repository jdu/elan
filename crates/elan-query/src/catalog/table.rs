//! DataFusion `TableProvider` that dispatches scans to a remote executor.
//!
//! [`ElanTableProvider`] is a leaf in the DataFusion physical plan.  Its
//! `scan()` method returns a [`RemoteTableScanExec`], which is later
//! intercepted by [`IamFilterRule`](elan_iam::optimizer::IamFilterRule).

use crate::planner::remote_scan::RemoteTableScanExec;
use arrow_schema::SchemaRef;
use async_trait::async_trait;
use datafusion::{
    catalog::Session,
    datasource::{TableProvider, TableType},
    error::Result as DfResult,
    logical_expr::TableProviderFilterPushDown,
    physical_plan::ExecutionPlan,
    prelude::Expr,
};
use elan_common::types::DatasetInfo;
use std::{any::Any, sync::Arc};

/// DataFusion `TableProvider` backed by a remote elan-executor.
///
/// The stored schema may be a placeholder if inference failed at coordinator
/// registration time; the real schema is obtained from the executor at query
/// time via the Arrow IPC response.
#[derive(Debug)]
pub struct ElanTableProvider {
    pub dataset: DatasetInfo,
    schema: SchemaRef,
}

impl ElanTableProvider {
    pub fn new(dataset: DatasetInfo, schema: SchemaRef) -> Self {
        Self { dataset, schema }
    }
}

#[async_trait]
impl TableProvider for ElanTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn table_type(&self) -> TableType {
        TableType::Base
    }

    async fn scan(
        &self,
        _state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> DfResult<Arc<dyn ExecutionPlan>> {
        let projected_schema = if let Some(proj) = projection {
            Arc::new(self.schema.project(proj)?)
        } else {
            self.schema.clone()
        };

        Ok(Arc::new(RemoteTableScanExec::new(
            self.dataset.clone(),
            projected_schema,
            filters.to_vec(),
            limit,
        )))
    }

    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> DfResult<Vec<TableProviderFilterPushDown>> {
        // Report Inexact (not Exact) so DataFusion re-applies filters locally
        // after receiving results; the executor may not support all predicates.
        Ok(vec![
            TableProviderFilterPushDown::Inexact;
            filters.len()
        ])
    }
}
