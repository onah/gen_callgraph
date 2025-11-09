pub mod communicator;
pub mod message_creator;

use crate::lsp::message_creator::{Message, SendMessage};

use lsp_types::SymbolKind;
//use std::fs;
pub struct LspClient {
    communicator: communicator::Communicator,
    message_factory: message_creator::MesssageFactory,
    message_creator: message_creator::MessageCreator,
}

impl LspClient {
    pub fn new(communicator: communicator::Communicator) -> Self {
        let message_factory = message_creator::MesssageFactory::new();
        let message_creator = message_creator::MessageCreator::new();
        LspClient {
            communicator,
            message_factory,
            message_creator,
        }
    }

    pub async fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let request = self.message_creator.initialize()?;
        let id = request.id;
        let message = serde_json::to_string(&request)?;

        self.communicator.send_message2(&message).await?;
        self.communicator.receive_response(id).await?;

        let initialized_notification = self.message_creator.initialized_notification()?;
        let message = serde_json::to_string(&initialized_notification)?;

        self.communicator.send_message2(&message).await?;

        Ok(())
    }

    pub async fn get_all_function_list(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let request = self
            .message_factory
            .create_request("workspace/symbol", Some(serde_json::json!({"query": ""})));

        let message = serde_json::to_string(&request)?;
        self.communicator.send_message2(&message).await?;
        loop {
            let response = self.communicator.receive_message().await?;
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
        let request = self.message_factory.create_request("shutdown", Some(""));
        let request = SendMessage::Request(request);

        self.communicator.send_message(&request).await?;
        let _response = self.communicator.receive_message().await?;

        let notification = self.message_factory.create_notification("exit", Some(""));
        let notification = SendMessage::Notification(notification);
        self.communicator.send_message(&notification).await?;

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
