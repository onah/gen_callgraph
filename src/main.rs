mod code_analysis;
mod lsp_client;

use code_analysis::CodeAnalyzer;
use lsp_client::LspClient;
use tokio::io::BufReader;
use tokio::process::Command;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut child = Command::new("rust-analyzer")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .expect("Failed to start rust-analyzer");

    let writer = child.stdin.take().unwrap();
    let reader = BufReader::new(child.stdout.take().unwrap());

    let lsp_client = LspClient::new(writer, reader);
    let mut code_analyzer = CodeAnalyzer::new(lsp_client);

    code_analyzer.initialize().await?;
    code_analyzer.get_all_function_list().await?;
    code_analyzer.shutdown().await?;

    Ok(())
}
