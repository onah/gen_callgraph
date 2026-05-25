use crate::lsp::types::{Notification, Request};
use lsp_types::{
    ClientCapabilities, InitializeParams, SymbolKind, SymbolKindCapability,
    TextDocumentClientCapabilities, WorkspaceClientCapabilities, WorkspaceFolder,
};
use serde::Serialize;

pub struct RequestIdGenerator {
    id: i32,
}

impl RequestIdGenerator {
    pub fn new() -> Self {
        RequestIdGenerator { id: 0 }
    }

    pub fn get_id(&mut self) -> i32 {
        self.id += 1;
        self.id
    }
}

pub struct MessageBuilder {
    message_factory: RequestIdGenerator,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_with_relative_path_returns_error() {
        let mut builder = MessageBuilder::new();
        assert!(builder.initialize(".").is_err());
        assert!(builder.initialize("src/main.rs").is_err());
    }

    #[test]
    fn initialize_with_absolute_path_produces_file_uri() {
        let mut builder = MessageBuilder::new();
        let request = builder.initialize("/tmp/test_workspace").unwrap();
        let uri = request.params["workspaceFolders"][0]["uri"]
            .as_str()
            .expect("workspaceFolders[0].uri should be a string");
        assert!(
            uri.starts_with("file:///"),
            "URI should start with file:///, got: {}",
            uri
        );
        assert!(
            uri.contains("test_workspace"),
            "URI should contain the workspace directory name, got: {}",
            uri
        );
    }

    #[test]
    fn initialize_method_name_is_initialize() {
        let mut builder = MessageBuilder::new();
        let request = builder.initialize("/tmp/workspace").unwrap();
        assert_eq!(request.method, "initialize");
    }

    #[test]
    fn initialize_jsonrpc_version_is_2_0() {
        let mut builder = MessageBuilder::new();
        let request = builder.initialize("/tmp/workspace").unwrap();
        assert_eq!(request.jsonrpc, "2.0");
    }
}

impl MessageBuilder {
    pub fn new() -> MessageBuilder {
        let message_factory = RequestIdGenerator::new();
        MessageBuilder { message_factory }
    }

    pub fn create_request<T: Serialize>(
        &mut self,
        method: &str,
        params: T,
    ) -> anyhow::Result<Request> {
        let value = serde_json::to_value(params)?;
        let id = self.message_factory.get_id();
        Ok(Request::new(id, method.to_string(), value))
    }

    pub fn create_notification<T: Serialize>(
        &mut self,
        method: &str,
        params: T,
    ) -> anyhow::Result<Notification> {
        let value = serde_json::to_value(params)?;
        Ok(Notification::new(method.to_string(), value))
    }

    pub fn initialize(&mut self, workspace_path: &str) -> anyhow::Result<Request> {
        let uri = lsp_types::Url::from_file_path(workspace_path)
            .map_err(|_| anyhow::anyhow!("invalid workspace path: {}", workspace_path))?;
        let initialize_params = InitializeParams {
            process_id: Some(std::process::id()),
            workspace_folders: Some(vec![WorkspaceFolder {
                uri,
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
                    call_hierarchy: Some(lsp_types::CallHierarchyClientCapabilities {
                        dynamic_registration: Some(true),
                    }),
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
        let request = self.create_request("initialize", initialize_params)?;

        Ok(request)
    }

    pub fn initialized_notification(&mut self) -> anyhow::Result<Notification> {
        let notification = self.create_notification("initialized", "")?;
        Ok(notification)
    }

    /*
    pub fn did_open_notification(
        &mut self,
        file_path: &str,
        file_contents: &str,
    ) -> Result<Notification, Box<dyn std::error::Error>> {
        let notification = self.create_notification(
            "textDocument/didOpen",
            serde_json::json!({
                "textDocument": {
                    "uri": format!("file://{}", file_path),
                    "languageId": "rust",
                    "version": 1,
                    "text": file_contents
                }
            }),
        )?;
        Ok(notification)
    }
    */
}
