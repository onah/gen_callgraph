use crate::lsp::framed::FramedTransport;
use crate::lsp::message_parser::parse_message_from_slice;
use crate::lsp::transport::LspTransport;
use crate::lsp::types::{Message, Notification, Request};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, Mutex};

enum Outgoing {
    Request(Request),
    Notification(Notification),
}

/// Async framed transport wrapper that runs a single background task
/// owning the `LspTransport`. It supports concurrent requests by
/// registering pending oneshot channels keyed by request id.
pub struct FramedBox {
    outgoing_tx: mpsc::Sender<Outgoing>,
    // sender map is used by the background task to resolve responses
    pending_senders: Arc<Mutex<HashMap<i32, oneshot::Sender<Message>>>>,
    // receiver map is used by callers to await responses
    pending_receivers: Arc<Mutex<HashMap<i32, oneshot::Receiver<Message>>>>,
}

impl FramedBox {
    pub fn new(transport: Box<dyn LspTransport + Send + Sync + 'static>) -> Self {
        let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<Outgoing>(32);
        let (notification_tx, _notification_rx) = mpsc::channel::<Notification>(32);

        let pending_senders: Arc<Mutex<HashMap<i32, oneshot::Sender<Message>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let pending_receivers: Arc<Mutex<HashMap<i32, oneshot::Receiver<Message>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Clone for background task
        let pending_senders_bg = pending_senders.clone();

        tokio::spawn(async move {
            let mut transport = transport;

            loop {
                tokio::select! {
                    // Outgoing message to send
                    opt = outgoing_rx.recv() => {
                        match opt {
                            Some(msg) => {
                                let bytes = match &msg {
                                    Outgoing::Request(r) => match serde_json::to_vec(r) {
                                        Ok(b) => b,
                                        Err(e) => { eprintln!("serialize request: {}", e); continue; }
                                    },
                                    Outgoing::Notification(n) => match serde_json::to_vec(n) {
                                        Ok(b) => b,
                                        Err(e) => { eprintln!("serialize notification: {}", e); continue; }
                                    }
                                };
                                if let Err(e) = transport.write(&bytes).await {
                                    eprintln!("transport write error: {}", e);
                                }
                            }
                            None => {
                                // sender dropped; exit loop
                                break;
                            }
                        }
                    }
                    // Incoming message from transport
                    read_res = transport.read() => {
                        match read_res {
                            Ok(buf) => {
                                match parse_message_from_slice(&buf) {
                                    Ok(message) => {
                                        match message {
                                            Message::Response(resp) => {
                                                let id = resp.id;
                                                let mut senders = pending_senders_bg.lock().await;
                                                if let Some(tx) = senders.remove(&id) {
                                                    let _ = tx.send(Message::Response(resp));
                                                } else {
                                                    eprintln!("no pending sender for id={}", id);
                                                }
                                            }
                                            Message::Error(err) => {
                                                let id = err.id;
                                                let mut senders = pending_senders_bg.lock().await;
                                                if let Some(tx) = senders.remove(&id) {
                                                    let _ = tx.send(Message::Error(err));
                                                } else {
                                                    eprintln!("no pending sender for id={}", id);
                                                }
                                            }
                                            Message::Notification(note) => {
                                                if let Err(e) = notification_tx.send(note).await {
                                                    eprintln!("notification channel closed: {}", e);
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("parse message error: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("transport read error: {}", e);
                                break;
                            }
                        }
                    }
                }
            }
        });

        FramedBox {
            outgoing_tx,
            pending_senders,
            pending_receivers,
        }
    }
}

#[async_trait]
impl FramedTransport for FramedBox {
    async fn send_request(&mut self, request: Request) -> anyhow::Result<i32> {
        let id = request.id;
        let (tx, rx) = oneshot::channel();

        // register sender for background task and keep receiver locally for caller
        {
            let mut senders = self.pending_senders.lock().await;
            senders.insert(id, tx);
        }
        {
            let mut receivers = self.pending_receivers.lock().await;
            receivers.insert(id, rx);
        }

        self.outgoing_tx.send(Outgoing::Request(request)).await?;

        Ok(id)
    }

    async fn send_notification(&mut self, notification: Notification) -> anyhow::Result<()> {
        self.outgoing_tx
            .send(Outgoing::Notification(notification))
            .await?;
        Ok(())
    }

    async fn receive_response_with_timeout(
        &mut self,
        id: i32,
        timeout: Option<Duration>,
    ) -> anyhow::Result<Message> {
        // take receiver out of map
        let rx_opt = {
            let mut map = self.pending_receivers.lock().await;
            map.remove(&id)
        };

        if let Some(mut rx) = rx_opt {
            match timeout {
                Some(dur) => match tokio::time::timeout(dur, &mut rx).await {
                    Ok(Ok(msg)) => Ok(msg),
                    Ok(Err(_)) => Err(anyhow::anyhow!("response channel closed")),
                    Err(_) => Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "response timeout",
                    )
                    .into()),
                },
                None => match rx.await {
                    Ok(msg) => Ok(msg),
                    Err(_) => Err(anyhow::anyhow!("response channel closed")),
                },
            }
        } else {
            Err(anyhow::anyhow!("no pending receiver for id"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::message_creator::MessageBuilder;
    use crate::lsp::transport::LspTransport;
    use crate::lsp::types::Message as LspMessage;
    use anyhow::Result;
    use std::sync::Arc;
    use tokio::sync::{mpsc, Mutex};

    struct MockTransport {
        write_tx: mpsc::Sender<Vec<u8>>,
        read_rx: Arc<Mutex<mpsc::Receiver<Vec<u8>>>>,
    }

    #[async_trait::async_trait]
    impl LspTransport for MockTransport {
        async fn write(&mut self, json_body: &[u8]) -> Result<(), anyhow::Error> {
            self.write_tx
                .send(json_body.to_vec())
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            Ok(())
        }

        async fn read(&mut self) -> Result<Vec<u8>, anyhow::Error> {
            let mut rx = self.read_rx.lock().await;
            match rx.recv().await {
                Some(v) => Ok(v),
                None => Err(anyhow::anyhow!("mock read closed")),
            }
        }
    }

    #[tokio::test]
    async fn basic_request_response() -> Result<()> {
        let (to_server_tx, mut to_server_rx) = mpsc::channel::<Vec<u8>>(8);
        let (to_client_tx, to_client_rx) = mpsc::channel::<Vec<u8>>(8);

        let transport = MockTransport {
            write_tx: to_server_tx.clone(),
            read_rx: Arc::new(Mutex::new(to_client_rx)),
        };

        let mut client = FramedBox::new(Box::new(transport));

        // spawn mock server that echos a response with same id
        tokio::spawn(async move {
            while let Some(req_bytes) = to_server_rx.recv().await {
                if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&req_bytes) {
                    if let Some(id) = json.get("id") {
                        let resp = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {"ok": true}
                        });
                        let _ = to_client_tx.send(serde_json::to_vec(&resp).unwrap()).await;
                    }
                }
            }
        });

        let mut builder = MessageBuilder::new();
        let req = builder.create_request("test/method", serde_json::json!({"a":1}))?;
        let id = client.send_request(req).await?;

        let msg = client
            .receive_response_with_timeout(id, Some(std::time::Duration::from_secs(1)))
            .await?;
        match msg {
            LspMessage::Response(r) => {
                assert_eq!(r.id, id);
                assert!(r.result.is_some());
            }
            _ => panic!("expected response"),
        }

        Ok(())
    }
}
