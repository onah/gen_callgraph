use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};

#[derive(Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub id: i32,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ResponseMessage {
    pub jsonrpc: String,
    pub id: i32,
    pub result: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
pub struct ResponseError {
    pub jsonrpc: String,
    pub id: i32,
    pub error: Option<serde_json::Value>,
}

pub enum Response {
    ResponseMessage(ResponseMessage),
    ResponseError(ResponseError),
}

pub struct LspClient {
    writer: ChildStdin,
    reader: BufReader<ChildStdout>,
}

impl LspClient {
    pub fn new(writer: ChildStdin, reader: BufReader<ChildStdout>) -> Self {
        LspClient { writer, reader }
    }

    pub async fn send_message<T: Serialize>(
        &mut self,
        message: &T,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let message = serde_json::to_string(&message)?;
        //println!("Sent: {:#?}", message);

        let length = message.as_bytes().len();
        let header = format!("Content-Length: {}\r\n\r\n", length);

        self.writer.write_all(header.as_bytes()).await?;
        self.writer.write_all(message.as_bytes()).await?;
        self.writer.flush().await?;

        Ok(())
    }

    pub async fn receive_response(&mut self) -> Result<Response, Box<dyn std::error::Error>> {
        let mut length = vec![0; 1024];
        let count = self.reader.read(&mut length).await?;
        let length_str = String::from_utf8_lossy(&length[..count]);

        if length_str.starts_with("Content-Length: ") {
            let content_length = &length_str[16..];
            let content_length = content_length.trim().parse::<usize>()?;

            let mut buffer = vec![0; content_length];
            let count = self.reader.read(&mut buffer).await?;
            let buffer = buffer[..count].to_vec();
            let buffer = String::from_utf8(buffer)?;

            let response: ResponseMessage = serde_json::from_str(&buffer)?;
            //println!("Response: {:#?}", response);

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
}
