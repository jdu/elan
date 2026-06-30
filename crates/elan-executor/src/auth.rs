//! HTTP client for the coordinator's auth-check endpoint.
//!
//! Before executing SQL, the executor can optionally call the coordinator's
//! `GET /auth/check` endpoint to verify that the requesting caller has
//! access to the dataset.  In the current PoC the coordinator always returns
//! `allowed=true`.

use reqwest::Client;

/// HTTP client that delegates authorization checks to the coordinator.
pub struct AuthClient {
    http: Client,
    coordinator_url: String,
}

impl AuthClient {
    /// Create a client pointed at the given coordinator base URL.
    pub fn new(coordinator_url: String) -> Self {
        Self {
            http: Client::new(),
            coordinator_url,
        }
    }

    /// Ask the coordinator whether `caller` may access `dataset`.
    pub async fn check(&self, dataset: &str, caller: &str) -> anyhow::Result<bool> {
        let url = format!(
            "{}/auth/check?dataset={}&caller={}",
            self.coordinator_url, dataset, caller
        );
        let resp: serde_json::Value = self.http.get(&url).send().await?.json().await?;
        Ok(resp["allowed"].as_bool().unwrap_or(false))
    }
}
