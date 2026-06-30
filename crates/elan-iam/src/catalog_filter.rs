//! Pre-planning IAM visibility filter for the DataFusion catalog.
//!
//! Used by `ElanSchemaProvider::table_names()` and `::table()` to silently
//! hide datasets the requesting user cannot SELECT from, so they don't
//! appear in `SHOW TABLES` or cause confusing "table not found" errors.

use crate::{IamEngine, types::{ResourceId, Subject}};

/// Returns true if the subject can SELECT from this dataset.
/// Used by ElanSchemaProvider::table() to hide inaccessible tables silently.
pub fn is_visible(
    engine: &dyn IamEngine,
    subject: &Subject,
    namespace: &str,
    name: &str,
) -> bool {
    let resource = ResourceId {
        namespace: namespace.to_string(),
        name: name.to_string(),
    };
    engine.check(subject, &resource, "SELECT").is_allowed()
}
