//! LSP session lifecycle management.
//!
//! [`LspSession`] owns the LSP child process and [`lsp::LspClient`], and manages the
//! full session lifecycle: process startup, protocol initialization, workspace indexing
//! wait, and graceful shutdown.
//!
//! # Separation of concerns
//!
//! This module handles *when* and *how* the LSP server is alive.
//! It contains no call-graph logic; that belongs in [`crate::call_graph_builder`].
//! [`crate::app`] wires the two together.

use std::time::Duration;
use tokio::process::Child;

use crate::cli::Config;
use crate::error::LspError;
use crate::lsp;
use crate::lsp::stdio_transport::spawn_lsp_process;

/// Owns the LSP server process and client for the duration of a session.
///
/// Create with [`LspSession::start`], pass [`LspSession::client_mut`] to
/// [`crate::call_graph_builder::CallGraphBuilder`], then call [`LspSession::shutdown`].
pub struct LspSession {
    client: lsp::LspClient,
    /// Keeps the rust-analyzer process alive for the duration of the session.
    _child: Child,
}

impl LspSession {
    /// Spawns the LSP server, initializes the protocol session, and waits for
    /// workspace indexing to complete.
    ///
    /// Returns an `LspSession` ready for use, or an error if the process could not
    /// be started or the initialization handshake failed.
    pub async fn start(config: &Config) -> anyhow::Result<Self> {
        let (_child, stdio) = spawn_lsp_process("rust-analyzer", &[])
            .map_err(|e| LspError::ProcessStartFailed(e.to_string()))?;

        let mut client = lsp::LspClient::new(Box::new(stdio), config.workspace.clone());
        client.initialize(Some(Duration::from_secs(10))).await?;
        println!("Initialization Success");

        wait_for_indexing(&mut client).await?;

        Ok(LspSession { client, _child })
    }

    /// Returns a mutable reference to the underlying [`lsp::LspClient`].
    ///
    /// Pass this to [`crate::call_graph_builder::CallGraphBuilder::new`].
    pub fn client_mut(&mut self) -> &mut lsp::LspClient {
        &mut self.client
    }

    /// Shuts down the LSP session gracefully.
    ///
    /// Sends `shutdown` + `exit` to the server. A shutdown error is logged but does not
    /// propagate because the process will be dropped regardless.
    pub async fn shutdown(mut self) {
        if let Err(e) = self.client.shutdown().await {
            eprintln!("Shutdown error: {:?}", e);
        }
    }
}

/// Polls for server notifications until indexing appears complete, then waits briefly
/// for the index to settle.
///
/// rust-analyzer streams notifications while indexing. We wait until 500 ms passes
/// without a notification (after seeing at least 5), which indicates the burst is over,
/// then sleep an additional 2 s.
///
/// Returns `Err` if the LSP transport fails (e.g. rust-analyzer process exits unexpectedly).
/// A `Timeout` — no notification arriving within 500 ms — is a normal quiet period, not an error.
async fn wait_for_indexing(client: &mut lsp::LspClient) -> anyhow::Result<()> {
    println!("Waiting for rust-analyzer to index the workspace...");
    for i in 0..50 {
        match client
            .wait_notification(Some(Duration::from_millis(500)))
            .await
        {
            Ok(_) => {
                if i % 5 == 0 {
                    println!("  Still indexing... ({} notifications received)", i + 1);
                }
            }
            Err(LspError::Timeout { .. }) => {
                if i > 5 {
                    println!("  Indexing appears complete (no notifications for 500ms)");
                    break;
                }
            }
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "LSP transport failed during indexing: {}",
                    e
                ));
            }
        }
    }
    println!("Waiting additional 2 seconds for indexing to complete...");
    tokio::time::sleep(Duration::from_secs(2)).await;
    Ok(())
}
