//! # Client Layer
//!
//! Provides a high-level LSP client API for use by the rest of the application.
//!
//! ## Responsibilities
//! - Typed methods corresponding to individual LSP protocol methods
//! - Timeout management
//! - Workspace utilities (e.g. checking whether a URI falls within the workspace)
//!
//! ## Usage
//! Create an instance with [`LspClient::new`], start the session with
//! [`LspClient::initialize`], and call [`LspClient::shutdown`] when done.
//!
//! ```ignore
//! let (child, stdio) = spawn_lsp_process("rust-analyzer", &[])?;
//! let mut client = LspClient::new(Box::new(stdio), workspace_root);
//! client.initialize(Some(Duration::from_secs(10))).await?;
//! // ... call individual LSP methods ...
//! client.shutdown().await?;
//! ```

use crate::lsp::lsp_protocol::{FramedBox, FramedTransport};
use crate::lsp::message_creator::MessageBuilder;
use crate::lsp::types::{Message, Notification};
use lsp_types::{
    CallHierarchyItem, CallHierarchyOutgoingCall, DocumentSymbol, InitializeResult,
    SymbolInformation,
};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// LSP client.
///
/// Wraps the Protocol Layer ([`FramedTransport`]) and exposes individual LSP
/// methods as typed async methods.
pub struct LspClient {
    communicator: Box<dyn FramedTransport + Send + Sync>,
    message_builder: MessageBuilder,
    workspace_root: String,
    workspace_root_path: PathBuf,
    #[allow(dead_code)]
    crate_name: String,
}

impl LspClient {
    /// Creates a new [`LspClient`] with the given transport and workspace root.
    ///
    /// `transport` must implement [`crate::lsp::transport::LspTransport`].
    /// Typically this is a `StdioTransport` obtained from
    /// [`crate::lsp::stdio_transport::spawn_lsp_process`].
    pub fn new(
        transport: Box<dyn crate::lsp::transport::LspTransport + Send + Sync>,
        workspace_root: String,
    ) -> Self {
        let message_builder = MessageBuilder::new();
        let framed = FramedBox::new(transport);
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

    /// Initializes the LSP session.
    ///
    /// Sends the `initialize` request and, on success, sends the `initialized`
    /// notification. Other methods must not be called before this succeeds.
    pub async fn initialize(
        &mut self,
        timeout: Option<Duration>,
    ) -> anyhow::Result<InitializeResult> {
        let workspace_path = self
            .workspace_root_path
            .to_str()
            .unwrap_or(&self.workspace_root)
            .to_string();
        let request = self.message_builder.initialize(&workspace_path)?;
        let response = self.communicator.send_and_wait(request, timeout).await?;

        let result = match response {
            Message::Response(resp) => {
                let value = resp
                    .result
                    .ok_or_else(|| anyhow::anyhow!("protocol:initialize response has no result"))?;
                serde_json::from_value::<InitializeResult>(value)?
            }
            Message::Error(error) => {
                return Err(Self::protocol_error_for_response("initialize", error.error))
            }
            Message::Notification(note) => {
                return Err(Self::protocol_error_unexpected_notification(
                    "initialize",
                    &note.method,
                ))
            }
        };

        let initialized_notification = self.message_builder.initialized_notification()?;
        self.communicator
            .send_notification(initialized_notification)
            .await?;

        Ok(result)
    }

    /// Sends a `workspace/symbol` request and returns the matching symbols.
    pub(crate) async fn workspace_symbol(
        &mut self,
        query: &str,
    ) -> anyhow::Result<Vec<SymbolInformation>> {
        self.request(
            "workspace/symbol",
            serde_json::json!({"query": query}),
            Some(Duration::from_secs(10)),
        )
        .await
    }

    /// Sends a `textDocument/prepareCallHierarchy` request.
    pub(crate) async fn text_document_prepare_call_hierarchy(
        &mut self,
        symbol: &SymbolInformation,
    ) -> anyhow::Result<Vec<CallHierarchyItem>> {
        let params = serde_json::json!({
            "textDocument": {
                "uri": symbol.location.uri
            },
            "position": symbol.location.range.start
        });

        self.request(
            "textDocument/prepareCallHierarchy",
            params,
            Some(Duration::from_secs(10)),
        )
        .await
    }

    /// Sends a `callHierarchy/outgoingCalls` request.
    pub(crate) async fn call_hierarchy_outgoing_calls(
        &mut self,
        item: &CallHierarchyItem,
    ) -> anyhow::Result<Vec<CallHierarchyOutgoingCall>> {
        self.request(
            "callHierarchy/outgoingCalls",
            serde_json::json!({"item": item}),
            Some(Duration::from_secs(10)),
        )
        .await
    }

    /// Sends a `textDocument/didOpen` notification.
    pub(crate) async fn text_document_did_open(
        &mut self,
        uri: &lsp_types::Url,
        language_id: &str,
        text: String,
    ) -> anyhow::Result<()> {
        let params = serde_json::json!({
            "textDocument": {
                "uri": uri,
                "languageId": language_id,
                "version": 1,
                "text": text
            }
        });

        let notification = self
            .message_builder
            .create_notification("textDocument/didOpen", params)?;
        self.communicator.send_notification(notification).await?;
        Ok(())
    }

    /// Sends a `textDocument/documentSymbol` request and returns the document symbols.
    pub(crate) async fn text_document_document_symbol(
        &mut self,
        uri: &lsp_types::Url,
    ) -> anyhow::Result<Vec<DocumentSymbol>> {
        let params = serde_json::json!({
            "textDocument": {
                "uri": uri
            }
        });

        self.request(
            "textDocument/documentSymbol",
            params,
            Some(Duration::from_secs(10)),
        )
        .await
    }

    /// Waits for the next server-to-client notification.
    ///
    /// Returns a timeout error if `timeout` is `Some` and the duration elapses.
    pub async fn wait_notification(
        &mut self,
        timeout: Option<Duration>,
    ) -> anyhow::Result<Notification> {
        self.communicator.wait_notification(timeout).await
    }

    /// Shuts down the LSP session gracefully.
    ///
    /// Sends the `shutdown` request and, on success, sends the `exit` notification.
    pub async fn shutdown(&mut self) -> anyhow::Result<()> {
        let request = self.message_builder.create_request("shutdown", ())?;

        let response = self
            .communicator
            .send_and_wait(request, Some(Duration::from_secs(10)))
            .await?;
        let shutdown_result = Self::expect_response("shutdown", response)?;
        if shutdown_result.is_some() {
            return Err(anyhow::anyhow!(
                "protocol:shutdown expected null result, got non-null result"
            ));
        }

        let notification = self.message_builder.create_notification("exit", ())?;
        self.communicator.send_notification(notification).await?;

        Ok(())
    }

    /// Returns `true` if the given URI falls within the workspace root.
    pub(crate) fn is_uri_in_workspace(&self, uri: &lsp_types::Url) -> bool {
        let Ok(path) = uri.to_file_path() else {
            return false;
        };
        let normalized = std::fs::canonicalize(&path).unwrap_or(path);
        normalized.starts_with(&self.workspace_root_path)
    }

    #[allow(dead_code)]
    pub(crate) fn workspace_root_path(&self) -> &Path {
        &self.workspace_root_path
    }

    #[allow(dead_code)]
    pub(crate) fn crate_name(&self) -> &str {
        &self.crate_name
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Generic helper for sending an LSP request and deserializing the response.
    ///
    /// 1. Builds the request.
    /// 2. Sends it via the Protocol Layer and awaits the response.
    /// 3. Matches the response variant and deserializes the result into `R`.
    async fn request<P, R>(
        &mut self,
        method: &str,
        params: P,
        timeout: Option<Duration>,
    ) -> anyhow::Result<R>
    where
        P: serde::Serialize,
        R: serde::de::DeserializeOwned,
    {
        let request = self.message_builder.create_request(method, params)?;
        let response = self.communicator.send_and_wait(request, timeout).await?;

        match response {
            Message::Response(resp) => {
                let result = resp
                    .result
                    .ok_or_else(|| anyhow::anyhow!("protocol:{} response has no result", method))?;
                Ok(serde_json::from_value(result)?)
            }
            Message::Error(error) => Err(Self::protocol_error_for_response(method, error.error)),
            Message::Notification(note) => Err(Self::protocol_error_unexpected_notification(
                method,
                &note.method,
            )),
        }
    }

    fn expect_response(
        method: &str,
        response: Message,
    ) -> anyhow::Result<Option<serde_json::Value>> {
        match response {
            Message::Response(resp) => Ok(resp.result),
            Message::Error(error) => Err(Self::protocol_error_for_response(method, error.error)),
            Message::Notification(note) => Err(Self::protocol_error_unexpected_notification(
                method,
                &note.method,
            )),
        }
    }

    fn protocol_error_for_response(
        method: &str,
        error: Option<serde_json::Value>,
    ) -> anyhow::Error {
        anyhow::anyhow!(
            "protocol:{} returned error response: {}",
            method,
            error
                .map(|e| e.to_string())
                .unwrap_or_else(|| String::from("null"))
        )
    }

    fn protocol_error_unexpected_notification(
        method: &str,
        notification_method: &str,
    ) -> anyhow::Error {
        anyhow::anyhow!(
            "protocol:{} got unexpected notification response: {}",
            method,
            notification_method
        )
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
}
