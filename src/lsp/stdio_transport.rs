// low-level stdio transport: framing (Content-Length) and raw read/write
use crate::lsp::transport::LspTransport;
use anyhow::anyhow;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/*
pub struct StdioTransport {
    writer: ChildStdin,
    reader: BufReader<ChildStdout>,
}
*/

pub struct StdioTransport<W, R> {
    writer: W,
    reader: R,
}

#[async_trait::async_trait]
impl<W, R> LspTransport for StdioTransport<W, R>
where
    W: AsyncWriteExt + Unpin + Send + Sync + 'static,
    R: AsyncReadExt + Unpin + Send + Sync + 'static,
{
    async fn write(&mut self, json_body: &[u8]) -> anyhow::Result<()> {
        let length = json_body.len();
        let header = format!("Content-Length: {}\r\n\r\n", length);
        self.writer.write_all(header.as_bytes()).await?;
        self.writer.write_all(json_body).await?;
        self.writer.flush().await?;
        Ok(())
    }

    async fn read(&mut self) -> anyhow::Result<Vec<u8>> {
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

        Ok(payload_buffer)
    }
}

impl<W, R> StdioTransport<W, R>
where
    W: AsyncWriteExt + Unpin + Send + Sync + 'static,
    R: AsyncReadExt + Unpin + Send + Sync + 'static,
{
    pub fn new(writer: W, reader: R) -> StdioTransport<W, R> {
        StdioTransport { writer, reader }
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
