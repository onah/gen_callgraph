use lsp_types::{ClientCapabilities, InitializeParams, Url, WorkspaceFolder};
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::process::ChildStdin;
use tokio::process::ChildStdout;

#[derive(Serialize, Deserialize)]
struct Request {
    jsonrpc: String,
    id: i32,
    method: String,
    params: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Notification {
    jsonrpc: String,
    method: String,
    params: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug)]
struct ResponseMessage {
    jsonrpc: String,
    id: i32,
    result: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
struct ResponseError {
    jsonrpc: String,
    id: i32,
    error: Option<serde_json::Value>,
}

enum Response {
    ResponseMessage(ResponseMessage),
    ResponseError(ResponseError),
}

pub struct CommunicateLSP {
    writer: ChildStdin,
    reader: BufReader<ChildStdout>,
}

impl CommunicateLSP {
    pub fn new(writer: ChildStdin, reader: BufReader<ChildStdout>) -> Self {
        CommunicateLSP { writer, reader }
    }

    pub async fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let initialize_params = InitializeParams {
            process_id: Some(std::process::id()),
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: Url::parse("file:///c:/Users/PCuser/Work/rust/sample-lsp/sample3/")?,
                name: String::from("sample3"),
            }]),
            capabilities: ClientCapabilities::default(),
            ..InitializeParams::default()
        };
        let initialize_params = initialize_params;

        let request = Request {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "initialize".to_string(),
            params: Some(serde_json::to_value(&initialize_params)?),
        };

        send_message(&mut self.writer, &request).await?;
        recieve_response(&mut self.reader).await?;

        let initialized_notification: Notification = Notification {
            jsonrpc: "2.0".to_string(),
            method: "initialized".to_string(),
            params: Some(serde_json::json!({})),
        };

        send_message(&mut self.writer, &initialized_notification)
            .await
            .unwrap();

        Ok(())
    }

    pub async fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let request = Request {
            jsonrpc: "2.0".to_string(),
            id: 2,
            method: "shutdown".to_string(),
            params: None,
        };

        send_message(&mut self.writer, &request).await?;
        recieve_response(&mut self.reader).await?;

        let notification = Notification {
            jsonrpc: "2.0".to_string(),
            method: "exit".to_string(),
            params: None,
        };

        send_message(&mut self.writer, &notification).await?;

        Ok(())
    }

    pub async fn get_main_function_location(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // send textDocumetn/didOpen notification
        let file_path = "c:/Users/PCuser/Work/rust/sample-lsp/sample3/src/main.rs";
        let file_contents = fs::read_to_string(file_path).unwrap();

        let did_open_notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": format!("file://{}", file_path),
                    "languageId": "rust",
                    "version": 1,
                    "text": file_contents
                }
            }
        });

        send_message(&mut self.writer, &did_open_notification)
            .await
            .unwrap();

        // send textDocument/documentSymbol request
        let request = Request {
            jsonrpc: "2.0".to_string(),
            id: 3,
            method: "textDocument/documentSymbol".to_string(),
            params: Some(serde_json::json!({
                "textDocument": {
                    "uri": "file:///c:/Users/PCuser/Work/rust/sample-lsp/sample3/src/main.rs"
                }
            })),
        };

        send_message(&mut self.writer, &request).await.unwrap();
        let response = recieve_response(&mut self.reader).await.unwrap();

        match response {
            Response::ResponseMessage(response) => {
                println!("{:#?}", response.result.unwrap());
            }
            Response::ResponseError(response) => {
                println!("{:?}", response.error.unwrap());
            }
        }
        Ok(())
    }
}

async fn send_message<T: Serialize>(
    writer: &mut ChildStdin,
    message: &T,
) -> Result<(), Box<dyn std::error::Error>> {
    // Convert the request to a string
    let message = serde_json::to_string(&message)?;

    // Create the header
    let length = message.as_bytes().len();
    let header = format!("Content-Length: {}\r\n\r\n", length);

    // Send the header and the request to the server
    writer.write_all(header.as_bytes()).await?;
    writer.write_all(message.as_bytes()).await?;
    writer.flush().await?;

    Ok(())
}

async fn recieve_response(
    reader: &mut BufReader<ChildStdout>,
) -> Result<Response, Box<dyn std::error::Error>> {
    let mut length = vec![0; 1024];

    // Read the header
    let count = reader.read(&mut length).await?;
    let length_str = String::from_utf8_lossy(&length[..count]);

    // Check if the header is valid
    if length_str.starts_with("Content-Length: ") {
        // Get the content length
        let content_length = &length_str[16..];
        let content_length = content_length.trim().parse::<usize>()?;

        let mut buffer = vec![0; content_length];

        // Read the content
        let count = reader.read(&mut buffer).await?;
        let buffer = buffer[..count].to_vec();
        let buffer = String::from_utf8(buffer)?;

        // Parse the content
        let response: ResponseMessage = serde_json::from_str(&buffer)?;
        if response.result.is_some() {
            return Ok(Response::ResponseMessage(response));
        } else {
            let response: ResponseError = serde_json::from_str(&buffer)?;
            return Ok(Response::ResponseError(response));
        }
    } else {
        Err("Invalid header".into())
    }
}
