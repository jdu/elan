//! [`KafkaAuditSink`]: publishes audit events to Apache Kafka topics.
//!
//! Each event is serialized to JSON and sent to a topic derived from the
//! event type (e.g. `elan.audit.query.submitted`).  The sink is optional —
//! elan-query falls back to [`NoOpAuditSink`] when no brokers are configured.

use crate::{sink::AuditSink, AuditEvent};
use async_trait::async_trait;
use elan_common::ElanError;
use rdkafka::{
    producer::{FutureProducer, FutureRecord},
    ClientConfig,
};
use std::time::Duration;
use tracing::instrument;

/// Audit sink that sends events to Kafka using `rdkafka`'s async `FutureProducer`.
pub struct KafkaAuditSink {
    producer: FutureProducer,
}

impl KafkaAuditSink {
    /// Create a sink using a comma-separated broker list, e.g. `"localhost:9092"`.
    pub fn new(brokers: &str) -> Result<Self, ElanError> {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .set("delivery.timeout.ms", "10000")
            .create()
            .map_err(|e| ElanError::Kafka(e.to_string()))?;

        Ok(Self { producer })
    }

    /// Create a sink from a fully-configured `rdkafka::ClientConfig` (useful in tests).
    pub fn new_with_config(config: &ClientConfig) -> Result<Self, ElanError> {
        let producer: FutureProducer = config
            .create()
            .map_err(|e| ElanError::Kafka(e.to_string()))?;
        Ok(Self { producer })
    }
}

#[async_trait]
impl AuditSink for KafkaAuditSink {
    #[instrument(skip(self, event), fields(event_type = %event.event_type()))]
    async fn publish(&self, event: AuditEvent) -> Result<(), ElanError> {
        let topic = event.kafka_topic();
        let key = event.event_id.to_string();
        let payload = serde_json::to_string(&event).map_err(ElanError::Serde)?;

        let record = FutureRecord::to(&topic).key(&key).payload(&payload);

        self.producer
            .send(record, Duration::from_secs(5))
            .await
            .map_err(|(e, _)| ElanError::Kafka(e.to_string()))?;

        Ok(())
    }
}
