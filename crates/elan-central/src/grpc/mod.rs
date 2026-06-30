//! gRPC service implementations for elan-central.
//!
//! Each sub-module implements one proto service:
//! - [`audit_svc`]: `AuditService` — persist and stream audit events
//! - [`catalog_svc`]: `CatalogService` — read dataset/coordinator metadata
//! - [`coordinator_svc`]: `CoordinatorService` — register coordinators and datasets
//! - [`iam_svc`]: `IamService` — manage subjects, policies, and access checks

pub mod audit_svc;
pub mod catalog_svc;
pub mod coordinator_svc;
pub mod iam_svc;
