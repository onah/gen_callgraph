//! LSP transport abstraction (framed Content-Length messages).
use async_trait::async_trait;

/// Minimal async trait for LSP transport.
/// - `write` takes a JSON body as raw bytes (not including LSP headers) and will frame it (Content-Length) and send.
/// - `read` returns the JSON body bytes (header stripped).
#[async_trait]
pub trait LspTransport: Send + Sync {
    async fn write(&mut self, json_body: &[u8]) -> Result<(), anyhow::Error>;
    async fn read(&mut self) -> Result<Vec<u8>, anyhow::Error>;
}
