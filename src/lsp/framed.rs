use crate::lsp::types::{Message, Notification, Request};
use async_trait::async_trait;

#[async_trait]
pub trait FramedTransport: Send + Sync {
    // New higher-level APIs (non-breaking additions).
    // - `send_request` should register the pending receiver internally and send the request payload.
    //   It returns the assigned request id on success.
    // Returns the assigned id for the sent request. Responses can be received via
    // `receive_response_with_timeout` by passing the id.
    async fn send_request(&mut self, request: Request) -> anyhow::Result<i32>;

    // Convenience API: send request and wait for the corresponding response.
    // This keeps backward compatibility while offering a simpler call style.
    async fn request(
        &mut self,
        request: Request,
        timeout: Option<std::time::Duration>,
    ) -> anyhow::Result<Message> {
        let id = self.send_request(request).await?;
        self.receive_response_with_timeout(id, timeout).await
    }

    // - `send_notification` sends a notification message (no response expected).
    async fn send_notification(&mut self, notification: Notification) -> anyhow::Result<()>;

    // - `receive_response_with_timeout` waits for a response for the given id, with an optional timeout.
    //   If `timeout` is `Some(duration)` and the wait exceeds it, return an error.
    async fn receive_response_with_timeout(
        &mut self,
        id: i32,
        timeout: Option<std::time::Duration>,
    ) -> anyhow::Result<Message>;
}
