use crate::lsp::types::{Message, SendMessage};
use crate::lsp::DynError;
use async_trait::async_trait;

#[async_trait]
pub trait FramedTransport: Send + Sync {
    async fn send_message(&mut self, message: &SendMessage) -> Result<(), DynError>;
    async fn send_message2(&mut self, message: &str) -> Result<(), DynError>;
    async fn receive_message(&mut self) -> Result<Message, DynError>;
    async fn receive_response(&mut self, id: i32) -> Result<Message, DynError>;
}
