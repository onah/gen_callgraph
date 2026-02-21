use anyhow::anyhow;
use std::fs;
use std::process::Stdio;
use tokio::io::BufReader;
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::time::{sleep, Duration};

use crate::cli::Config;
use crate::code_analysis::CodeAnalyzer;
use crate::lsp;
use crate::lsp::stdio_transport::StdioTransport;

pub async fn run(config: Config) -> anyhow::Result<()> {
    let (_child, writer, reader) = start_rust_analyzer("rust-analyzer", &[])?;
    let stdio = StdioTransport::new(writer, reader);

    let lsp_client = lsp::LspClient::new(Box::new(stdio), config.workspace);
    let mut code_analyzer = CodeAnalyzer::new(lsp_client);

    let _result = async {
        match code_analyzer.initialize().await {
            Ok(_) => println!("Initialization Success"),
            Err(e) => eprintln!("Initialization Error: {:?}", e),
        };

        match get_all_function_list_with_retry(&mut code_analyzer, 10, Duration::from_secs(1)).await
        {
            Ok(_) => println!("Function list Success"),
            Err(e) => eprintln!("Function list Error: {:?}", e),
        }

        match code_analyzer
            .generate_call_graph_dot(&config.entry_function)
            .await
        {
            Ok(dot) => {
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

async fn get_all_function_list_with_retry(
    code_analyzer: &mut CodeAnalyzer,
    max_attempts: usize,
    interval: Duration,
) -> anyhow::Result<()> {
    for attempt in 1..=max_attempts {
        match code_analyzer.get_all_function_list().await {
            Ok(()) => return Ok(()),
            Err(e) if attempt < max_attempts => {
                eprintln!(
                    "Function list attempt {}/{} failed: {:?}. Retrying...",
                    attempt, max_attempts, e
                );
                sleep(interval).await;
            }
            Err(e) => return Err(e),
        }
    }

    unreachable!()
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
