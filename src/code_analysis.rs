use crate::lsp_client::{LspClient, Message, Request};
use lsp_types::DocumentSymbol;

use lsp_types::{
    ClientCapabilities, InitializeParams, SymbolKind, SymbolKindCapability,
    TextDocumentClientCapabilities, WorkspaceClientCapabilities, WorkspaceFolder,
};
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Serialize, Deserialize, Debug)]
pub struct Notification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<serde_json::Value>,
}
struct MesssageFuctory {
    id: i32,
}

impl MesssageFuctory {
    pub fn new() -> Self {
        MesssageFuctory { id: 0 }
    }

    pub fn get_id(&mut self) -> i32 {
        self.id += 1;
        self.id
    }

    pub fn create_request<T: Serialize>(&mut self, method: &str, params: Option<T>) -> Request {
        Request {
            jsonrpc: "2.0".to_string(),
            id: self.get_id(),
            method: method.to_string(),
            params: params.map(|p| serde_json::to_value(p).unwrap()),
        }
    }

    pub fn create_notification<T: Serialize>(
        &mut self,
        method: &str,
        params: Option<T>,
    ) -> Notification {
        Notification {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params: params.map(|p| serde_json::to_value(p).unwrap()),
        }
    }
}

pub struct CodeAnalyzer {
    client: LspClient,
    factory: MesssageFuctory,
}

impl CodeAnalyzer {
    pub fn new(client: LspClient) -> Self {
        let factory = MesssageFuctory::new();
        CodeAnalyzer { client, factory }
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
            .factory
            .create_request("initialize", Some(initialize_params));

        self.client.send_message(&request).await?;
        self.client.receive_message().await?;

        let initialized_notification = self.factory.create_notification("initialized", Some(""));
        self.client.send_message(&initialized_notification).await?;

        Ok(())
    }

    pub async fn get_all_function_list(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let request = self.factory.create_request(
            "workspace/symbol",
            Some(serde_json::json!({"query": "main"})),
        );

        self.client.send_message(&request).await?;
        let response = self.client.receive_message().await?;

        match response {
            Message::ResponseMessage(response) => {
                //println!("{:#?}", response);

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
            Message::ResponseError(response) => {
                println!("Error: {:#?}", response.error.unwrap());
            }
            Message::Notification(notification) => {
                println!("{:#?}", notification);
            }
        }

        Ok(())
    }

    pub async fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let request = self.factory.create_request("shutdown", Some(""));

        self.client.send_message(&request).await?;
        let _response = self.client.receive_message().await?;

        let notification = self.factory.create_notification("exit", Some(""));
        self.client.send_message(&notification).await?;

        Ok(())
    }

    pub async fn get_main_function_location(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // send textDocumetn/didOpen notification

        let file_path = "c:/Users/PCuser/Work/rust/gen_callgraph/src/communicate_lsp.rs";
        let file_contents = fs::read_to_string(file_path).unwrap();

        let did_open_notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": format!("file://{}", file_path),
                    "languageId": "rust",
                    "version": 1,
                    "text": file_contents
                }
            }
        });

        self.client
            .send_message(&did_open_notification)
            .await
            .unwrap();

        // send textDocument/documentSymbol request

        let request = Request {
            jsonrpc: "2.0".to_string(),
            id: 3,
            method: "textDocument/documentSymbol".to_string(),
            params: Some(serde_json::json!({
                "textDocument": {
                    "uri": "file:///c:/Users/PCuser/Work/rust/gen_callgraph/src/communicate_lsp.rs"
                }
            })),
        };

        self.client.send_message(&request).await.unwrap();
        let response = self.client.receive_message().await?;

        match response {
            Message::ResponseMessage(response) => {
                let symbols: Vec<DocumentSymbol> =
                    serde_json::from_value(response.result.unwrap()).unwrap();

                for symbol in symbols {
                    println!("{:#?}", symbol);
                }

                //println!("{:#?}", response.result.unwrap());
            }
            Message::ResponseError(response) => {
                println!("{:#?}", response.error.unwrap());
            }
            Message::Notification(notification) => {
                println!("{:#?}", notification);
            }
        }

        let request = Request {
            jsonrpc: "2.0".to_string(),
            id: 4,
            method: "textDocument/prepareCallHierarchy".to_string(),
            params: Some(serde_json::json!({
                "textDocument": {
                    "uri": "file:///c:/Users/PCuser/Work/rust/gen_callgraph/src/communicate_lsp.rs"
                },
                "position": {
                    "line": 0,
                    "character": 0
                }
            })),
        };

        self.client.send_message(&request).await?;
        let response = self.client.receive_message().await?;
        match response {
            Message::ResponseMessage(response) => {
                println!("{:#?}", response.result.unwrap());
            }
            Message::ResponseError(response) => {
                println!("{:#?}", response.error.unwrap());
            }
            Message::Notification(notification) => {
                println!("{:#?}", notification);
            }
        }
        Ok(())
    }
}
