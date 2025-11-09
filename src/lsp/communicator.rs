use crate::lsp::message_creator::Message;
//use serde::Serialize;
// use tokio::io::AsyncBufReadExt;
use crate::lsp::framed::FramedTransport;
use crate::lsp::protocol::{parse_notification, parse_response, DynError};
use crate::lsp::transport::LspTransport;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};

use super::message_creator::SendMessage;

pub struct Communicator {
    writer: ChildStdin,
    reader: BufReader<ChildStdout>,
}

#[async_trait::async_trait]
impl LspTransport for Communicator {
    async fn send(&mut self, json_body: &str) -> Result<(), DynError> {
        // Communicator's inherent send_message2 returns Box<dyn Error> (not Send+Sync),
        // so wrap its error into DynError here.
        Communicator::send_message2(self, json_body)
            .await
            .map_err(|e| {
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e.to_string(),
                )) as DynError
            })
    }

    async fn read(&mut self) -> Result<String, DynError> {
        Communicator::read_message_buffer(self).await.map_err(|e| {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            )) as DynError
        })
    }
}

impl Communicator {
    pub fn new(writer: ChildStdin, reader: BufReader<ChildStdout>) -> Self {
        Communicator { writer, reader }
    }

    pub async fn send_message(
        &mut self,
        message: &SendMessage,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let message = match message {
            SendMessage::Request(request) => serde_json::to_string(request)?,
            SendMessage::Notification(notification) => serde_json::to_string(notification)?,
        };

        self.send_message2(&message).await?;
        Ok(())
    }

    pub async fn send_message2(&mut self, message: &str) -> Result<(), Box<dyn std::error::Error>> {
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

        if let Some(notification) =
            parse_notification(&json).map_err(|e| e as Box<dyn std::error::Error>)?
        {
            return Ok(Message::Notification(notification));
        }

        if let Some(response) =
            parse_response(&json).map_err(|e| e as Box<dyn std::error::Error>)?
        {
            return Ok(response);
        }

        Err("Other Message".into())
    }

    pub async fn receive_response(
        &mut self,
        id: i32,
    ) -> Result<Message, Box<dyn std::error::Error>> {
        loop {
            let buffer = self.read_message_buffer().await?;
            let json: serde_json::Value = serde_json::from_str(&buffer)?;

            if let Some(notification) =
                parse_notification(&json).map_err(|e| e as Box<dyn std::error::Error>)?
            {
                return Ok(Message::Notification(notification));
            }
            if let Some(message) =
                parse_response(&json).map_err(|e| e as Box<dyn std::error::Error>)?
            {
                if let Message::Response(ref response) = message {
                    if response.id == id {
                        return Ok(message);
                    }
                }
            }
        }
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
        //println!("Header: {}", header_str);

        // Content-Lengthを取得
        let content_length = self.get_content_length(&header_str)?;
        //println!("Parsed Content-Length: {}", content_length);

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
}

#[async_trait::async_trait]
impl FramedTransport for Communicator {
    async fn send_message(&mut self, message: &SendMessage) -> Result<(), DynError> {
        Communicator::send_message(self, message)
            .await
            .map_err(|e| {
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e.to_string(),
                )) as DynError
            })
    }

    async fn send_message2(&mut self, message: &str) -> Result<(), DynError> {
        Communicator::send_message2(self, message)
            .await
            .map_err(|e| {
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e.to_string(),
                )) as DynError
            })
    }

    async fn receive_message(&mut self) -> Result<Message, DynError> {
        Communicator::receive_message(self).await.map_err(|e| {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            )) as DynError
        })
    }

    async fn receive_response(&mut self, id: i32) -> Result<Message, DynError> {
        Communicator::receive_response(self, id).await.map_err(|e| {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            )) as DynError
        })
    }
}
