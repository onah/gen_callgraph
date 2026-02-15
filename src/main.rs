mod code_analysis;
mod dot;
mod lsp;
use anyhow::anyhow;
use code_analysis::CodeAnalyzer;
use lsp::stdio_transport::StdioTransport;
use std::env;
use std::fs;
use std::process::Stdio;
use std::{thread, time};
use tokio::io::BufReader;
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();

    //let stdio = StdioTransport::spawn()?;
    let (_child, writer, reader) = start_rust_analyzer("rust-analyzer", &[])?;
    let stdio = StdioTransport::new(writer, reader);

    // Use the transport-based constructor so higher layers can provide transports.
    let workspace = args.get(1).cloned().unwrap_or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|p| p.to_str().map(|s| s.to_string()))
            .unwrap_or_else(|| String::from("."))
    });
    let entry_function = args.get(2).cloned().unwrap_or_else(|| "main".to_string());
    let output_path = args
        .get(3)
        .cloned()
        .unwrap_or_else(|| "callgraph.dot".to_string());

    let lsp_client = lsp::LspClient::new(Box::new(stdio), workspace);
    let mut code_analyzer = CodeAnalyzer::new(lsp_client);

    let _result = async {
        match code_analyzer.initialize().await {
            Ok(_) => println!("Initialization Success"),
            Err(e) => eprintln!("Initialization Error: {:?}", e),
        };
        //code_analyz0er.wait_process().await?;

        thread::sleep(time::Duration::from_secs(10));

        //println!("start ger all function list");
        match code_analyzer.get_all_function_list().await {
            Ok(_) => println!("Function list Success"),
            Err(e) => eprintln!("Function list Error: {:?}", e),
        }

        match code_analyzer.generate_call_graph_dot(&entry_function).await {
            Ok(dot) => {
                if let Err(e) = fs::write(&output_path, dot) {
                    eprintln!("DOT write Error: {:?}", e);
                } else {
                    println!("DOT output Success: {}", output_path);
                }
            }
            Err(e) => eprintln!("Call graph Error: {:?}", e),
        }

        //code_analyzer.get_main_function_location().await?;
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
