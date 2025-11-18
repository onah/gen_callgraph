mod code_analysis;
mod lsp;

use code_analysis::CodeAnalyzer;
use lsp::stdio_transport::StdioTransport;
use lsp::transport;
use std::{thread, time};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (_child, writer, reader) = transport::start_rust_analyzer("rust-analyzer", &[])
        .await
        .expect("Failed to start rust-analyzer");

    let stdio = StdioTransport::new(writer, reader);
    // Use the transport-based constructor so higher layers can provide transports.
    let lsp_client = lsp::LspClient::new(Box::new(stdio));
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

        //code_analyzer.get_main_function_location().await?;
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    if let Err(e) = code_analyzer.shutdown().await {
        eprintln!("Error: {:?}", e);
    }

    Ok(())
}
