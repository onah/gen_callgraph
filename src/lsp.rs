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
use lsp_types::{CallHierarchyItem, CallHierarchyOutgoingCall, SymbolInformation, SymbolKind};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct LspClient {
    communicator: Box<dyn FramedTransport + Send + Sync>,
    message_builder: message_creator::MessageBuilder,
    workspace_root: String,
    workspace_root_path: PathBuf,
    crate_name: String,
}

impl LspClient {
    pub fn new(
        transport: Box<dyn crate::lsp::transport::LspTransport + Send + Sync>,
        workspace_root: String,
    ) -> Self {
        let message_builder = message_creator::MessageBuilder::new();
        let framed = crate::lsp::framed_wrapper::FramedBox::new(transport);
        let workspace_root_path = std::fs::canonicalize(&workspace_root)
            .unwrap_or_else(|_| PathBuf::from(workspace_root.clone()));
        let crate_name =
            Self::read_crate_name(&workspace_root_path).unwrap_or_else(|| String::from("crate"));
        LspClient {
            communicator: Box::new(framed),
            message_builder,
            workspace_root,
            workspace_root_path,
            crate_name,
        }
    }

    fn read_crate_name(workspace_root_path: &Path) -> Option<String> {
        let cargo = workspace_root_path.join("Cargo.toml");
        let text = std::fs::read_to_string(cargo).ok()?;
        let mut in_package = false;
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') {
                in_package = trimmed == "[package]";
                continue;
            }
            if in_package && trimmed.starts_with("name") {
                let (_, rhs) = trimmed.split_once('=')?;
                let value = rhs.trim().trim_matches('"').trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
        None
    }

    pub(crate) fn workspace_root_path(&self) -> &Path {
        &self.workspace_root_path
    }

    pub(crate) fn crate_name(&self) -> &str {
        &self.crate_name
    }

    pub(crate) fn is_uri_in_workspace(&self, uri: &lsp_types::Url) -> bool {
        let Ok(path) = uri.to_file_path() else {
            return false;
        };
        let normalized = std::fs::canonicalize(&path).unwrap_or(path);
        normalized.starts_with(&self.workspace_root_path)
    }

    pub async fn initialize(&mut self) -> anyhow::Result<()> {
        let request = self.message_builder.initialize(&self.workspace_root)?;
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
        let symbols = self.get_workspace_function_symbols().await?;
        if symbols.is_empty() {
            return Err(anyhow::anyhow!(
                "workspace function symbols are not ready yet"
            ));
        }

        for symbol in symbols {
            println!("Function: {}", symbol.name);
        }

        Ok(())
    }

    pub(crate) async fn get_workspace_function_symbols(
        &mut self,
    ) -> anyhow::Result<Vec<SymbolInformation>> {
        let request = self
            .message_builder
            .create_request("workspace/symbol", Some(serde_json::json!({"query": ""})))?;

        let id = self.communicator.send_request(request).await?;
        let response = self
            .communicator
            .receive_response_with_timeout(id, Some(Duration::from_secs(10)))
            .await?;

        match response {
            Message::Response(response) => {
                let result = response
                    .result
                    .ok_or_else(|| anyhow::anyhow!("workspace/symbol response has no result"))?;
                let symbols: Vec<SymbolInformation> = serde_json::from_value(result)?;
                Ok(symbols
                    .into_iter()
                    .filter(|s| {
                        (s.kind == SymbolKind::FUNCTION || s.kind == SymbolKind::METHOD)
                            && self.is_uri_in_workspace(&s.location.uri)
                    })
                    .collect())
            }
            Message::Error(_) => Ok(vec![]),
            Message::Notification(_) => Ok(vec![]),
        }
    }

    pub(crate) async fn find_function_symbol(
        &mut self,
        query: &str,
    ) -> anyhow::Result<Option<SymbolInformation>> {
        let request = self.message_builder.create_request(
            "workspace/symbol",
            Some(serde_json::json!({"query": query})),
        )?;

        let id = self.communicator.send_request(request).await?;
        let response = self
            .communicator
            .receive_response_with_timeout(id, Some(Duration::from_secs(10)))
            .await?;

        match response {
            Message::Response(response) => {
                let result = response
                    .result
                    .ok_or_else(|| anyhow::anyhow!("workspace/symbol response has no result"))?;
                let symbols: Vec<SymbolInformation> = serde_json::from_value(result)?;

                let exact = symbols
                    .iter()
                    .find(|s| {
                        (s.kind == SymbolKind::FUNCTION || s.kind == SymbolKind::METHOD)
                            && s.name == query
                            && self.is_uri_in_workspace(&s.location.uri)
                    })
                    .cloned();
                if exact.is_some() {
                    return Ok(exact);
                }

                let partial = symbols
                    .iter()
                    .find(|s| {
                        (s.kind == SymbolKind::FUNCTION || s.kind == SymbolKind::METHOD)
                            && self.is_uri_in_workspace(&s.location.uri)
                    })
                    .cloned();
                Ok(partial)
            }
            Message::Error(_) => Ok(None),
            Message::Notification(_) => Ok(None),
        }
    }

    pub(crate) async fn prepare_call_hierarchy(
        &mut self,
        symbol: &SymbolInformation,
    ) -> anyhow::Result<Vec<CallHierarchyItem>> {
        let params = serde_json::json!({
            "textDocument": {
                "uri": symbol.location.uri
            },
            "position": symbol.location.range.start
        });

        let request = self
            .message_builder
            .create_request("textDocument/prepareCallHierarchy", Some(params))?;

        let id = self.communicator.send_request(request).await?;
        let response = self
            .communicator
            .receive_response_with_timeout(id, Some(Duration::from_secs(10)))
            .await?;

        match response {
            Message::Response(response) => {
                let result = response.result.ok_or_else(|| {
                    anyhow::anyhow!("prepareCallHierarchy response has no result")
                })?;
                let items: Vec<CallHierarchyItem> = serde_json::from_value(result)?;
                Ok(items)
            }
            Message::Error(_) => Ok(vec![]),
            Message::Notification(_) => Ok(vec![]),
        }
    }

    pub(crate) async fn get_outgoing_calls(
        &mut self,
        item: &CallHierarchyItem,
    ) -> anyhow::Result<Vec<CallHierarchyOutgoingCall>> {
        let request = self.message_builder.create_request(
            "callHierarchy/outgoingCalls",
            Some(serde_json::json!({"item": item})),
        )?;

        let id = self.communicator.send_request(request).await?;
        let response = self
            .communicator
            .receive_response_with_timeout(id, Some(Duration::from_secs(10)))
            .await?;

        match response {
            Message::Response(response) => {
                if let Some(result) = response.result {
                    let calls: Vec<CallHierarchyOutgoingCall> = serde_json::from_value(result)?;
                    Ok(calls)
                } else {
                    Ok(vec![])
                }
            }
            Message::Error(_) => Ok(vec![]),
            Message::Notification(_) => Ok(vec![]),
        }
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
