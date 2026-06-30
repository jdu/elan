//! Unified error type for the elan system.
//!
//! [`ElanError`] is the single error enum used across all public APIs.
//! The `From<ElanError> for tonic::Status` impl enables propagating domain
//! errors directly through gRPC handlers without manual mapping.

use thiserror::Error;

/// Top-level error type for all elan operations.
#[derive(Error, Debug)]
pub enum ElanError {
    #[error("dataset not found: {namespace}.{name}")]
    DatasetNotFound { namespace: String, name: String },

    #[error("access denied: {reason}")]
    AccessDenied { reason: String },

    #[error("coordinator not found: {id}")]
    CoordinatorNotFound { id: String },

    #[error("gRPC error: {0}")]
    Grpc(#[from] tonic::Status),

    #[error("transport error: {0}")]
    Transport(#[from] tonic::transport::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("uuid parse error: {0}")]
    Uuid(#[from] uuid::Error),

    #[error("kafka error: {0}")]
    Kafka(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, ElanError>;

impl From<ElanError> for tonic::Status {
    fn from(e: ElanError) -> Self {
        match e {
            ElanError::DatasetNotFound { .. } => tonic::Status::not_found(e.to_string()),
            ElanError::AccessDenied { .. } => tonic::Status::permission_denied(e.to_string()),
            ElanError::CoordinatorNotFound { .. } => tonic::Status::not_found(e.to_string()),
            ElanError::Grpc(s) => s,
            _ => tonic::Status::internal(e.to_string()),
        }
    }
}
