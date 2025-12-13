pub mod framed;
pub mod framed_wrapper;
pub mod message_creator;
pub mod message_parser;
pub mod stdio_transport;
pub mod transport;
pub mod types;

/// Common boxed error type for LSP module boundaries.
// Using `anyhow::Error` directly across the codebase; removed `DynError alias.
use crate::lsp::framed::FramedTransport;
use crate::lsp::types::Message;
use lsp_types::SymbolKind;
use std::time::Duration;

pub struct LspClient {
    communicator: Box<dyn FramedTransport + Send + Sync>,
    message_builder: message_creator::MessageBuilder,
}

impl LspClient {
    pub fn new(transport: Box<dyn crate::lsp::transport::LspTransport + Send + Sync>) -> Self {
        let message_builder = message_creator::MessageBuilder::new();
        let framed = crate::lsp::framed_wrapper::FramedBox::new(transport);
        LspClient {
            communicator: Box::new(framed),
            message_builder,
        }
    }

    pub async fn initialize(&mut self) -> anyhow::Result<()> {
        let request = self.message_builder.initialize()?;
        // send request via framed transport and wait for response
        let id = self.communicator.send_request(request).await?;
        let _resp = self
            .communicator
            .receive_response_with_timeout(id, Some(Duration::from_secs(10)))
            .await?;

        let initialized_notification = self.message_builder.initialized_notification()?;
        // send initialized notification
        self.communicator
            .send_notification(initialized_notification)
            .await?;

        Ok(())
    }

    pub async fn get_all_function_list(&mut self) -> anyhow::Result<()> {
        let request = self
            .message_builder
            .create_request("workspace/symbol", Some(serde_json::json!({"query": ""})))?;

        // send request and wait for response
        let id = self.communicator.send_request(request).await?;

        let response = self
            .communicator
            .receive_response_with_timeout(id, Some(Duration::from_secs(10)))
            .await?;

        match response {
            Message::Response(response) => {
                let symbols: Vec<lsp_types::SymbolInformation> =
                    serde_json::from_value(response.result.unwrap()).unwrap();

                for symbol in symbols {
                    match symbol.kind {
                        SymbolKind::FUNCTION => println!("Function: {}", symbol.name),
                        SymbolKind::STRUCT => println!("Struct: {}", symbol.name),
                        _ => {}
                    }
                }
            }
            Message::Error(_response) => {
                // handle error
            }
            Message::Notification(_notification) => {
                // ignore notifications here
            }
        }

        Ok(())
    }

    pub async fn shutdown(&mut self) -> anyhow::Result<()> {
        let request = self.message_builder.create_request("shutdown", Some(""))?;

        // send shutdown request and wait for response
        let id = self.communicator.send_request(request).await?;

        let _response = self
            .communicator
            .receive_response_with_timeout(id, Some(Duration::from_secs(10)))
            .await?;

        let notification = self.message_builder.create_notification("exit", Some(""))?;
        self.communicator.send_notification(notification).await?;

        Ok(())
    }
    /*
    pub async fn did_open_notification(
        &mut self,
        file_path: &str,
        file_contents: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let notification = self
            .message_creator
            .did_open_notification(file_path, file_contents)?;
        let message = serde_json::to_string(&notification)?;
        self.communicator.send_message2(&message).await?;

        Ok(())
    }
    */
}
