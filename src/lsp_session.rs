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
        client.initialize().await?;
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

/// Polls for `$/progress` notifications until all open progress tokens are closed,
/// then waits 500 ms for a quiet period to confirm completion.
///
/// rust-analyzer sends `$/progress` with `kind: "begin"` when an indexing phase starts
/// and `kind: "end"` when it finishes. This function tracks the count of open tokens
/// and exits once all have been closed and no new notifications arrive for 500 ms.
///
/// Falls back to a 3 s no-notification timeout (small projects that finish quickly)
/// and a hard 120 s deadline in case the server never signals completion.
async fn wait_for_indexing(client: &mut lsp::LspClient) -> anyhow::Result<()> {
    println!("Waiting for rust-analyzer to index the workspace...");

    let mut active_progress: u32 = 0;
    let mut seen_any_progress = false;
    let mut all_done_since: Option<std::time::Instant> = None;
    let mut last_notification_at = std::time::Instant::now();
    let deadline = std::time::Instant::now() + Duration::from_secs(120);

    loop {
        if std::time::Instant::now() >= deadline {
            println!("  Timed out waiting for indexing (120 s), continuing");
            break;
        }

        match client.try_get_notification() {
            Some(notification) => {
                last_notification_at = std::time::Instant::now();

                if notification.method == "$/progress" {
                    let kind = notification
                        .params
                        .get("value")
                        .and_then(|v| v.get("kind"))
                        .and_then(|k| k.as_str());

                    match kind {
                        Some("begin") => {
                            active_progress += 1;
                            seen_any_progress = true;
                            all_done_since = None;
                            println!("  Indexing in progress (active: {})", active_progress);
                        }
                        Some("end") if active_progress > 0 => {
                            active_progress -= 1;
                            if active_progress == 0 {
                                all_done_since = Some(std::time::Instant::now());
                            }
                        }
                        _ => {}
                    }
                }
            }
            None => {
                // All progress tokens closed — wait for a 500 ms quiet period to confirm.
                if seen_any_progress {
                    if let Some(since) = all_done_since {
                        if since.elapsed() >= Duration::from_millis(500) {
                            println!("  Indexing complete");
                            break;
                        }
                    }
                }
                // Fallback: no notifications at all for 3 s (e.g. very small project).
                if last_notification_at.elapsed() >= Duration::from_secs(3) {
                    println!("  No notifications for 3 s, assuming indexing complete");
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }

    Ok(())
}
