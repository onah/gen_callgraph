use crate::lsp::framed::FramedTransport;
use crate::lsp::message_parser::parse_message_from_slice;
use crate::lsp::transport::LspTransport;
use crate::lsp::types::{Message, Notification, Request};
use async_trait::async_trait;
use std::time::Duration;

// Convenience impl for boxed transports (trait objects)
// FramedBox: convenience wrapper for boxed trait objects
pub struct FramedBox {
    transport: Box<dyn LspTransport + Send + Sync>,
}

impl FramedBox {
    pub fn new(transport: Box<dyn LspTransport + Send + Sync>) -> Self {
        FramedBox { transport }
    }
}

#[async_trait]
impl FramedTransport for FramedBox {
    async fn receive_response(&mut self, id: i32) -> anyhow::Result<Message> {
        loop {
            let buffer = self.transport.read().await?;
            let message = parse_message_from_slice(&buffer)?;
            if let Message::Response(ref response) = message {
                if response.id == id {
                    return Ok(message);
                }
            } else if let Message::Notification(_) = message {
                // Ignore notifications here; caller is waiting for a response with a specific id.
                // Continue the loop to read the next message.
                eprintln!(
                    "FramedBox: received notification while waiting for id={}: ignored",
                    id
                );
                continue;
            }
        }
    }

    async fn send_request(&mut self, request: Request) -> anyhow::Result<i32> {
        let id = request.id;
        // serialize and send
        let s = serde_json::to_vec(&request)?;
        self.transport.write(&s).await?;
        Ok(id)
    }

    async fn send_notification(&mut self, notification: Notification) -> anyhow::Result<()> {
        let s = serde_json::to_vec(&notification)?;
        self.transport.write(&s).await
    }

    async fn receive_response_with_timeout(
        &mut self,
        id: i32,
        timeout: Option<Duration>,
    ) -> anyhow::Result<Message> {
        match timeout {
            Some(dur) => {
                let fut = self.receive_response(id);
                match tokio::time::timeout(dur, fut).await {
                    Ok(res) => res,
                    Err(_) => Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "response timeout",
                    )
                    .into()),
                }
            }
            None => self.receive_response(id).await,
        }
    }
}
