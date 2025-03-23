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

#[derive(Serialize, Deserialize, Debug)]
pub struct Notification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<serde_json::Value>,
}
pub enum Message {
    ResponseMessage(ResponseMessage),
    ResponseError(ResponseError),
    Notification(Notification),
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

    pub async fn receive_message(&mut self) -> Result<Message, Box<dyn std::error::Error>> {
        let buffer = self.read_message_buffer().await?;
        let json: serde_json::Value = serde_json::from_str(&buffer)?;

        if let Some(notification) = self.parse_notification(&json)? {
            return Ok(Message::Notification(notification));
        }

        if let Some(response) = self.parse_response(&json)? {
            return Ok(response);
        }

        Err("Invalid header".into())
    }

    async fn read_message_buffer(&mut self) -> Result<String, Box<dyn std::error::Error>> {
        let mut length = vec![0; 1024];
        let count = self.reader.read(&mut length).await?;
        let length_str = String::from_utf8_lossy(&length[..count]);

        if !length_str.starts_with("Content-Length: ") {
            return Err("Invalid header".into());
        }

        let content_length = &length_str[16..];
        let content_length = content_length.trim().parse::<usize>()?;

        let mut buffer = vec![0; content_length];
        let count = self.reader.read(&mut buffer).await?;
        let buffer = buffer[..count].to_vec();
        let buffer = String::from_utf8(buffer)?;

        Ok(buffer)
    }

    fn parse_notification(
        &self,
        json: &serde_json::Value,
    ) -> Result<Option<Notification>, Box<dyn std::error::Error>> {
        if json.get("method").is_some() {
            let notification: Notification = serde_json::from_value(json.clone())?;
            return Ok(Some(notification));
        }
        Ok(None)
    }

    fn parse_response(
        &self,
        json: &serde_json::Value,
    ) -> Result<Option<Message>, Box<dyn std::error::Error>> {
        if json.get("id").is_some() {
            if json.get("result").is_some() {
                let response: ResponseMessage = serde_json::from_value(json.clone())?;
                return Ok(Some(Message::ResponseMessage(response)));
            } else {
                let response: ResponseError = serde_json::from_value(json.clone())?;
                return Ok(Some(Message::ResponseError(response)));
            }
        }
        Ok(None)
    }
}
