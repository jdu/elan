use crate::{AccessDecision, IamEngine, ResourceId, Subject};
use datafusion::{
    config::ConfigOptions,
    physical_optimizer::PhysicalOptimizerRule,
    physical_plan::{empty::EmptyExec, ExecutionPlan},
};
use std::sync::Arc;
use tracing::warn;

/// Trait that RemoteTableScanExec in elan-query implements so the optimizer
/// can extract dataset identity for IAM checks. Identified via `Any::downcast_ref`
/// on the concrete type using a TypeId registered in the plan's `as_any()`.
pub trait RemoteScan {
    fn dataset_namespace(&self) -> &str;
    fn dataset_name(&self) -> &str;
}

/// Physical optimizer rule that enforces IAM on all remote table scans.
/// Registered into the SessionContext via `add_physical_optimizer_rule`.
pub struct IamFilterRule {
    engine: Arc<dyn IamEngine>,
    subject: Subject,
    /// Name of the ExecutionPlan node to intercept (checked via plan.name())
    remote_scan_node_name: &'static str,
    /// Callback that extracts (namespace, dataset_name) from a matched plan node.
    extract: Arc<dyn Fn(&dyn ExecutionPlan) -> Option<(String, String)> + Send + Sync>,
}

impl IamFilterRule {
    pub fn new(engine: Arc<dyn IamEngine>, subject: Subject) -> Self {
        Self {
            engine,
            subject,
            remote_scan_node_name: "RemoteTableScanExec",
            extract: Arc::new(|_| None), // overridden by with_extractor
        }
    }

    /// Register the function that extracts (namespace, name) from a RemoteTableScanExec.
    /// Called from elan-query during session construction so the IAM layer doesn't
    /// need to depend on elan-query.
    pub fn with_extractor(
        mut self,
        extractor: impl Fn(&dyn ExecutionPlan) -> Option<(String, String)> + Send + Sync + 'static,
    ) -> Self {
        self.extract = Arc::new(extractor);
        self
    }
}

impl std::fmt::Debug for IamFilterRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "IamFilterRule(user={})", self.subject.user_id)
    }
}

impl PhysicalOptimizerRule for IamFilterRule {
    fn optimize(
        &self,
        plan: Arc<dyn ExecutionPlan>,
        _config: &ConfigOptions,
    ) -> datafusion::error::Result<Arc<dyn ExecutionPlan>> {
        apply_iam(plan, &self.engine, &self.subject, &self.remote_scan_node_name, &self.extract)
    }

    fn name(&self) -> &str {
        "IamFilterRule"
    }

    fn schema_check(&self) -> bool {
        true
    }
}

fn apply_iam(
    plan: Arc<dyn ExecutionPlan>,
    engine: &Arc<dyn IamEngine>,
    subject: &Subject,
    node_name: &str,
    extract: &Arc<dyn Fn(&dyn ExecutionPlan) -> Option<(String, String)> + Send + Sync>,
) -> datafusion::error::Result<Arc<dyn ExecutionPlan>> {
    // Check if this node is a RemoteTableScanExec by name, then extract dataset info
    if plan.name() == node_name {
        if let Some((namespace, dataset_name)) = extract(plan.as_ref()) {
            let resource = ResourceId { namespace, name: dataset_name };

            match engine.check(subject, &resource, "SELECT") {
                AccessDecision::Deny { reason } => {
                    warn!(
                        user = %subject.user_id,
                        namespace = %resource.namespace,
                        dataset = %resource.name,
                        reason = %reason,
                        "IAM denied query — replacing with EmptyExec"
                    );
                    return Ok(Arc::new(EmptyExec::new(plan.schema())));
                }
                AccessDecision::Allow { row_filter: Some(filter_sql), .. } => {
                    warn!(
                        user = %subject.user_id,
                        filter = %filter_sql,
                        "Row-level filter from IAM policy (not yet applied to remote scan)"
                    );
                }
                AccessDecision::Allow { .. } => {}
            }
        }
    }

    // Recurse into children
    let new_children: datafusion::error::Result<Vec<Arc<dyn ExecutionPlan>>> = plan
        .children()
        .into_iter()
        .map(|child| apply_iam(child.clone(), engine, subject, node_name, extract))
        .collect();

    plan.with_new_children(new_children?)
}
