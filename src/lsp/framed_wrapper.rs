use crate::lsp::framed::FramedTransport;
use crate::lsp::message_creator::{Message, SendMessage};
use crate::lsp::protocol::{parse_notification, parse_response, DynError};
use crate::lsp::transport::LspTransport;
use async_trait::async_trait;

// Keep the generic Framed<T> implementation available for future use, but allow
// dead_code to avoid warnings while the codebase uses boxed transports.
#[allow(dead_code)]
pub struct Framed<T>
where
    T: LspTransport + Send + Sync,
{
    inner: T,
}

#[allow(dead_code)]
impl<T> Framed<T>
where
    T: LspTransport + Send + Sync,
{
    pub fn new(inner: T) -> Self {
        Framed { inner }
    }
}

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
        let json: serde_json::Value = serde_json::from_str(&buffer)?;

        if let Some(notification) = parse_notification(&json)? {
            return Ok(Message::Notification(notification));
        }

        if let Some(response) = parse_response(&json)? {
            return Ok(response);
        }

        Err(Box::new(std::io::Error::other("Other Message")))
    }

    async fn receive_response(&mut self, id: i32) -> Result<Message, DynError> {
        loop {
            let buffer = self.inner.read().await?;
            let json: serde_json::Value = serde_json::from_str(&buffer)?;

            if let Some(notification) = parse_notification(&json)? {
                return Ok(Message::Notification(notification));
            }

            if let Some(message) = parse_response(&json)? {
                if let Message::Response(ref response) = message {
                    if response.id == id {
                        return Ok(message);
                    }
                }
            }
        }
    }
}
