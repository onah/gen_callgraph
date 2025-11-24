//! LSP transport abstraction (framed Content-Length messages).
use async_trait::async_trait;
use std::error::Error;

/// Minimal async trait for LSP transport.
/// - `send` takes a JSON body (not including LSP headers) and will frame it (Content-Length) and send.
/// - `read` returns the JSON body string (header stripped).
#[async_trait]
pub trait LspTransport: Send + Sync {
    async fn send(&mut self, json_body: &str) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn read(&mut self) -> Result<String, Box<dyn Error + Send + Sync>>;
}

#[cfg(test)]
mod tests {
    use super::LspTransport;
    use async_trait::async_trait;
    use std::error::Error;
    use tokio::io::{duplex, AsyncReadExt, AsyncWriteExt, DuplexStream};

    struct InMemoryTransport {
        stream: DuplexStream,
    }

    impl InMemoryTransport {
        fn new(stream: DuplexStream) -> Self {
            Self { stream }
        }
    }

    #[async_trait]
    impl LspTransport for InMemoryTransport {
        async fn send(&mut self, json_body: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
            let framed = format!("Content-Length: {}\r\n\r\n{}", json_body.len(), json_body);
            self.stream.write_all(framed.as_bytes()).await?;
            self.stream.flush().await?;
            Ok(())
        }

        async fn read(&mut self) -> Result<String, Box<dyn Error + Send + Sync>> {
            // read header until \r\n\r\n
            let mut header_buffer: Vec<u8> = Vec::new();
            loop {
                let mut byte = [0u8; 1];
                self.stream.read_exact(&mut byte).await?;
                header_buffer.push(byte[0]);
                if header_buffer.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            let header = String::from_utf8(header_buffer)?;
            // parse content-length
            let mut content_length: Option<usize> = None;
            for line in header.lines() {
                if line.to_lowercase().starts_with("content-length:") {
                    if let Some(v) = line.split(':').nth(1) {
                        content_length = v.trim().parse::<usize>().ok();
                    }
                }
            }
            let len = content_length.ok_or("no content-length")?;
            let mut body = vec![0u8; len];
            self.stream.read_exact(&mut body).await?;
            Ok(String::from_utf8(body)?)
        }
    }

    #[tokio::test]
    async fn test_inmemory_read_body() {
        let (a, mut b) = duplex(1024);

        // create transport using endpoint `a`
        let mut transport = InMemoryTransport::new(a);

        // spawn a task that writes a framed message into peer `b`
        let handle = tokio::spawn(async move {
            let json = r#"{"jsonrpc":"2.0","id":1,"result":{"ok":true}}"#;
            let framed = format!("Content-Length: {}\r\n\r\n{}", json.len(), json);
            b.write_all(framed.as_bytes()).await.unwrap();
            b.flush().await.unwrap();
        });

        // read using transport and assert body contains result
        let body = transport.read().await.expect("read failed");
        assert!(body.contains("\"result\""));

        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_inmemory_send_framing() {
        let (a, mut b) = duplex(1024);
        let mut transport = InMemoryTransport::new(a);

        // spawn a reader task to consume framed message from peer `d`
        let reader = tokio::spawn(async move {
            // read framed message from d
            let mut header_buf: Vec<u8> = Vec::new();
            loop {
                let mut byte = [0u8; 1];
                b.read_exact(&mut byte).await.unwrap();
                header_buf.push(byte[0]);
                if header_buf.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            let header = String::from_utf8(header_buf).unwrap();
            let mut content_length = 0usize;
            for line in header.lines() {
                if line.to_lowercase().starts_with("content-length:") {
                    content_length = line.split(':').nth(1).unwrap().trim().parse().unwrap();
                }
            }
            let mut body = vec![0u8; content_length];
            b.read_exact(&mut body).await.unwrap();
            String::from_utf8(body).unwrap()
        });

        transport
            .send("{\"jsonrpc\":\"2.0\",\"method\":\"test\",\"params\":{}}")
            .await
            .expect("send failed");

        let received = reader.await.expect("reader task failed");
        assert!(received.contains("\"method\":\"test\""));
    }
}
