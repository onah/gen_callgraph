use std::path::PathBuf;
use thiserror::Error;

/// Main error type for gen_callgraph operations
#[derive(Error, Debug)]
pub enum CallGraphError {
    /// LSP communication errors
    #[error("LSP error: {0}")]
    Lsp(#[from] LspError),

    /// Symbol resolution errors
    #[error("Symbol error: {0}")]
    Symbol(#[from] SymbolError),

    /// File I/O errors
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Call graph generation errors
    #[error("Call graph error: {0}")]
    CallGraph(String),

    /// Other errors
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// LSP-specific errors
#[derive(Error, Debug)]
pub enum LspError {
    #[error("Failed to initialize LSP server: {0}")]
    InitializationFailed(String),

    #[error("LSP server communication timeout after {timeout:?}")]
    Timeout { timeout: std::time::Duration },

    #[error("LSP request '{method}' failed: {reason}")]
    RequestFailed { method: String, reason: String },

    #[error("Failed to start LSP server process: {0}")]
    ProcessStartFailed(String),

    #[error("LSP server returned invalid response for '{method}': {reason}")]
    InvalidResponse { method: String, reason: String },

    #[error("LSP server shutdown failed: {0}")]
    ShutdownFailed(String),
}

/// Symbol resolution errors
#[derive(Error, Debug)]
pub enum SymbolError {
    #[error("Entry function '{name}' not found in workspace")]
    EntryFunctionNotFound { name: String },

    #[error("No call hierarchy root found for function '{name}'")]
    NoCallHierarchyRoot { name: String },

    #[error("Symbol '{name}' exists but is not a function (kind: {kind:?})")]
    NotAFunction {
        name: String,
        kind: lsp_types::SymbolKind,
    },

    #[error("Failed to resolve workspace symbols: {0}")]
    WorkspaceSymbolFailed(String),

    #[error("Failed to open document at {path:?}: {reason}")]
    DocumentOpenFailed { path: PathBuf, reason: String },

    #[error("No symbols found in document {path:?}")]
    NoDocumentSymbols { path: PathBuf },
}

impl CallGraphError {
    /// Create a call graph error with a custom message
    pub fn call_graph<S: Into<String>>(message: S) -> Self {
        CallGraphError::CallGraph(message.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = SymbolError::EntryFunctionNotFound {
            name: "main".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Entry function 'main' not found in workspace"
        );
    }

    #[test]
    fn test_lsp_timeout_error() {
        let err = LspError::Timeout {
            timeout: std::time::Duration::from_secs(10),
        };
        assert!(err.to_string().contains("10s"));
    }

    #[test]
    fn test_error_conversion() {
        let symbol_err = SymbolError::EntryFunctionNotFound {
            name: "test".to_string(),
        };
        let call_graph_err: CallGraphError = symbol_err.into();
        // anyhow automatically converts any std::error::Error
        let _anyhow_err: anyhow::Error = call_graph_err.into();
    }
}
