//! Orchestration layer. Wires `cli::Config` → `LspSession` → `CallGraphBuilder` →
//! `DotRenderer` → file write.
//!
//! This module contains no domain logic. It is the only place in the codebase that is
//! allowed to connect the independent layers (CLI, LSP session, builder, renderer) together.

use std::fs;

use crate::call_graph_builder::CallGraphBuilder;
use crate::cli::Config;
use crate::lsp_session::LspSession;

pub async fn run(config: Config) -> anyhow::Result<()> {
    let mut session = LspSession::start(&config).await?;

    let graph_result = {
        let mut builder = CallGraphBuilder::new(session.client_mut());
        match &config.entry_function {
            Some(entry) => {
                println!("Generating call graph for entry function: {}", entry);
                builder.generate_call_graph(entry).await
            }
            None => {
                println!("No entry function specified. Generating call graph for all symbols.");
                builder.generate_call_graph_all().await
            }
        }
    };

    match graph_result {
        Ok(graph) => {
            let dot = crate::dot_renderer::to_dot(&graph);
            if let Err(e) = fs::write(&config.output_path, dot) {
                eprintln!("DOT write Error: {:?}", e);
            } else {
                println!("DOT output Success: {}", config.output_path);
            }
        }
        Err(e) => eprintln!("Call graph Error: {:?}", e),
    }

    session.shutdown().await;

    Ok(())
}
