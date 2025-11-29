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
        let mut header_buffer = Vec::new();

        loop {
            let mut byte = [0u8; 1];
            self.reader.read_exact(&mut byte).await?;
            header_buffer.push(byte[0]);
            if header_buffer.ends_with(b"\r\n\r\n") {
                break;
            }
        }

        let header_str = String::from_utf8(header_buffer)?;
        let content_length = get_content_length_from(&header_str)?;
        let mut payload_buffer = vec![0u8; content_length];
        self.reader.read_exact(&mut payload_buffer).await?;

        Ok(String::from_utf8(payload_buffer)?)
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
}

/// Extract Content-Length from header string. Case-insensitive search.
fn get_content_length_from(header: &str) -> anyhow::Result<usize> {
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

// Test-only small transport wrapper to allow injecting an in-memory stream
// into tests without exposing the helper functions publicly.
#[cfg(test)]
mod test_utils {
    use anyhow::Result;
    use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

    pub async fn write_framed<W>(w: &mut W, json: &str) -> Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let header = format!("Content-Length: {}\r\n\r\n", json.len());
        w.write_all(header.as_bytes()).await?;
        w.write_all(json.as_bytes()).await?;
        w.flush().await?;
        Ok(())
    }

    pub async fn read_framed<R>(r: &mut R) -> Result<String>
    where
        R: AsyncRead + Unpin,
    {
        let mut header_buffer = Vec::new();
        loop {
            let mut byte = [0u8; 1];
            r.read_exact(&mut byte).await?;
            header_buffer.push(byte[0]);
            if header_buffer.ends_with(b"\r\n\r\n") {
                break;
            }
        }
        let header = String::from_utf8(header_buffer)?;
        let mut content_length: Option<usize> = None;
        for line in header.lines() {
            if line.to_lowercase().starts_with("content-length:") {
                if let Some(v) = line.split(':').nth(1) {
                    content_length = v.trim().parse::<usize>().ok();
                }
            }
        }
        let len = content_length.ok_or_else(|| anyhow::anyhow!("no content-length"))?;
        let mut body = vec![0u8; len];
        r.read_exact(&mut body).await?;
        Ok(String::from_utf8(body)?)
    }
}

#[cfg(test)]
mod test_transport {
    use crate::lsp::transport::LspTransport;
    use anyhow::Result;
    use async_trait::async_trait;
    use tokio::io::{AsyncRead, AsyncWrite};

    pub struct TestTransport<S> {
        pub stream: S,
    }

    impl<S> TestTransport<S> {
        pub fn new(stream: S) -> Self
        where
            S: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static,
        {
            Self { stream }
        }
    }

    #[async_trait]
    impl<S> LspTransport for TestTransport<S>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static,
    {
        async fn write(&mut self, json_body: &str) -> Result<()> {
            // Use shared test util to frame and write
            super::test_utils::write_framed(&mut self.stream, json_body).await?;
            Ok(())
        }
        async fn read(&mut self) -> Result<String> {
            super::test_utils::read_framed(&mut self.stream).await
        }
    }

    // TestTransport is only re-exported via the factory; no additional re-export needed.
}

#[cfg(test)]
impl StdioTransport {
    /// Create a test-only transport backed by an in-memory stream.
    /// The stream must implement both `AsyncRead` and `AsyncWrite`.
    pub fn from_reader_writer<S>(
        stream: S,
    ) -> Box<dyn crate::lsp::transport::LspTransport + Send + Sync>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
    {
        Box::new(test_transport::TestTransport::new(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::StdioTransport;
    use tokio::io::{duplex, AsyncReadExt, AsyncWriteExt};

    // Tests use the shared `test_utils` for framing operations.

    #[tokio::test]
    async fn test_read_message_from_duplex() {
        let (a, mut b) = duplex(1024);
        // transport uses endpoint `a` (reader)
        let mut transport = StdioTransport::from_reader_writer(a);

        let writer = tokio::spawn(async move {
            let json = r#"{"jsonrpc":"2.0","id":1,"result":{"ok":true}}"#;
            super::test_utils::write_framed(&mut b, json).await.unwrap();
        });

        let body = transport.read().await.expect("read failed");
        assert!(body.contains("\"result\""));

        writer.await.unwrap(); // Ensure writer completes
    }

    #[tokio::test]
    async fn test_write_sets_correct_content_length() {
        let (a, mut b) = duplex(1024);
        let mut transport = StdioTransport::from_reader_writer(a);

        let json = r#"{"jsonrpc":"2.0","method":"x","params":{"n":42}}"#;
        transport.write(json).await.expect("write failed");

        // Read header bytes from peer until CRLFCRLF
        let mut header_buf = Vec::new();
        loop {
            let mut byte = [0u8; 1];
            b.read_exact(&mut byte).await.unwrap();
            header_buf.push(byte[0]);
            if header_buf.ends_with(b"\r\n\r\n") {
                break;
            }
        }

        let header = String::from_utf8(header_buf).expect("header utf8");
        // parse Content-Length
        let mut content_len: Option<usize> = None;
        for line in header.lines() {
            if line.to_lowercase().starts_with("content-length:") {
                if let Some(v) = line.split(':').nth(1) {
                    content_len = v.trim().parse::<usize>().ok();
                }
            }
        }

        assert_eq!(content_len.unwrap(), json.len());
    }

    #[tokio::test]
    async fn test_write_message_to_and_read() {
        let (a, mut b) = duplex(1024);
        let mut transport = StdioTransport::from_reader_writer(a);

        let reader = tokio::spawn(async move {
            super::test_utils::read_framed(&mut b)
                .await
                .expect("reader failed")
        });

        transport
            .write("{\"jsonrpc\":\"2.0\",\"method\":\"test\",\"params\":{}}")
            .await
            .expect("write failed");

        let received = reader.await.expect("reader task failed");
        assert!(received.contains("\"method\":\"test\""));
    }

    #[tokio::test]
    async fn test_read_message_from_malformed_content_length() {
        let (a, mut b) = duplex(64);
        let mut transport = StdioTransport::from_reader_writer(a);

        let writer = tokio::spawn(async move {
            // send malformed content-length
            b.write_all(b"Content-Length: abc\r\n\r\n").await.unwrap(); // Malformed content-length
            b.flush().await.unwrap();
        });

        let res = transport.read().await;
        assert!(res.is_err());

        writer.await.unwrap();
    }
}
