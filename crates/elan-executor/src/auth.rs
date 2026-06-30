use reqwest::Client;

pub struct AuthClient {
    http: Client,
    coordinator_url: String,
}

impl AuthClient {
    pub fn new(coordinator_url: String) -> Self {
        Self {
            http: Client::new(),
            coordinator_url,
        }
    }

    pub async fn check(&self, dataset: &str, caller: &str) -> anyhow::Result<bool> {
        let url = format!(
            "{}/auth/check?dataset={}&caller={}",
            self.coordinator_url, dataset, caller
        );
        let resp: serde_json::Value = self.http.get(&url).send().await?.json().await?;
        Ok(resp["allowed"].as_bool().unwrap_or(false))
    }
}
