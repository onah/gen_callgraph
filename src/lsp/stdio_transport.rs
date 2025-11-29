// low-level stdio transport: framing (Content-Length) and raw read/write
use crate::lsp::transport::LspTransport;
use anyhow::anyhow;
use std::process::Stdio;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

pub struct StdioTransport {
    writer: ChildStdin,
    reader: BufReader<ChildStdout>,
    _child: Option<Child>, // Keep child process handle if needed
}

#[async_trait::async_trait]
impl LspTransport for StdioTransport {
    async fn write(&mut self, json_body: &str) -> anyhow::Result<()> {
        let length = json_body.len();
        let header = format!("Content-Length: {}\r\n\r\n", length);
        self.writer.write_all(header.as_bytes()).await?;
        self.writer.write_all(json_body.as_bytes()).await?;
        self.writer.flush().await?;
        Ok(())
    }

    async fn read(&mut self) -> anyhow::Result<String> {
        StdioTransport::read_message_buffer(self).await
    }
}

impl StdioTransport {
    //pub fn new(writer: ChildStdin, reader: BufReader<ChildStdout>) -> Self {
    //    StdioTransport { writer, reader }
    //}

    pub fn spawn() -> anyhow::Result<Self> {
        let (child, writer, reader) = start_rust_analyzer("rust-analyzer", &[])?;
        Ok(StdioTransport {
            writer,
            reader,
            _child: Some(child),
        })
    }

    // High-level message send/receive methods belong to the framed layer.
    // StdioTransport keeps low-level framing (read_message_buffer/get_content_length)
    // and implements `LspTransport` (send/read) which operate on raw JSON strings.

    async fn read_message_buffer(&mut self) -> anyhow::Result<String> {
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

    fn get_content_length(&self, header: &str) -> anyhow::Result<usize> {
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
            Err(anyhow!("Content-Length header not found"))
        }
    }
}

// Note: FramedTransport implementations are provided by `framed_wrapper.rs` (FramedBox),
// which wraps a `Box<dyn LspTransport>` and provides message-level APIs.

fn start_rust_analyzer(
    exe: &str,
    args: &[&str],
) -> anyhow::Result<(Child, ChildStdin, BufReader<ChildStdout>)> {
    let mut cmd = Command::new(exe);
    for a in args {
        cmd.arg(a);
    }

    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    let writer = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("failed to take child stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("failed to take child stdout"))?;
    let reader = BufReader::new(stdout);

    Ok((child, writer, reader))
}
