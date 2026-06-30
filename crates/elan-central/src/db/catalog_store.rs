use chrono::Utc;
use elan_common::types::catalog::{DatasetInfo, SourceType};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct CatalogStore {
    pool: SqlitePool,
}

impl CatalogStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn upsert_coordinator(
        &self,
        id: &str,
        environment: &str,
        hostname: &str,
        executor_endpoint: &str,
    ) -> anyhow::Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO coordinators (id, environment, hostname, executor_endpoint, registered_at, last_heartbeat_at, is_alive)
            VALUES (?, ?, ?, ?, ?, ?, 1)
            ON CONFLICT(id) DO UPDATE SET
                environment       = excluded.environment,
                hostname          = excluded.hostname,
                executor_endpoint = excluded.executor_endpoint,
                last_heartbeat_at = excluded.last_heartbeat_at,
                is_alive          = 1
            "#,
        )
        .bind(id)
        .bind(environment)
        .bind(hostname)
        .bind(executor_endpoint)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn heartbeat(&self, coordinator_id: &str) -> anyhow::Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE coordinators SET last_heartbeat_at = ?, is_alive = 1 WHERE id = ?")
            .bind(&now)
            .bind(coordinator_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn upsert_dataset(
        &self,
        coordinator_id: &str,
        dataset_id: &str,
        name: &str,
        namespace: &str,
        source_type: &str,
        _ignored: &str,
        arrow_schema_ipc: &[u8],
        metadata_json: Option<&str>,
    ) -> anyhow::Result<()> {
        // Look up executor_endpoint from the coordinator record
        let executor_endpoint: String = sqlx::query(
            "SELECT executor_endpoint FROM coordinators WHERE id = ?",
        )
        .bind(coordinator_id)
        .fetch_optional(&self.pool)
        .await?
        .map(|r: sqlx::sqlite::SqliteRow| r.get::<String, _>("executor_endpoint"))
        .unwrap_or_default();

        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO datasets
                (id, name, namespace, source_type, coordinator_id, executor_endpoint,
                 arrow_schema_ipc, metadata_json, registered_at, last_seen_at, is_active)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)
            ON CONFLICT(namespace, name) DO UPDATE SET
                source_type       = excluded.source_type,
                coordinator_id    = excluded.coordinator_id,
                executor_endpoint = excluded.executor_endpoint,
                arrow_schema_ipc  = excluded.arrow_schema_ipc,
                metadata_json     = excluded.metadata_json,
                last_seen_at      = excluded.last_seen_at,
                is_active         = 1
            "#,
        )
        .bind(dataset_id)
        .bind(name)
        .bind(namespace)
        .bind(source_type)
        .bind(coordinator_id)
        .bind(&executor_endpoint)
        .bind(arrow_schema_ipc)
        .bind(metadata_json)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_dataset(
        &self,
        namespace: &str,
        name: &str,
    ) -> anyhow::Result<Option<DatasetInfo>> {
        let row = sqlx::query(
            r#"
            SELECT d.id, d.name, d.namespace, d.source_type, d.coordinator_id,
                   d.executor_endpoint, d.arrow_schema_ipc, d.metadata_json
            FROM datasets d
            JOIN coordinators c ON c.id = d.coordinator_id
            WHERE d.namespace = ? AND d.name = ? AND d.is_active = 1 AND c.is_alive = 1
            "#,
        )
        .bind(namespace)
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r: sqlx::sqlite::SqliteRow| row_to_dataset(&r))
            .transpose()
    }

    pub async fn list_datasets(
        &self,
        namespace_filter: Option<&str>,
    ) -> anyhow::Result<Vec<DatasetInfo>> {
        let rows: Vec<sqlx::sqlite::SqliteRow> = if let Some(ns) = namespace_filter {
            sqlx::query(
                r#"
                SELECT d.id, d.name, d.namespace, d.source_type, d.coordinator_id,
                       d.executor_endpoint, d.arrow_schema_ipc, d.metadata_json
                FROM datasets d
                JOIN coordinators c ON c.id = d.coordinator_id
                WHERE d.namespace = ? AND d.is_active = 1 AND c.is_alive = 1
                ORDER BY d.namespace, d.name
                "#,
            )
            .bind(ns)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT d.id, d.name, d.namespace, d.source_type, d.coordinator_id,
                       d.executor_endpoint, d.arrow_schema_ipc, d.metadata_json
                FROM datasets d
                JOIN coordinators c ON c.id = d.coordinator_id
                WHERE d.is_active = 1 AND c.is_alive = 1
                ORDER BY d.namespace, d.name
                "#,
            )
            .fetch_all(&self.pool)
            .await?
        };

        rows.iter().map(row_to_dataset).collect()
    }

    pub async fn deactivate_dataset(&self, dataset_id: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE datasets SET is_active = 0 WHERE id = ?")
            .bind(dataset_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn mark_coordinator_dead(&self, coordinator_id: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE coordinators SET is_alive = 0 WHERE id = ?")
            .bind(coordinator_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

fn row_to_dataset(r: &sqlx::sqlite::SqliteRow) -> anyhow::Result<DatasetInfo> {
    let id: String = r.try_get("id")?;
    let metadata_json: Option<String> = r.try_get("metadata_json")?;
    Ok(DatasetInfo {
        id: Uuid::parse_str(&id)?,
        name: r.try_get("name")?,
        namespace: r.try_get("namespace")?,
        source_type: {
            let st: String = r.try_get("source_type")?;
            SourceType::try_from(st.as_str()).map_err(|e| anyhow::anyhow!(e))?
        },
        coordinator_id: r.try_get("coordinator_id")?,
        executor_endpoint: r.try_get("executor_endpoint")?,
        schema_ipc: r.try_get("arrow_schema_ipc")?,
        metadata: metadata_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?
            .unwrap_or(serde_json::Value::Null),
    })
}
