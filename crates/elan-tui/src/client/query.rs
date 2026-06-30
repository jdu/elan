use elan_common::types::api::{CatalogResponse, QueryRequest, QueryResponse};
use reqwest::Client;

pub struct QueryClient {
    client: Client,
    base_url: String,
    username: String,
}

impl QueryClient {
    pub fn new(base_url: String, username: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
            username,
        }
    }

    pub async fn query(&self, sql: &str, session_id: &str) -> anyhow::Result<QueryResponse> {
        let resp = self
            .client
            .post(format!("{}/api/v1/query", self.base_url))
            .header("Authorization", format!("Bearer {}", self.username))
            .json(&QueryRequest {
                sql: sql.to_string(),
                session_id: Some(session_id.to_string()),
            })
            .send()
            .await?;

        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            let status = resp.status();
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            anyhow::bail!("query failed ({}): {}", status, body)
        }
    }

    pub async fn catalog(&self) -> anyhow::Result<CatalogResponse> {
        let resp = self
            .client
            .get(format!("{}/api/v1/catalog", self.base_url))
            .header("Authorization", format!("Bearer {}", self.username))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("catalog request failed ({}): {}", status, body);
        }
        Ok(resp.json().await?)
    }
}
