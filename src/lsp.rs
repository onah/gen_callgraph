pub mod framed;
pub mod framed_wrapper;
pub mod message_creator;
pub mod message_parser;
pub mod stdio_transport;
pub mod transport;
pub mod types;

/// Common boxed error type for LSP module boundaries.
pub type DynError = Box<dyn std::error::Error + Send + Sync>;

use crate::lsp::framed::FramedTransport;
use crate::lsp::types::{Message, SendMessage};

use lsp_types::SymbolKind;
//use std::fs;
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
            message_builder: message_builder,
        }
    }

    pub async fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let request = self.message_builder.initialize()?;
        let id = request.id;
        let message = serde_json::to_string(&request)?;

        self.communicator
            .send_message2(&message)
            .await
            .map_err(|e| e as Box<dyn std::error::Error>)?;
        self.communicator
            .receive_response(id)
            .await
            .map_err(|e| e as Box<dyn std::error::Error>)?;

        let initialized_notification = self.message_builder.initialized_notification()?;
        let message = serde_json::to_string(&initialized_notification)?;

        self.communicator
            .send_message2(&message)
            .await
            .map_err(|e| e as Box<dyn std::error::Error>)?;

        Ok(())
    }

    pub async fn get_all_function_list(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let request = self
            .message_builder
            .create_request("workspace/symbol", Some(serde_json::json!({"query": ""})))?;

        let message = serde_json::to_string(&request)?;
        self.communicator
            .send_message2(&message)
            .await
            .map_err(|e| e as Box<dyn std::error::Error>)?;
        loop {
            let response = self
                .communicator
                .receive_message()
                .await
                .map_err(|e| e as Box<dyn std::error::Error>)?;
            //println!("End get all function list");

            match response {
                Message::Response(response) => {
                    //println!("ResponseMessage: {:#?}", response);

                    let symbols: Vec<lsp_types::SymbolInformation> =
                        serde_json::from_value(response.result.unwrap()).unwrap();

                    for symbol in symbols {
                        match symbol.kind {
                            SymbolKind::FUNCTION => println!("Function: {}", symbol.name),
                            SymbolKind::STRUCT => println!("Struct: {}", symbol.name),
                            _ => {}
                        }
                    }
                    break;
                }
                Message::Error(_response) => {
                    //println!("Error: {:#?}", response.error.unwrap());
                    break;
                }
                Message::Notification(_notification) => {
                    //println!("Notification {:#?}", notification);
                }
            }
        }

        Ok(())
    }

    pub async fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let request = self.message_builder.create_request("shutdown", Some(""))?;
        let request = SendMessage::Request(request);

        self.communicator
            .send_message(&request)
            .await
            .map_err(|e| e as Box<dyn std::error::Error>)?;
        let _response = self
            .communicator
            .receive_message()
            .await
            .map_err(|e| e as Box<dyn std::error::Error>)?;

        let notification = self.message_builder.create_notification("exit", Some(""))?;
        let notification = SendMessage::Notification(notification);
        self.communicator
            .send_message(&notification)
            .await
            .map_err(|e| e as Box<dyn std::error::Error>)?;

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
