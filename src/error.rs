//! Error types for gen_callgraph operations.
//!
//! # Error hierarchy
//!
//! ```text
//! CallGraphError (top-level, wraps everything below)
//! ├── LspError     — LSP communication failures
//! ├── SymbolError  — symbol resolution failures
//! ├── Io           — std::io::Error
//! └── Other        — anyhow catch-all
//! ```

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
    /// The rust-analyzer process failed to start or returned an error during the
    /// `initialize` handshake. Verify that `rust-analyzer` is installed and in PATH,
    /// then run `cargo check` to ensure the project compiles.
    #[error("Failed to initialize LSP server: {0}")]
    InitializationFailed(String),

    /// An LSP request did not receive a response within the configured duration.
    /// rust-analyzer may still be indexing the workspace; the tool retries automatically
    /// in most cases.
    #[error("LSP server communication timeout after {timeout:?}")]
    Timeout { timeout: std::time::Duration },

    /// The server returned a JSON-RPC error for a request. Check that the project
    /// compiles (`cargo check`) and inspect the rust-analyzer server logs for details.
    #[error("LSP request '{method}' failed: {reason}")]
    RequestFailed { method: String, reason: String },

    /// The OS could not spawn the rust-analyzer process. Install it with
    /// `rustup component add rust-analyzer` and verify it is on PATH.
    #[error("Failed to start LSP server process: {0}")]
    ProcessStartFailed(String),

    /// The server returned a response that could not be deserialized into the expected
    /// type. This typically indicates a rust-analyzer version mismatch.
    #[error("LSP server returned invalid response for '{method}': {reason}")]
    InvalidResponse { method: String, reason: String },

    /// The server returned an error or unexpected result during the `shutdown` sequence.
    /// Usually safe to ignore — the process will be dropped regardless.
    #[error("LSP server shutdown failed: {0}")]
    ShutdownFailed(String),
}

/// Symbol resolution errors
#[derive(Error, Debug)]
pub enum SymbolError {
    /// The named entry function does not appear in `workspace/symbol` results after all
    /// retry attempts. Check spelling, ensure the function exists in the workspace, and
    /// allow time for rust-analyzer indexing to complete.
    #[error("Entry function '{name}' not found in workspace")]
    EntryFunctionNotFound { name: String },

    /// The function was found but `textDocument/prepareCallHierarchy` returned an empty
    /// list. The function may have no outgoing calls, or rust-analyzer does not support
    /// call hierarchy for this particular construct.
    #[error("No call hierarchy root found for function '{name}'")]
    NoCallHierarchyRoot { name: String },

    /// A symbol with the requested name exists but has a non-function `SymbolKind`.
    /// Use a more specific name or qualify it (e.g. `MyStruct::method`).
    #[error("Symbol '{name}' exists but is not a function (kind: {kind:?})")]
    NotAFunction {
        name: String,
        kind: lsp_types::SymbolKind,
    },
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
