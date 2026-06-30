//! SQLite persistence layer for elan-central.
//!
//! [`connect`] opens (or creates) the database and runs embedded migrations
//! from the workspace-level `migrations/` directory via `sqlx::migrate!`.
//! [`catalog_store::CatalogStore`] and [`iam_store::IamStore`] share the
//! same connection pool.

pub mod catalog_store;
pub mod iam_store;

use sqlx::sqlite::SqliteConnectOptions;
use sqlx::SqlitePool;
use std::str::FromStr;

/// Open a SQLite pool and run pending migrations, creating the file if absent.
pub async fn connect(database_url: &str) -> anyhow::Result<SqlitePool> {
    let opts = SqliteConnectOptions::from_str(database_url)?.create_if_missing(true);
    let pool = SqlitePool::connect_with(opts).await?;
    sqlx::migrate!("../../migrations").run(&pool).await?;
    Ok(pool)
}
