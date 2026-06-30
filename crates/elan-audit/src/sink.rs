use crate::AuditEvent;
use async_trait::async_trait;
use elan_common::ElanError;

#[async_trait]
pub trait AuditSink: Send + Sync + 'static {
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
