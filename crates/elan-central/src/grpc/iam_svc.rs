//! gRPC `IamService` implementation.
//!
//! Manages IAM subjects (users/groups), policies, and ad-hoc access checks.
//! Policy mutation methods (`create_policy`, `delete_policy`) call
//! `reload_engine()` afterwards so the in-process [`SnapshotIamEngine`] stays
//! consistent with the database without requiring a process restart.

use crate::db::iam_store::IamStore;
use elan_common::proto::iam::{
    iam_service_server::IamService, AccessDecision as ProtoDecision, AccessRequest,
    GroupMemberRequest, ListPoliciesRequest, PolicyIdRequest, PolicyIdResponse, PolicyProto,
    SubjectIdResponse, SubjectProto,
};
use elan_iam::{IamEngine, types::SubjectType};
use std::sync::Arc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

/// gRPC handler for the `IamService` proto service.
pub struct IamSvc {
    store: Arc<IamStore>,
    /// Shared in-process engine; reloaded after every policy write.
    engine: Arc<elan_iam::SnapshotIamEngine>,
}

impl IamSvc {
    /// Construct the service with the shared IAM store and snapshot engine.
    pub fn new(store: Arc<IamStore>, engine: Arc<elan_iam::SnapshotIamEngine>) -> Self {
        Self { store, engine }
    }
}

#[tonic::async_trait]
impl IamService for IamSvc {
    type ListPoliciesStream = ReceiverStream<Result<PolicyProto, Status>>;

    async fn check_access(
        &self,
        request: Request<AccessRequest>,
    ) -> Result<Response<ProtoDecision>, Status> {
        let req = request.into_inner();
        let subject = elan_iam::types::Subject {
            user_id: req.user_id,
            groups: req.groups,
        };
        let resource = elan_iam::types::ResourceId {
            namespace: req.namespace,
            name: req.dataset,
        };

        let decision = self.engine.check(&subject, &resource, &req.action);

        let proto = match decision {
            elan_iam::AccessDecision::Allow {
                row_filter,
                column_mask,
            } => ProtoDecision {
                allowed: true,
                reason: String::new(),
                row_filter: row_filter.unwrap_or_default(),
                column_mask_json: column_mask
                    .map(|m| {
                        let map: std::collections::HashMap<String, String> = m
                            .0
                            .into_iter()
                            .map(|(k, v)| {
                                let s = match v {
                                    elan_iam::MaskKind::Redact => "REDACT",
                                    elan_iam::MaskKind::Sha256 => "SHA256",
                                }
                                .to_string();
                                (k, s)
                            })
                            .collect();
                        serde_json::to_string(&map).unwrap_or_default()
                    })
                    .unwrap_or_default(),
            },
            elan_iam::AccessDecision::Deny { reason } => ProtoDecision {
                allowed: false,
                reason,
                row_filter: String::new(),
                column_mask_json: String::new(),
            },
        };

        Ok(Response::new(proto))
    }

    async fn list_policies(
        &self,
        request: Request<ListPoliciesRequest>,
    ) -> Result<Response<Self::ListPoliciesStream>, Status> {
        let req = request.into_inner();
        let subject_filter = if req.subject_name.is_empty() {
            None
        } else {
            Some(req.subject_name.as_str())
        };

        let policies = self
            .store
            .list_policies(subject_filter)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let (tx, rx) = tokio::sync::mpsc::channel(32);
        tokio::spawn(async move {
            for p in policies {
                let proto = PolicyProto {
                    policy_id: p.id.to_string(),
                    subject_name: p.subject_name,
                    subject_type: match p.subject_type {
                        SubjectType::User => "user".into(),
                        SubjectType::Group => "group".into(),
                    },
                    resource_pattern: p.resource_pattern,
                    action: p.action,
                    effect: match p.effect {
                        elan_iam::types::PolicyEffect::Allow => 0,
                        elan_iam::types::PolicyEffect::Deny => 1,
                    },
                    row_filter: p.row_filter.unwrap_or_default(),
                    column_mask_json: p.column_mask_json.unwrap_or_default(),
                    priority: p.priority,
                };
                if tx.send(Ok(proto)).await.is_err() {
                    break;
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn create_policy(
        &self,
        request: Request<PolicyProto>,
    ) -> Result<Response<PolicyIdResponse>, Status> {
        let p = request.into_inner();
        let effect = if p.effect == 1 { "Deny" } else { "Allow" };

        let policy_id = self
            .store
            .create_policy(
                &p.subject_name,
                &p.subject_type,
                &p.resource_pattern,
                &p.action,
                effect,
                if p.row_filter.is_empty() { None } else { Some(&p.row_filter) },
                if p.column_mask_json.is_empty() { None } else { Some(&p.column_mask_json) },
                p.priority,
            )
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // Reload engine policies after a write
        self.reload_engine().await?;

        Ok(Response::new(PolicyIdResponse { policy_id }))
    }

    async fn delete_policy(
        &self,
        request: Request<PolicyIdRequest>,
    ) -> Result<Response<()>, Status> {
        self.store
            .delete_policy(&request.into_inner().policy_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        self.reload_engine().await?;

        Ok(Response::new(()))
    }

    async fn create_subject(
        &self,
        request: Request<SubjectProto>,
    ) -> Result<Response<SubjectIdResponse>, Status> {
        let req = request.into_inner();
        let id = self
            .store
            .get_or_create_subject(&req.subject_type, &req.name)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(SubjectIdResponse { subject_id: id }))
    }

    async fn add_group_member(
        &self,
        request: Request<GroupMemberRequest>,
    ) -> Result<Response<()>, Status> {
        let req = request.into_inner();
        self.store
            .add_group_member(&req.group_name, &req.user_name)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(()))
    }
}

impl IamSvc {
    async fn reload_engine(&self) -> Result<(), Status> {
        let policies = self
            .store
            .list_policies(None)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        self.engine.reload(policies);
        Ok(())
    }
}
