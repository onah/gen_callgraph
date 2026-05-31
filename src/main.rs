//! `gen_callgraph` — a CLI tool that generates a function call graph for a Rust workspace
//! using the Language Server Protocol (rust-analyzer).
//!
//! # Module Overview
//!
//! | Module | Responsibility |
//! |---|---|
//! | `cli` | CLI argument parsing. Produces `Config`. |
//! | `lsp` | LSP communication. Sends requests and receives responses. No domain logic. |
//! | `call_graph_builder` | Builds `CallGraph` from LSP results. No output format knowledge. |
//! | `dot_renderer` | Renders `CallGraph` into DOT format string. No LSP/analysis knowledge. |
//! | `app` | Orchestration only. Wires CLI → LSP → Builder → Renderer → file write. |
//! | `main` | Entry point. Parses config and calls `app::run`. |
//!
//! # Data Flow
//!
//! ```text
//! CLI -> App -> CallGraphBuilder -> LspClient
//!                     |
//!                DotRenderer (depends only on CallGraph)
//! ```

mod app;
mod call_graph;
mod call_graph_builder;
mod cli;
mod dot_renderer;
mod error;
mod lsp;
use cli::Cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Cli::from_args().into_config()?;
    app::run(config).await
}
