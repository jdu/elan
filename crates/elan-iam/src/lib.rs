//! IAM engine for the elan federated query system.
//!
//! This crate is intentionally decoupled from any specific database or HTTP
//! framework.  It provides:
//! - [`engine`]: the [`IamEngine`] trait and [`SnapshotIamEngine`] (in-memory
//!   policy snapshot loaded from elan-central, reloaded after mutations)
//! - [`optimizer`]: [`IamFilterRule`], a DataFusion `PhysicalOptimizerRule`
//!   that replaces denied `RemoteTableScanExec` nodes with `EmptyExec`
//! - [`catalog_filter`]: helper that tells the schema provider whether to
//!   expose a table at all (pre-planning visibility filter)
//! - [`types`]: `Subject`, `ResourceId`, `Policy`, `AccessDecision`, etc.

pub mod catalog_filter;
pub mod engine;
pub mod optimizer;
pub mod types;

pub use engine::{IamEngine, SnapshotIamEngine};
pub use types::{AccessDecision, ColumnMask, Effect, MaskKind, Policy, ResourceId, Subject};
