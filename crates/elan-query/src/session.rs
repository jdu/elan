//! Per-request DataFusion session factory for elan-query.
//!
//! [`SessionFactory`] owns the shared catalog state (dataset list, IAM engine)
//! and builds a fresh `SessionContext` for each incoming query with the
//! requesting user's IAM context baked in.
//!
//! **Why `Arc<Self>` not `Self`?**  `new()` spawns a background catalog-refresh
//! task that holds a `Weak<Self>`.  The weak reference lets the background task
//! detect when the factory is dropped (e.g. in tests) and exit cleanly, avoiding
//! a reference cycle that would keep the factory alive forever.
//!
//! **`with_default_catalog_and_schema("elan", "public")`**: DataFusion resolves
//! unqualified table references against the default catalog and schema.  Setting
//! the default catalog to `"elan"` means a two-part reference like
//! `crm.customers` resolves to `elan.crm.customers` as intended.

use crate::catalog::provider::{ElanCatalogProvider, ElanSchemaProvider};
use crate::config::QueryConfig;
use datafusion::execution::context::SessionContext;
use datafusion::execution::SessionStateBuilder;
use elan_common::{
    proto::catalog::{
        catalog_service_client::CatalogServiceClient, ListDatasetsRequest,
    },
    proto::iam::{
        iam_service_client::IamServiceClient, ListPoliciesRequest,
    },
    types::DatasetInfo,
};
use elan_iam::{
    types::{Policy, PolicyEffect, SubjectType},
    IamEngine, SnapshotIamEngine, Subject,
};
use uuid::Uuid;
use std::collections::HashMap;
use std::sync::Arc;
use tokio_stream::StreamExt;
use tonic::transport::Channel;
use tracing::info;

/// Builds per-request DataFusion sessions with IAM enforcement wired in.
///
/// Shared via `Arc<SessionFactory>` between HTTP handler tasks.
pub struct SessionFactory {
    catalog_client: Arc<tokio::sync::Mutex<CatalogServiceClient<Channel>>>,
    iam_engine: Arc<SnapshotIamEngine>,
    datasets_by_ns: Arc<tokio::sync::RwLock<HashMap<String, Vec<DatasetInfo>>>>,
}

impl SessionFactory {
    /// Connect to elan-central, load datasets and IAM policies, then start the
    /// background catalog-refresh task.  Returns an `Arc<Self>` because the
    /// refresh task holds a `Weak` back-reference to detect factory shutdown.
    pub async fn new(cfg: &QueryConfig) -> anyhow::Result<Arc<Self>> {
        let channel = tonic::transport::Channel::from_shared(cfg.central_endpoint.clone())?
            .connect()
            .await?;
        let mut catalog_client = CatalogServiceClient::new(channel.clone());

        // Eagerly load all datasets at startup
        let datasets_by_ns = load_datasets(&mut catalog_client).await?;
        info!("loaded {} namespace(s) from catalog", datasets_by_ns.len());

        // Load IAM policies from elan-central
        let policies = load_iam_policies(channel).await.unwrap_or_else(|e| {
            tracing::warn!(error = %e, "failed to load IAM policies; defaulting to deny-all");
            vec![]
        });
        info!(count = policies.len(), "loaded IAM policies");
        let iam_engine = SnapshotIamEngine::new(policies);

        let factory = Arc::new(Self {
            catalog_client: Arc::new(tokio::sync::Mutex::new(catalog_client)),
            iam_engine,
            datasets_by_ns: Arc::new(tokio::sync::RwLock::new(datasets_by_ns)),
        });

        // Background task: re-poll elan-central's catalog every 30 s so newly registered
        // datasets appear without restarting elan-query.
        let weak = Arc::downgrade(&factory);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            interval.tick().await; // skip immediate first tick
            loop {
                interval.tick().await;
                let Some(factory) = weak.upgrade() else { break };
                let mut client = factory.catalog_client.lock().await;
                match load_datasets(&mut client).await {
                    Ok(fresh) => {
                        let prev_count = factory.datasets_by_ns.read().await.len();
                        let new_count = fresh.len();
                        *factory.datasets_by_ns.write().await = fresh;
                        if new_count != prev_count {
                            info!(namespaces = new_count, "catalog refreshed: namespace count changed");
                        }
                    }
                    Err(e) => tracing::warn!(error = %e, "catalog refresh failed, retaining cached view"),
                }
            }
        });

        Ok(factory)
    }

    /// Build a per-request SessionContext with IAM enforcement wired up for this user.
    pub async fn build_for_user(&self, subject: Subject) -> anyhow::Result<SessionContext> {
        let datasets = self.datasets_by_ns.read().await.clone();
        let client = Arc::clone(&self.catalog_client);
        let iam = Arc::clone(&self.iam_engine) as Arc<dyn IamEngine>;

        // Build per-namespace schema providers
        let mut schemas: HashMap<String, Arc<ElanSchemaProvider>> = HashMap::new();
        for (ns, ds_list) in datasets {
            let schema_provider = ElanSchemaProvider::new(
                ns.clone(),
                ds_list,
                Arc::clone(&client),
                Arc::clone(&iam),
                subject.clone(),
            );
            schemas.insert(ns, Arc::new(schema_provider));
        }

        let catalog = ElanCatalogProvider::new(schemas);

        // Build the physical optimizer rule for IAM enforcement
        let optimizer_rule = Arc::new(
            elan_iam::optimizer::IamFilterRule::new(
                Arc::clone(&self.iam_engine) as Arc<dyn IamEngine>,
                subject,
            )
            .with_extractor(|plan| {
                plan.as_any()
                    .downcast_ref::<crate::planner::remote_scan::RemoteTableScanExec>()
                    .map(|s| (s.dataset_namespace().to_string(), s.dataset_name().to_string()))
            }),
        );

        // Add the IAM rule via SessionState builder (DF53 API — SessionContext has no
        // add_physical_optimizer_rule; must build state then wrap in context)
        let state = SessionStateBuilder::new()
            .with_default_features()
            .with_physical_optimizer_rule(optimizer_rule)
            .with_config(
                datafusion::prelude::SessionConfig::new()
                    .with_default_catalog_and_schema("elan", "public"),
            )
            .build();
        let ctx = SessionContext::new_with_state(state);

        // Register as "elan" catalog
        ctx.register_catalog("elan", catalog);

        Ok(ctx)
    }

    /// Expose the shared IAM engine (e.g. for the IAM gRPC service to reload it).
    pub fn iam_engine(&self) -> &Arc<SnapshotIamEngine> {
        &self.iam_engine
    }
}

async fn load_iam_policies(channel: Channel) -> anyhow::Result<Vec<Policy>> {
    let mut client = IamServiceClient::new(channel);
    let mut stream = client
        .list_policies(ListPoliciesRequest {
            subject_name: String::new(),
        })
        .await?
        .into_inner();

    let mut policies = vec![];
    while let Some(proto) = stream.next().await {
        let proto = proto?;
        let subject_type = match proto.subject_type.as_str() {
            "group" => SubjectType::Group,
            _ => SubjectType::User,
        };
        let effect = match proto.effect {
            1 => PolicyEffect::Deny,
            _ => PolicyEffect::Allow,
        };
        policies.push(Policy {
            id: Uuid::parse_str(&proto.policy_id).unwrap_or_else(|_| Uuid::new_v4()),
            subject_name: proto.subject_name,
            subject_type,
            resource_pattern: proto.resource_pattern,
            action: proto.action,
            effect,
            row_filter: if proto.row_filter.is_empty() {
                None
            } else {
                Some(proto.row_filter)
            },
            column_mask_json: if proto.column_mask_json.is_empty() {
                None
            } else {
                Some(proto.column_mask_json)
            },
            priority: proto.priority,
        });
    }
    Ok(policies)
}

async fn load_datasets(
    client: &mut CatalogServiceClient<Channel>,
) -> anyhow::Result<HashMap<String, Vec<DatasetInfo>>> {
    let mut stream = client
        .list_datasets(ListDatasetsRequest {
            namespace_filter: String::new(),
        })
        .await?
        .into_inner();

    let mut by_ns: HashMap<String, Vec<DatasetInfo>> = HashMap::new();

    while let Some(proto) = stream.next().await {
        let proto = proto?;
        let ds = DatasetInfo {
            id: uuid::Uuid::parse_str(&proto.dataset_id)?,
            name: proto.name,
            namespace: proto.namespace.clone(),
            source_type: elan_common::types::catalog::SourceType::try_from(
                proto.source_type.as_str(),
            )?,
            coordinator_id: proto.coordinator_id,
            executor_endpoint: proto.executor_endpoint,
            schema_ipc: proto.arrow_schema_ipc,
            metadata: serde_json::from_str(&proto.metadata_json).unwrap_or_default(),
        };
        by_ns.entry(ds.namespace.clone()).or_default().push(ds);
    }

    Ok(by_ns)
}
