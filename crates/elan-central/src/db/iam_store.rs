use elan_iam::types::{Policy, PolicyEffect, SubjectType};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct IamStore {
    pool: SqlitePool,
}

impl IamStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create_subject(&self, subject_type: &str, name: &str) -> anyhow::Result<String> {
        let id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO iam_subjects (id, subject_type, name) VALUES (?, ?, ?)")
            .bind(&id)
            .bind(subject_type)
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(id)
    }

    pub async fn get_or_create_subject(
        &self,
        subject_type: &str,
        name: &str,
    ) -> anyhow::Result<String> {
        let row = sqlx::query("SELECT id FROM iam_subjects WHERE name = ?")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;
        if let Some(r) = row {
            Ok(r.try_get::<String, _>("id")?)
        } else {
            self.create_subject(subject_type, name).await
        }
    }

    pub async fn add_group_member(&self, group_name: &str, user_name: &str) -> anyhow::Result<()> {
        let group_id = self.get_or_create_subject("group", group_name).await?;
        let user_id = self.get_or_create_subject("user", user_name).await?;
        sqlx::query(
            "INSERT OR IGNORE INTO iam_group_members (group_id, user_id) VALUES (?, ?)",
        )
        .bind(&group_id)
        .bind(&user_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn create_policy(
        &self,
        subject_name: &str,
        subject_type: &str,
        resource_pattern: &str,
        action: &str,
        effect: &str,
        row_filter: Option<&str>,
        column_mask_json: Option<&str>,
        priority: i32,
    ) -> anyhow::Result<String> {
        let subject_id = self.get_or_create_subject(subject_type, subject_name).await?;
        let policy_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO iam_policies
                (id, subject_id, resource_pattern, action, effect, row_filter, column_mask_json, priority)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&policy_id)
        .bind(&subject_id)
        .bind(resource_pattern)
        .bind(action)
        .bind(effect)
        .bind(row_filter)
        .bind(column_mask_json)
        .bind(priority)
        .execute(&self.pool)
        .await?;
        Ok(policy_id)
    }

    pub async fn delete_policy(&self, policy_id: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM iam_policies WHERE id = ?")
            .bind(policy_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list_policies(
        &self,
        subject_name: Option<&str>,
    ) -> anyhow::Result<Vec<Policy>> {
        let rows = sqlx::query(
            r#"
            SELECT p.id, s.name AS subject_name, s.subject_type,
                   p.resource_pattern, p.action, p.effect,
                   p.row_filter, p.column_mask_json, p.priority
            FROM iam_policies p
            JOIN iam_subjects s ON s.id = p.subject_id
            ORDER BY p.priority DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.iter()
            .filter(|r| {
                subject_name
                    .map(|n| r.get::<String, _>("subject_name") == n)
                    .unwrap_or(true)
            })
            .map(|r| {
                let id: String = r.try_get("id").map_err(|e| anyhow::anyhow!(e))?;
                let subject_type: String = r.try_get("subject_type").map_err(|e| anyhow::anyhow!(e))?;
                let effect: String = r.try_get("effect").map_err(|e| anyhow::anyhow!(e))?;
                let priority: i64 = r.try_get("priority").map_err(|e| anyhow::anyhow!(e))?;
                Ok::<Policy, anyhow::Error>(Policy {
                    id: Uuid::parse_str(&id)?,
                    subject_name: r.try_get("subject_name")?,
                    subject_type: match subject_type.as_str() {
                        "group" => SubjectType::Group,
                        _ => SubjectType::User,
                    },
                    resource_pattern: r.try_get("resource_pattern").map_err(|e| anyhow::anyhow!(e))?,
                    action: r.try_get("action").map_err(|e| anyhow::anyhow!(e))?,
                    effect: match effect.as_str() {
                        "Deny" => PolicyEffect::Deny,
                        _ => PolicyEffect::Allow,
                    },
                    row_filter: r.try_get("row_filter").map_err(|e| anyhow::anyhow!(e))?,
                    column_mask_json: r.try_get("column_mask_json").map_err(|e| anyhow::anyhow!(e))?,
                    priority: priority as i32,
                })
            })
            .collect::<Result<Vec<_>, anyhow::Error>>()
    }

    pub async fn store_audit_event(
        &self,
        id: &str,
        event_type: &str,
        occurred_at: &str,
        source_service: &str,
        user_id: &str,
        session_id: Option<&str>,
        payload_json: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO audit_events
                (id, event_type, occurred_at, source_service, user_id, session_id, payload_json)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id)
        .bind(event_type)
        .bind(occurred_at)
        .bind(source_service)
        .bind(user_id)
        .bind(session_id)
        .bind(payload_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_recent_audit_events(
        &self,
        since: Option<&str>,
        event_types: &[String],
        limit: i64,
    ) -> anyhow::Result<Vec<(String, String, String, String, String, Option<String>, String)>> {
        let rows = sqlx::query(
            r#"
            SELECT id, event_type, occurred_at, source_service, user_id, session_id, payload_json
            FROM audit_events
            WHERE (? IS NULL OR occurred_at >= ?)
            ORDER BY occurred_at DESC
            LIMIT ?
            "#,
        )
        .bind(since)
        .bind(since)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .filter(|r| {
                event_types.is_empty()
                    || event_types.contains(&r.get::<String, _>("event_type"))
            })
            .map(|r| {
                (
                    r.get::<String, _>("id"),
                    r.get::<String, _>("event_type"),
                    r.get::<String, _>("occurred_at"),
                    r.get::<String, _>("source_service"),
                    r.get::<String, _>("user_id"),
                    r.get::<Option<String>, _>("session_id"),
                    r.get::<String, _>("payload_json"),
                )
            })
            .collect())
    }
}
