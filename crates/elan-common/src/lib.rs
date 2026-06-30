//! Shared types for the elan federated query system.
//!
//! - [`error`]: the unified [`ElanError`] enum used across all crates
//! - [`proto`]: tonic-generated gRPC stubs for all four proto services
//! - [`types`]: domain types (`DatasetInfo`, `SourceType`, HTTP API shapes)

pub mod error;
pub mod proto;
pub mod types;

pub use error::{ElanError, Result};
