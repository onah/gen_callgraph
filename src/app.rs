use anyhow::anyhow;
use std::fs;
use std::process::Stdio;
use tokio::io::BufReader;
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::time::Duration;

use crate::call_graph_builder::CodeAnalyzer;
use crate::cli::Config;
use crate::lsp;
use crate::lsp::stdio_transport::StdioTransport;
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
    let (_child, writer, reader) = start_rust_analyzer("rust-analyzer", &[])?;
    let stdio = StdioTransport::new(writer, reader);

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

fn start_rust_analyzer(
    exe: &str,
    args: &[&str],
) -> anyhow::Result<(Child, ChildStdin, BufReader<ChildStdout>)> {
    let mut cmd = Command::new(exe);
    for a in args {
        cmd.arg(a);
    }

    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    let writer = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("failed to take child stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("failed to take child stdout"))?;
    let reader = BufReader::new(stdout);

    Ok((child, writer, reader))
}
