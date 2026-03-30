use std::fs;
use tokio::time::Duration;

use crate::call_graph_builder::CodeAnalyzer;
use crate::cli::Config;
use crate::lsp;
use crate::lsp::stdio_transport::spawn_lsp_process;
use crate::trace;

pub async fn run(config: Config) -> anyhow::Result<()> {
    trace::log(
        "app",
        "run-start",
        &format!(
            "workspace={} entry={} output={}",
            config.workspace, config.entry_function, config.output_path
        ),
    );
    let (_child, stdio) = spawn_lsp_process("rust-analyzer", &[])?;

    let lsp_client = lsp::LspClient::new(Box::new(stdio), config.workspace);
    let mut code_analyzer = CodeAnalyzer::new(lsp_client);

    let _result = async {
        match code_analyzer.initialize().await {
            Ok(_) => println!("Initialization Success"),
            Err(e) => eprintln!("Initialization Error: {:?}", e),
        };

        let _ = code_analyzer
            .wait_notification(Some(Duration::from_millis(1)))
            .await;

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
