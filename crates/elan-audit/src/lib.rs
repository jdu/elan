//! Audit event publishing for the elan federated query system.
//!
//! This crate defines the [`AuditEvent`] type and the [`AuditSink`] trait.
//! Concrete sinks: [`CentralAuditSink`] (gRPC to elan-central, enabling
//! real-time TUI streaming) and [`KafkaAuditSink`] (async Kafka publishing).
//! [`NoOpAuditSink`] is used in tests or when no sink is configured.

pub mod central;
pub mod event;
pub mod kafka;
pub mod sink;

pub use central::CentralAuditSink;
pub use event::{AuditEvent, AuditPayload, AuditSubject};
pub use sink::{AuditSink, NoOpAuditSink};
