// low-level stdio transport: framing (Content-Length) and raw read/write
use crate::lsp::transport::LspTransport;
use anyhow::anyhow;
use std::process::Stdio;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};
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
        // Delegate to the generic helper so it can be tested with in-memory streams.
        read_message_from(&mut self.reader).await
    }
    // instance helper removed in favor of `get_content_length_from` free function
}

/// Read a single LSP message from an async reader (Content-Length framing).
pub(crate) async fn read_message_from<R>(reader: &mut R) -> anyhow::Result<String>
where
    R: AsyncRead + Unpin + Send,
{
    let mut header_buffer = Vec::new();

    loop {
        let mut byte = [0u8; 1];
        reader.read_exact(&mut byte).await?;
        header_buffer.push(byte[0]);
        if header_buffer.ends_with(b"\r\n\r\n") {
            break;
        }
    }

    let header_str = String::from_utf8(header_buffer)?;
    let content_length = get_content_length_from(&header_str)?;
    let mut payload_buffer = vec![0u8; content_length];
    reader.read_exact(&mut payload_buffer).await?;

    Ok(String::from_utf8(payload_buffer)?)
}

/// Extract Content-Length from header string. Case-insensitive search.
pub(crate) fn get_content_length_from(header: &str) -> anyhow::Result<usize> {
    for line in header.lines() {
        if line.to_lowercase().starts_with("content-length:") {
            if let Some(v) = line.split(':').nth(1) {
                let parsed = v.trim().parse::<usize>()?;
                return Ok(parsed);
            }
        }
    }
    Err(anyhow!("Content-Length header not found"))
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

#[cfg(test)]
mod tests {
    use super::read_message_from;
    use tokio::io::{duplex, AsyncWrite, AsyncWriteExt};

    /// Write a single LSP message to an async writer with Content-Length framing.
    pub(crate) async fn write_message_to<W>(writer: &mut W, json_body: &str) -> anyhow::Result<()>
    where
        W: AsyncWrite + Unpin + Send,
    {
        let length = json_body.len();
        let header = format!("Content-Length: {}\r\n\r\n", length);
        writer.write_all(header.as_bytes()).await?;
        writer.write_all(json_body.as_bytes()).await?;
        writer.flush().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_read_message_from_duplex() {
        let (mut a, mut b) = duplex(1024);

        let writer = tokio::spawn(async move {
            let json = r#"{"jsonrpc":"2.0","id":1,"result":{"ok":true}}"#;
            write_message_to(&mut a, json).await.expect("write failed");
        });

        let body = read_message_from(&mut b).await.expect("read failed");
        assert!(body.contains("\"result\""));

        writer.await.unwrap();
    }

    #[tokio::test]
    async fn test_write_message_to_and_read() {
        let (mut a, mut b) = duplex(1024);

        let reader =
            tokio::spawn(async move { read_message_from(&mut b).await.expect("reader failed") });

        write_message_to(
            &mut a,
            "{\"jsonrpc\":\"2.0\",\"method\":\"test\",\"params\":{}}",
        )
        .await
        .expect("write failed");

        let received = reader.await.expect("reader task failed");
        assert!(received.contains("\"method\":\"test\""));
    }

    #[tokio::test]
    async fn test_read_message_from_malformed_content_length() {
        let (mut a, mut b) = duplex(64);

        let writer = tokio::spawn(async move {
            // send malformed content-length
            a.write_all(b"Content-Length: abc\r\n\r\n").await.unwrap();
            a.flush().await.unwrap();
        });

        let res = read_message_from(&mut b).await;
        assert!(res.is_err());

        writer.await.unwrap();
    }
}
