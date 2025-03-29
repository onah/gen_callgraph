mod code_analysis;
mod lsp_client;

use code_analysis::CodeAnalyzer;
use lsp_client::LspClient;
use std::{thread, time};
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

    let _result = async {
        match code_analyzer.initialize().await {
            Ok(_) => println!("Initialization Success"),
            Err(e) => eprintln!("Initialization Error: {:?}", e),
        };
        //code_analyz0er.wait_process().await?;

        thread::sleep(time::Duration::from_secs(10));

        println!("start ger all function list");
        match code_analyzer.get_all_function_list().await {
            Ok(_) => println!("Function list Success"),
            Err(e) => eprintln!("Function list Error: {:?}", e),
        }

        //code_analyzer.get_main_function_location().await?;
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    if let Err(e) = code_analyzer.shutdown().await {
        eprintln!("Error: {:?}", e);
    }

    Ok(())
}
