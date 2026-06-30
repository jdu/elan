use crate::db::iam_store::IamStore;
use elan_common::proto::audit::{
    audit_service_server::AuditService, AuditEventProto, PublishResponse, StreamRequest,
};
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use tracing::warn;

/// A broadcast channel allows multiple TUI clients to subscribe to the same audit stream.
pub type AuditBroadcast = broadcast::Sender<AuditEventProto>;

pub struct AuditSvc {
    store: Arc<IamStore>,
    broadcast: AuditBroadcast,
}

impl AuditSvc {
    pub fn new(store: Arc<IamStore>, broadcast: AuditBroadcast) -> Self {
        Self { store, broadcast }
    }
}

#[tonic::async_trait]
impl AuditService for AuditSvc {
    type StreamAuditEventsStream = ReceiverStream<Result<AuditEventProto, Status>>;

    async fn stream_audit_events(
        &self,
        request: Request<StreamRequest>,
    ) -> Result<Response<Self::StreamAuditEventsStream>, Status> {
        let req = request.into_inner();
        let (tx, rx) = tokio::sync::mpsc::channel(64);

        // First send recent events from SQLite
        let since = req.since.map(|ts| {
            chrono::DateTime::<chrono::Utc>::from_timestamp(ts.seconds, ts.nanos as u32)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default()
        });

        let recent = self
            .store
            .get_recent_audit_events(since.as_deref(), &req.event_types, 100)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        for (id, event_type, occurred_at, source_service, user_id, session_id, payload_json) in
            recent
        {
            let proto = AuditEventProto {
                event_id: id,
                event_type,
                occurred_at: None,
                source_service,
                user_id,
                session_id: session_id.unwrap_or_default(),
                payload_json,
            };
            if tx.send(Ok(proto)).await.is_err() {
                return Ok(Response::new(ReceiverStream::new(rx)));
            }
        }

        // Then subscribe to live events via broadcast channel
        let mut broadcast_rx = self.broadcast.subscribe();
        let filter_types = req.event_types.clone();

        tokio::spawn(async move {
            loop {
                match broadcast_rx.recv().await {
                    Ok(event) => {
                        if filter_types.is_empty() || filter_types.contains(&event.event_type) {
                            if tx.send(Ok(event)).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!(skipped = n, "audit broadcast lagged");
                    }
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn publish_event(
        &self,
        request: Request<AuditEventProto>,
    ) -> Result<Response<PublishResponse>, Status> {
        let event = request.into_inner();

        // Persist to SQLite
        let payload = serde_json::json!({ "raw": &event.payload_json });
        self.store
            .store_audit_event(
                &event.event_id,
                &event.event_type,
                &chrono::Utc::now().to_rfc3339(),
                &event.source_service,
                &event.user_id,
                if event.session_id.is_empty() { None } else { Some(&event.session_id) },
                &event.payload_json,
            )
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // Broadcast to live subscribers
        let _ = self.broadcast.send(event);

        Ok(Response::new(PublishResponse { accepted: true }))
    }
}
