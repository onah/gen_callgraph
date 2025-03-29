use serde::{Deserialize, Serialize};
// use tokio::io::AsyncBufReadExt;
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
    Response(ResponseMessage),
    Error(ResponseError),
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

        Err("Other Message".into())
    }

    async fn read_message_buffer(&mut self) -> Result<String, Box<dyn std::error::Error>> {
        let mut header_buffer = Vec::new();

        // `&mut self.reader`を直接使用
        loop {
            let mut byte = [0; 1];
            self.reader.read_exact(&mut byte).await?;
            header_buffer.push(byte[0]);

            // ヘッダーの終わりを検出
            if header_buffer.ends_with(b"\r\n\r\n") {
                break;
            }
        }

        // ヘッダーを文字列に変換
        let header_str = String::from_utf8(header_buffer)?;
        println!("Header: {}", header_str);

        // Content-Lengthを取得
        let content_length = self.get_content_length(&header_str)?;
        println!("Parsed Content-Length: {}", content_length);

        // ペイロード部分を読み取る
        let mut payload_buffer = vec![0; content_length];
        self.reader.read_exact(&mut payload_buffer).await?;

        // ペイロードを文字列に変換
        Ok(String::from_utf8(payload_buffer)?)
    }

    fn get_content_length(&self, header: &str) -> Result<usize, Box<dyn std::error::Error>> {
        // "Content-Length: " で始まる行を探す
        if let Some(content_length_line) = header
            .lines()
            .find(|line| line.starts_with("Content-Length: "))
        {
            // "Content-Length: " の部分を取り除き、数値部分を抽出
            let content_length = content_length_line["Content-Length: ".len()..]
                .trim() // 前後の空白を削除
                .parse::<usize>()?; // 数値に変換
            Ok(content_length)
        } else {
            Err("Content-Length header not found".into())
        }
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
                return Ok(Some(Message::Response(response)));
            } else {
                let response: ResponseError = serde_json::from_value(json.clone())?;
                return Ok(Some(Message::Error(response)));
            }
        }
        Ok(None)
    }
}
