pub mod communicator;
pub mod message_creator;

use crate::lsp::message_creator::{Message, SendMessage};

use lsp_types::{
    ClientCapabilities, InitializeParams, SymbolKind, SymbolKindCapability,
    TextDocumentClientCapabilities, WorkspaceClientCapabilities, WorkspaceFolder,
};
//use std::fs;
pub struct LspClient {
    communicator: communicator::Communicator,
    message_factory: message_creator::MesssageFuctory,
}

impl LspClient {
    pub fn new(communicator: communicator::Communicator) -> Self {
        let message_factory = message_creator::MesssageFuctory::new();
        LspClient {
            communicator,
            message_factory,
        }
    }

    pub async fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let initialize_params = InitializeParams {
            process_id: Some(std::process::id()),
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: lsp_types::Url::parse("file:///c:/Users/PCuser/Work/rust/gen_callgraph")?,
                name: String::from("gen_callgraph"),
            }]),
            capabilities: ClientCapabilities {
                workspace: Some(WorkspaceClientCapabilities {
                    symbol: Some(lsp_types::WorkspaceSymbolClientCapabilities {
                        dynamic_registration: Some(true),
                        symbol_kind: None,
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                text_document: Some(TextDocumentClientCapabilities {
                    document_symbol: Some(lsp_types::DocumentSymbolClientCapabilities {
                        dynamic_registration: Some(true),
                        symbol_kind: Some(SymbolKindCapability {
                            value_set: Some(vec![SymbolKind::FUNCTION, SymbolKind::STRUCT]),
                        }),
                        hierarchical_document_symbol_support: Some(true),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        };

        let request = self
            .message_factory
            .create_request("initialize", Some(initialize_params));
        let request = SendMessage::Request(request);

        self.communicator.send_message(&request).await?;
        self.communicator.receive_message().await?;

        let initialized_notification = self
            .message_factory
            .create_notification("initialized", Some(""));
        let initialized_notification = SendMessage::Notification(initialized_notification);

        self.communicator
            .send_message(&initialized_notification)
            .await?;

        Ok(())
    }

    pub async fn get_all_function_list(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let request = self
            .message_factory
            .create_request("workspace/symbol", Some(serde_json::json!({"query": ""})));
        let request = SendMessage::Request(request);

        self.communicator.send_message(&request).await?;
        loop {
            let response = self.communicator.receive_message().await?;
            println!("End get all function list");

            match response {
                Message::Response(response) => {
                    println!("ResponseMessage: {:#?}", response);

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
                Message::Error(response) => {
                    println!("Error: {:#?}", response.error.unwrap());
                    break;
                }
                Message::Notification(notification) => {
                    println!("Notification {:#?}", notification);
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
}
