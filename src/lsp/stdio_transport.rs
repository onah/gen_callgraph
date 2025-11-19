// low-level stdio transport: framing (Content-Length) and raw read/write
use crate::lsp::protocol::DynError;
use crate::lsp::transport::LspTransport;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};

pub struct StdioTransport {
    writer: ChildStdin,
    reader: BufReader<ChildStdout>,
}

#[async_trait::async_trait]
impl LspTransport for StdioTransport {
    async fn send(&mut self, json_body: &str) -> Result<(), DynError> {
        let length = json_body.len();
        let header = format!("Content-Length: {}\r\n\r\n", length);

        self.writer
            .write_all(header.as_bytes())
            .await
            .map_err(|e| Box::new(e) as DynError)?;
        self.writer
            .write_all(json_body.as_bytes())
            .await
            .map_err(|e| Box::new(e) as DynError)?;
        self.writer
            .flush()
            .await
            .map_err(|e| Box::new(e) as DynError)?;
        Ok(())
    }

    async fn read(&mut self) -> Result<String, DynError> {
        StdioTransport::read_message_buffer(self)
            .await
            .map_err(|e| Box::new(std::io::Error::other(e.to_string())) as DynError)
    }
}

impl StdioTransport {
    pub fn new(writer: ChildStdin, reader: BufReader<ChildStdout>) -> Self {
        StdioTransport { writer, reader }
    }

    // High-level message send/receive methods belong to the framed layer.
    // StdioTransport keeps low-level framing (read_message_buffer/get_content_length)
    // and implements `LspTransport` (send/read) which operate on raw JSON strings.

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

// Note: FramedTransport implementations are provided by `framed_wrapper.rs` (FramedBox),
// which wraps a `Box<dyn LspTransport>` and provides message-level APIs.
