//! Audit event types shared across all elan services.
//!
//! Every action that needs to be recorded produces an [`AuditEvent`] that
//! carries a strongly-typed [`AuditPayload`] variant.  The `event_type`
//! string is derived from the payload variant and used as both a Kafka topic
//! suffix and a filter key in the gRPC streaming API.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// The identity that triggered an auditable action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditSubject {
    pub user_id: String,
    pub groups: Vec<String>,
    pub session_id: Option<String>,
}

/// A single immutable audit record produced by any elan service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub schema_version: u8,
    pub event_id: Uuid,
    pub occurred_at: DateTime<Utc>,
    pub source_service: String,
    pub source_instance: String,
    pub subject: AuditSubject,
    #[serde(flatten)]
    pub payload: AuditPayload,
}

impl AuditEvent {
    /// Construct a new event, assigning a fresh UUID and the current UTC timestamp.
    pub fn new(
        source_service: impl Into<String>,
        source_instance: impl Into<String>,
        subject: AuditSubject,
        payload: AuditPayload,
    ) -> Self {
        Self {
            schema_version: 1,
            event_id: Uuid::new_v4(),
            occurred_at: Utc::now(),
            source_service: source_service.into(),
            source_instance: source_instance.into(),
            subject,
            payload,
        }
    }

    /// Returns a stable string tag for the payload variant, used as a filter
    /// key in the gRPC streaming API and as a Kafka topic suffix component.
    pub fn event_type(&self) -> &str {
        match &self.payload {
            AuditPayload::QuerySubmitted(_) => "QuerySubmitted",
            AuditPayload::QueryCompleted(_) => "QueryCompleted",
            AuditPayload::QueryFailed(_) => "QueryFailed",
            AuditPayload::AccessDenied(_) => "AccessDenied",
            AuditPayload::CoordinatorRegistered(_) => "CoordinatorRegistered",
            AuditPayload::DatasetRegistered(_) => "DatasetRegistered",
        }
    }

    /// Kafka topic derived from event type
    pub fn kafka_topic(&self) -> String {
        let suffix = match &self.payload {
            AuditPayload::QuerySubmitted(_) => "query.submitted",
            AuditPayload::QueryCompleted(_) => "query.completed",
            AuditPayload::QueryFailed(_) => "query.failed",
            AuditPayload::AccessDenied(_) => "access.denied",
            AuditPayload::CoordinatorRegistered(_) => "coordinator.registered",
            AuditPayload::DatasetRegistered(_) => "dataset.registered",
        };
        format!("elan.audit.{suffix}")
    }
}

/// Strongly-typed payload for each auditable event category.
///
/// Serialized with `serde(tag = "event_type", content = "payload")` so the
/// discriminant appears as a top-level JSON field alongside the event envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type", content = "payload")]
pub enum AuditPayload {
    QuerySubmitted(QuerySubmittedPayload),
    QueryCompleted(QueryCompletedPayload),
    QueryFailed(QueryFailedPayload),
    AccessDenied(AccessDeniedPayload),
    CoordinatorRegistered(CoordinatorRegisteredPayload),
    DatasetRegistered(DatasetRegisteredPayload),
}

/// Emitted by elan-query when a SQL statement is received, before execution begins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuerySubmittedPayload {
    pub query_id: Uuid,
    pub sql: String,
    pub resolved_tables: Vec<String>,
    pub executors: HashMap<String, String>,
}

/// Emitted by elan-query when a query finishes successfully.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryCompletedPayload {
    pub query_id: Uuid,
    pub duration_ms: u64,
    pub rows_returned: usize,
    pub bytes_scanned: u64,
}

/// Emitted by elan-query when a query fails (DataFusion error or IAM denial).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryFailedPayload {
    pub query_id: Uuid,
    pub duration_ms: u64,
    pub error_kind: String,
    pub error_msg: String,
}

/// Emitted when the IAM engine blocks access to a dataset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessDeniedPayload {
    pub query_id: Uuid,
    pub namespace: String,
    pub dataset: String,
    pub action: String,
    pub reason: String,
}

/// Emitted when a coordinator calls the `register` gRPC method on elan-central.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorRegisteredPayload {
    pub coordinator_id: String,
    pub environment: String,
    pub hostname: String,
}

/// Emitted when a coordinator registers a dataset with elan-central.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetRegisteredPayload {
    pub dataset_id: String,
    pub name: String,
    pub namespace: String,
    pub source_type: String,
    pub coordinator_id: String,
}
