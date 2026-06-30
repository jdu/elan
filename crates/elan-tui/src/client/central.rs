use elan_common::proto::audit::{
    audit_service_client::AuditServiceClient, AuditEventProto, StreamRequest,
};
use tokio_stream::{Stream, StreamExt};
use tonic::transport::Channel;

pub struct CentralClient {
    channel: Channel,
}

impl CentralClient {
    pub async fn connect(endpoint: &str) -> anyhow::Result<Self> {
        let channel = tonic::transport::Channel::from_shared(endpoint.to_string())?
            .connect()
            .await?;
        Ok(Self { channel })
    }

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
