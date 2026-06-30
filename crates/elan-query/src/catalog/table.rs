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
        Ok(vec![
            TableProviderFilterPushDown::Inexact;
            filters.len()
        ])
    }
}
