use crate::lsp::framed::FramedTransport;
use crate::lsp::types::{Message, SendMessage};
use crate::lsp::protocol::parse_message_from_str;
use crate::lsp::DynError;
use crate::lsp::transport::LspTransport;
use async_trait::async_trait;

// Convenience impl for boxed transports (trait objects)
// FramedBox: convenience wrapper for boxed trait objects
pub struct FramedBox {
    inner: Box<dyn LspTransport + Send + Sync>,
}

impl FramedBox {
    pub fn new(inner: Box<dyn LspTransport + Send + Sync>) -> Self {
        FramedBox { inner }
    }
}

#[async_trait]
impl FramedTransport for FramedBox {
    async fn send_message(&mut self, message: &SendMessage) -> Result<(), DynError> {
        let s = match message {
            SendMessage::Request(r) => serde_json::to_string(r)?,
            SendMessage::Notification(n) => serde_json::to_string(n)?,
        };
        self.send_message2(&s).await
    }

    async fn send_message2(&mut self, message: &str) -> Result<(), DynError> {
        self.inner.send(message).await
    }

    async fn receive_message(&mut self) -> Result<Message, DynError> {
        let buffer = self.inner.read().await?;
        let msg = parse_message_from_str(&buffer)?;
        Ok(msg)
    }

    async fn receive_response(&mut self, id: i32) -> Result<Message, DynError> {
        loop {
            let buffer = self.inner.read().await?;
            let message = parse_message_from_str(&buffer)?;
            if let Message::Response(ref response) = message {
                if response.id == id {
                    return Ok(message);
                }
            } else if let Message::Notification(_) = message {
                return Ok(message);
            }
        }
    }
}
