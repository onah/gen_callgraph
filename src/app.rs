use std::fs;
use tokio::time::Duration;

use crate::call_graph_builder::CodeAnalyzer;
use crate::cli::Config;
use crate::lsp;
use crate::lsp::stdio_transport::spawn_lsp_process;

pub async fn run(config: Config) -> anyhow::Result<()> {
    let (_child, stdio) = spawn_lsp_process("rust-analyzer", &[])?;

    let lsp_client = lsp::LspClient::new(Box::new(stdio), config.workspace);
    let mut code_analyzer = CodeAnalyzer::new(lsp_client);

    let _result = async {
        match code_analyzer
            .initialize(Some(Duration::from_secs(10)))
            .await
        {
            Ok(_) => println!("Initialization Success"),
            Err(e) => eprintln!("Initialization Error: {:?}", e),
        };

        // Wait for rust-analyzer to index the workspace
        // We need to wait for multiple notifications to ensure indexing is complete
        println!("Waiting for rust-analyzer to index the workspace...");
        for i in 0..50 {
            match code_analyzer
                .wait_notification(Some(Duration::from_millis(500)))
                .await
            {
                Ok(_) => {
                    if i % 5 == 0 {
                        println!("  Still indexing... ({} notifications received)", i + 1);
                    }
                }
                Err(_) => {
                    // Timeout means no more notifications for a while
                    if i > 5 {
                        println!("  Indexing appears complete (no notifications for 500ms)");
                        break;
                    }
                }
            }
        }

        // Give a bit more time to settle
        println!("Waiting additional 2 seconds for indexing to complete...");
        tokio::time::sleep(Duration::from_secs(2)).await;

        match code_analyzer
            .generate_call_graph(&config.entry_function)
            .await
        {
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

        Ok::<(), anyhow::Error>(())
    }
    .await;

    if let Err(e) = code_analyzer.shutdown().await {
        eprintln!("Error: {:?}", e);
    }

    Ok(())
}
