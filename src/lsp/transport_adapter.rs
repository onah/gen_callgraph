use crate::lsp::framed::FramedTransport;
use crate::lsp::message_creator::{Message, SendMessage};
use crate::lsp::protocol::{parse_notification, parse_response, DynError};
use crate::lsp::transport::LspTransport;
use async_trait::async_trait;

pub struct TransportAdapter {
    transport: Box<dyn LspTransport + Send + Sync>,
}

impl TransportAdapter {
    pub fn new(transport: Box<dyn LspTransport + Send + Sync>) -> Self {
        Self { transport }
    }
}

#[async_trait]
impl FramedTransport for TransportAdapter {
    async fn send_message(
        &mut self,
        message: &SendMessage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let s = match message {
            SendMessage::Request(r) => serde_json::to_string(r)?,
            SendMessage::Notification(n) => serde_json::to_string(n)?,
        };
        self.send_message2(&s).await
    }

    async fn send_message2(&mut self, message: &str) -> Result<(), DynError> {
        // transport.send already returns Box<dyn Error + Send + Sync>
        self.transport.send(message).await
    }

    async fn receive_message(&mut self) -> Result<Message, DynError> {
        let buffer = self.transport.read().await?;

        let json: serde_json::Value = serde_json::from_str(&buffer)?;

        if let Some(notification) = parse_notification(&json)? {
            return Ok(Message::Notification(notification));
        }

        if let Some(response) = parse_response(&json)? {
            return Ok(response);
        }

        Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Other Message",
        )))
    }

    async fn receive_response(&mut self, id: i32) -> Result<Message, DynError> {
        loop {
            let buffer = self.transport.read().await?;

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
