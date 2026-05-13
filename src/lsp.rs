//! # LSP Module
//!
//! Provides Language Server Protocol (LSP) client functionality in a three-layer architecture.
//!
//! ## Architecture
//!
//! - **Client Layer** (`lsp_client.rs`)
//!   - Typed high-level LSP methods
//!   - Timeout management
//! - **Protocol Layer** (`lsp_protocol.rs`, `message_creator.rs`, `message_parser.rs`)
//!   - JSON-RPC 2.0 protocol handling
//!   - Async request/response management
//!   - Auto-response to server requests
//! - **Transport Layer** (`transport.rs`, `stdio_transport.rs`)
//!   - Content-Length framing
//!   - stdio read/write
//!
//! ## Layer Details
//!
//! ### Transport Layer
//! Low-level communication layer abstracted by the [`transport::LspTransport`] trait.
//! Has no knowledge of the LSP protocol; responsible only for `Content-Length` header
//! framing and raw byte I/O. [`stdio_transport::StdioTransport`] is the standard
//! concrete implementation.
//!
//! ### Protocol Layer
//! Protocol-handling layer abstracted by the [`lsp_protocol::FramedTransport`] trait.
//! Responsible for interpreting JSON-RPC 2.0 messages, asynchronously mapping request
//! IDs to responses, and auto-responding to server-initiated requests.
//! [`lsp_protocol::FramedBox`] is the concrete implementation, providing I/O
//! multiplexing via a background task.
//!
//! ### Client Layer
//! High-level user-facing API provided by [`lsp_client::LspClient`].
//! Exposes individual LSP methods such as `workspace/symbol` and
//! `callHierarchy/outgoingCalls` as typed async methods.
//!
//! ## Typical Usage
//!
//! ```ignore
//! use crate::lsp::LspClient;
//! use crate::lsp::stdio_transport::spawn_lsp_process;
//!
//! let (child, stdio) = spawn_lsp_process("rust-analyzer", &[])?;
//! let mut client = LspClient::new(Box::new(stdio), workspace_root);
//! client.initialize(Some(Duration::from_secs(10))).await?;
//! let symbols = client.workspace_symbol("my_function").await?;
//! client.shutdown().await?;
//! ```

// ---------------------------------------------------------------------------
// Submodules
// ---------------------------------------------------------------------------

/// Transport Layer: concrete stdio implementation
pub mod stdio_transport;
/// Transport Layer: low-level communication abstraction
pub mod transport;

/// Protocol Layer: JSON-RPC message handling and async management
pub mod lsp_protocol;
/// Protocol Layer: outgoing message construction
pub mod message_creator;
/// Protocol Layer: incoming message parsing
pub mod message_parser;

/// Client Layer: high-level user-facing API
pub mod lsp_client;

/// Shared LSP type definitions (Request, Response, Notification, etc.)
pub mod types;

// ---------------------------------------------------------------------------
// Public API re-exports
// ---------------------------------------------------------------------------

/// Entry point for the LSP client.
pub use lsp_client::LspClient;
