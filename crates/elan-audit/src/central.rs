use crate::{AuditEvent, AuditSink};
use async_trait::async_trait;
use elan_common::{
    proto::audit::{audit_service_client::AuditServiceClient, AuditEventProto},
    ElanError,
};
use tonic::transport::Channel;

/// Publishes audit events directly to elan-central via gRPC.
/// The TUI subscribes to elan-central's stream, so events appear immediately.
pub struct CentralAuditSink {
    client: AuditServiceClient<Channel>,
}

impl CentralAuditSink {
    pub async fn new(endpoint: &str) -> anyhow::Result<Self> {
        let channel = Channel::from_shared(endpoint.to_string())?
            .connect()
            .await?;
        Ok(Self {
            client: AuditServiceClient::new(channel),
        })
    }
}

#[async_trait]
impl AuditSink for CentralAuditSink {
    async fn publish(&self, event: AuditEvent) -> Result<(), ElanError> {
        let payload_json =
            serde_json::to_string(&event.payload).map_err(ElanError::Serde)?;

        let proto = AuditEventProto {
            event_id: event.event_id.to_string(),
            event_type: event.event_type().to_string(),
            occurred_at: None,
            source_service: event.source_service.clone(),
            user_id: event.subject.user_id.clone(),
            session_id: event.subject.session_id.clone().unwrap_or_default(),
            payload_json,
        };

        // Clone the client so we can call it with &mut self from &self
        let mut client = self.client.clone();
        client
            .publish_event(proto)
            .await
            .map_err(|e| ElanError::Grpc(tonic::Status::internal(e.to_string())))?;

        Ok(())
    }
}
