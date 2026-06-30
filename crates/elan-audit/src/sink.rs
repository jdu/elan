//! Core [`AuditSink`] trait and a no-op implementation.

use crate::AuditEvent;
use async_trait::async_trait;
use elan_common::ElanError;

/// Abstraction over audit backends (elan-central gRPC, Kafka, or no-op).
///
/// Implementations must be `Send + Sync + 'static` so they can be shared
/// across async tasks via `Arc<dyn AuditSink>`.
#[async_trait]
pub trait AuditSink: Send + Sync + 'static {
    /// Publish a single audit event to the backing store.
    async fn publish(&self, event: AuditEvent) -> Result<(), ElanError>;
}

/// No-op sink for tests and when Kafka is not configured.
pub struct NoOpAuditSink;

#[async_trait]
impl AuditSink for NoOpAuditSink {
    async fn publish(&self, _event: AuditEvent) -> Result<(), ElanError> {
        Ok(())
    }
}
