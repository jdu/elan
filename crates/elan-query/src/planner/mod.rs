//! DataFusion physical plan nodes for elan-query.
//!
//! Currently contains only [`remote_scan::RemoteTableScanExec`], which fans
//! out SQL execution to a remote elan-executor over HTTP.
pub mod remote_scan;
