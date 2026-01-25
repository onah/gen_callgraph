#[async_trait::async_trait]
pub trait LspBackend {
    async fn send_request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value>;

    fn recieve_notification(&self) -> anyhow::Result<serde_json::Value>;
}
