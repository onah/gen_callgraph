mod communicate_lsp;

//use lsp_types::InitializeResult;
use std::process::Stdio;
use tokio::io::BufReader;
use tokio::process::Command;

#[tokio::main]
async fn main() {
    let mut child = Command::new("rust-analyzer")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("Failed to execute command");

    let writer = child.stdin.take().unwrap();
    let reader = BufReader::new(child.stdout.take().unwrap());

    let mut communicater = communicate_lsp::CommunicateLSP::new(writer, reader);

    // send initialize request
    communicater.initialize().await.unwrap();
    communicater.get_all_function_list().await.unwrap();
    communicater.get_main_function_location().await.unwrap();

    communicater.shutdown().await.unwrap();

    // Wait for the child process to exit
    let _ = child.wait().await.expect("Failed to wait on child");
}
