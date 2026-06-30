//! DataFusion catalog integration for elan-query.
//!
//! - [`ElanCatalogProvider`]: the top-level DataFusion `CatalogProvider` named
//!   `"elan"`, containing one [`ElanSchemaProvider`] per namespace.
//! - [`ElanSchemaProvider`]: a DataFusion `SchemaProvider` for one namespace;
//!   applies the IAM catalog filter so hidden datasets are silently absent.
//! - [`ElanTableProvider`]: a DataFusion `TableProvider` that produces a
//!   [`RemoteTableScanExec`] when scanned.

pub mod provider;
pub mod table;

pub use provider::{ElanCatalogProvider, ElanSchemaProvider};
pub use table::ElanTableProvider;
