use lsp_types::{
    ClientCapabilities, InitializeParams, SymbolKind, SymbolKindCapability,
    TextDocumentClientCapabilities, WorkspaceClientCapabilities, WorkspaceFolder,
};
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub id: i32,
    pub method: String,
    pub params: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ResponseMessage {
    pub jsonrpc: String,
    pub id: i32,
    pub result: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
pub struct ResponseError {
    pub jsonrpc: String,
    pub id: i32,
    pub error: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Notification {
    pub jsonrpc: String,
    pub method: String,
    pub params: serde_json::Value,
}
pub enum Message {
    Response(ResponseMessage),
    Error(ResponseError),
    Notification(Notification),
}

pub enum SendMessage {
    Request(Request),
    Notification(Notification),
}

pub struct MessageFactory {
    id: i32,
}

impl MessageFactory {
    pub fn new() -> Self {
        MessageFactory { id: 0 }
    }

    pub fn get_id(&mut self) -> i32 {
        self.id += 1;
        self.id
    }

    pub fn create_request<T: Serialize>(&mut self, method: &str, params: T) -> Request {
        Request {
            jsonrpc: "2.0".to_string(),
            id: self.get_id(),
            method: method.to_string(),
            params: serde_json::to_value(params).unwrap(),
        }
    }

    pub fn create_notification<T: Serialize>(&mut self, method: &str, params: T) -> Notification {
        Notification {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params: serde_json::to_value(params).unwrap(),
        }
    }
}

pub struct MessageCreator {
    message_factory: MessageFactory,
}

impl MessageCreator {
    pub fn new() -> MessageCreator {
        let message_factory = MessageFactory::new();
        MessageCreator { message_factory }
    }
    pub fn initialize(&mut self) -> Result<Request, Box<dyn std::error::Error>> {
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
            .create_request("initialize", initialize_params);

        Ok(request)
    }

    pub fn initialized_notification(&mut self) -> Result<Notification, Box<dyn std::error::Error>> {
        let notification = self.message_factory.create_notification("initialized", "");
        Ok(notification)
    }

    /*
    pub fn did_open_notification(
        &mut self,
        file_path: &str,
        file_contents: &str,
    ) -> Result<Notification, Box<dyn std::error::Error>> {
        let notification = self.message_factory.create_notification(
            "textDocument/didOpen",
            serde_json::json!({
                "textDocument": {
                    "uri": format!("file://{}", file_path),
                    "languageId": "rust",
                    "version": 1,
                    "text": file_contents
                }
            }),
        );
        Ok(notification)
    }
    */
}
