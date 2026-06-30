//! HTTP and gRPC client modules for elan-tui.
//!
//! - [`central`]: gRPC client for elan-central's audit stream
//! - [`query`]: HTTP client for elan-query's REST API

pub mod central;
pub mod query;
