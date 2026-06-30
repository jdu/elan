//! gRPC client for elan-central's audit stream.
//!
//! [`CentralClient`] wraps a tonic channel and exposes a single method
//! that returns an async stream of [`AuditEventProto`] messages.  The
//! `main.rs` event loop polls this stream and forwards events to the TUI
//! as [`AppEvent::AuditMessage`] strings.

use elan_common::proto::audit::{
    audit_service_client::AuditServiceClient, AuditEventProto, StreamRequest,
};
use tokio_stream::{Stream, StreamExt};
use tonic::transport::Channel;

/// Holds a tonic `Channel` to elan-central for audit stream subscriptions.
pub struct CentralClient {
    channel: Channel,
}

impl CentralClient {
    /// Connect to the given elan-central gRPC endpoint.
    pub async fn connect(endpoint: &str) -> anyhow::Result<Self> {
        let channel = tonic::transport::Channel::from_shared(endpoint.to_string())?
            .connect()
            .await?;
        Ok(Self { channel })
    }

    /// Subscribe to the elan-central audit event stream.
    /// elan-central will first replay recent events, then stream live ones.
    pub async fn stream_audit_events(
        &self,
    ) -> anyhow::Result<impl Stream<Item = Result<AuditEventProto, tonic::Status>>> {
        let mut client = AuditServiceClient::new(self.channel.clone());
        let stream = client
            .stream_audit_events(StreamRequest {
                since: None,
                event_types: vec![],
            })
            .await?
            .into_inner();
        Ok(stream)
    }
}
