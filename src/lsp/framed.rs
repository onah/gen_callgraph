use crate::lsp::types::{Message, Notification, Request};
use async_trait::async_trait;

#[async_trait]
pub trait FramedTransport: Send + Sync {
    // New higher-level APIs (non-breaking additions).
    // - `send_request` should register the pending receiver internally and send the request payload.
    //   It returns the assigned request id on success.
    // Returns the assigned id for the sent request. Responses can be received via
    // `wait_response` by passing the id.
    async fn send_request(&mut self, request: Request) -> anyhow::Result<i32>;

    // Convenience API: send request and wait for the corresponding response.
    // This keeps backward compatibility while offering a simpler call style.
    async fn send_and_wait(
        &mut self,
        request: Request,
        timeout: Option<std::time::Duration>,
    ) -> anyhow::Result<Message> {
        let id = self.send_request(request).await?;
        self.wait_response(id, timeout).await
    }

    // - `send_notification` sends a notification message (no response expected).
    async fn send_notification(&mut self, notification: Notification) -> anyhow::Result<()>;

    // - `wait_response` waits for a response for the given id, with an optional timeout.
    //   If `timeout` is `Some(duration)` and the wait exceeds it, return an error.
    async fn wait_response(
        &mut self,
        id: i32,
        timeout: Option<std::time::Duration>,
    ) -> anyhow::Result<Message>;

    // - `wait_notification` waits for the next server-to-client notification.
    //   If `timeout` is `Some(duration)` and the wait exceeds it, return an error.
    async fn wait_notification(
        &mut self,
        timeout: Option<std::time::Duration>,
    ) -> anyhow::Result<Notification>;
}
