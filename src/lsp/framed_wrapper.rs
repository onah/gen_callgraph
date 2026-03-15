use crate::lsp::framed::FramedTransport;
use crate::lsp::message_parser::parse_message_from_slice;
use crate::lsp::transport::LspTransport;
use crate::lsp::types::{Message, Notification, Request};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, Mutex};

enum ClientOutgoing {
    Request(Request),
    Notification(Notification),
}

/// Async framed transport wrapper that runs a single background task
/// owning the `LspTransport`. It supports concurrent requests by
/// registering pending oneshot channels keyed by request id.
pub struct FramedBox {
    outgoing_tx: mpsc::Sender<ClientOutgoing>,
    // sender map is used by the background task to resolve responses
    pending_senders: Arc<Mutex<HashMap<i32, oneshot::Sender<Message>>>>,
    // receiver map is used by callers to await responses
    pending_receivers: Arc<Mutex<HashMap<i32, oneshot::Receiver<Message>>>>,
    // server-to-client notifications queued by the background task
    notification_rx: Mutex<mpsc::Receiver<Notification>>,
}

/// Background I/O task owned by `FramedBox`.
///
/// Runs a `tokio::select!` loop that concurrently:
/// - takes outgoing messages from the channel and writes them to the transport, and
/// - reads incoming messages from the transport and delivers them to waiting callers.
///
/// When the loop exits (either because all `FramedBox` handles are dropped or because
/// a fatal read error occurs), any callers still waiting for a response are failed
/// immediately by draining `pending_senders`.
struct IoTask {
    outgoing_rx: mpsc::Receiver<ClientOutgoing>,
    pending_senders: Arc<Mutex<HashMap<i32, oneshot::Sender<Message>>>>,
    notification_tx: mpsc::Sender<Notification>,
    transport: Box<dyn LspTransport + Send + Sync>,
}

impl IoTask {
    async fn run(mut self) {
        loop {
            tokio::select! {
                opt = self.outgoing_rx.recv() => {
                    match opt {
                        Some(msg) => {
                            if let Err(e) = self.send_outgoing(msg).await {
                                eprintln!("transport write error: {}", e);
                            }
                        }
                        None => break, // all FramedBox handles dropped; clean shutdown
                    }
                }
                read_result = self.transport.read() => {
                    match read_result {
                        Ok(buf) => self.dispatch_incoming(buf).await,
                        Err(e) => {
                            eprintln!("transport read error: {}", e);
                            break;
                        }
                    }
                }
            }
        }
        // Fail any requests that are still waiting for a response so that callers
        // see a channel-close error instead of hanging until their timeout fires.
        self.drain_pending_senders().await;
    }

    /// Serialize and write one outgoing message to the transport.
    ///
    /// On failure (serialization or write), removes the corresponding pending sender
    /// so the caller's `wait_response` sees a channel-close error
    /// promptly rather than waiting until the timeout expires.
    async fn send_outgoing(&mut self, msg: ClientOutgoing) -> anyhow::Result<()> {
        let maybe_request_id = match &msg {
            ClientOutgoing::Request(r) => Some(r.id),
            ClientOutgoing::Notification(_) => None,
        };
        let write_result = match &msg {
            ClientOutgoing::Request(r) => match serde_json::to_vec(r) {
                Ok(bytes) => self.transport.write(&bytes).await,
                Err(e) => Err(e.into()),
            },
            ClientOutgoing::Notification(n) => match serde_json::to_vec(n) {
                Ok(bytes) => self.transport.write(&bytes).await,
                Err(e) => Err(e.into()),
            },
        };
        if let Err(e) = write_result {
            if let Some(id) = maybe_request_id {
                // Drop the sender so the caller's receiver wakes up with RecvError.
                self.pending_senders.lock().await.remove(&id);
            }
            return Err(e);
        }
        Ok(())
    }

    /// Parse and route one incoming payload to the appropriate pending receiver.
    async fn dispatch_incoming(&self, buf: Vec<u8>) {
        let message = match parse_message_from_slice(&buf) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("parse message error: {}", e);
                return;
            }
        };
        match message {
            Message::Response(resp) => self.resolve_pending(resp.id, Message::Response(resp)).await,
            Message::Error(err) => self.resolve_pending(err.id, Message::Error(err)).await,
            Message::Notification(note) => {
                // If the receiver has been dropped or the buffer is full, discard.
                let _ = self.notification_tx.try_send(note);
            }
        }
    }

    /// Deliver `msg` to the oneshot channel registered for `id`.
    async fn resolve_pending(&self, id: i32, msg: Message) {
        let mut senders = self.pending_senders.lock().await;
        if let Some(tx) = senders.remove(&id) {
            let _ = tx.send(msg);
        } else {
            eprintln!("no pending sender for id={}", id);
        }
    }

    /// Drop all pending senders so that every waiting `wait_response`
    /// wakes up with a channel-close error instead of hanging until its timeout fires.
    async fn drain_pending_senders(&self) {
        self.pending_senders.lock().await.clear();
    }
}

impl FramedBox {
    pub fn new(transport: Box<dyn LspTransport + Send + Sync + 'static>) -> Self {
        let (outgoing_tx, outgoing_rx) = mpsc::channel::<ClientOutgoing>(32);
        let (notification_tx, notification_rx) = mpsc::channel::<Notification>(64);

        let pending_senders: Arc<Mutex<HashMap<i32, oneshot::Sender<Message>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let pending_receivers: Arc<Mutex<HashMap<i32, oneshot::Receiver<Message>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let task = IoTask {
            outgoing_rx,
            pending_senders: pending_senders.clone(),
            notification_tx,
            transport,
        };
        tokio::spawn(task.run());

        FramedBox {
            outgoing_tx,
            pending_senders,
            pending_receivers,
            notification_rx: Mutex::new(notification_rx),
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

        if let Err(e) = self
            .outgoing_tx
            .send(ClientOutgoing::Request(request))
            .await
        {
            // Channel closed; clean up stale entries to avoid memory leaks.
            self.pending_senders.lock().await.remove(&id);
            self.pending_receivers.lock().await.remove(&id);
            return Err(anyhow::anyhow!("outgoing channel closed: {}", e));
        }

        Ok(id)
    }

    async fn send_notification(&mut self, notification: Notification) -> anyhow::Result<()> {
        self.outgoing_tx
            .send(ClientOutgoing::Notification(notification))
            .await?;
        Ok(())
    }

    async fn wait_response(
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

    async fn wait_notification(
        &mut self,
        timeout: Option<Duration>,
    ) -> anyhow::Result<Notification> {
        let mut rx = self.notification_rx.lock().await;
        match timeout {
            Some(dur) => match tokio::time::timeout(dur, rx.recv()).await {
                Ok(Some(note)) => Ok(note),
                Ok(None) => Err(anyhow::anyhow!("notification channel closed")),
                Err(_) => Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "notification receive timeout",
                )
                .into()),
            },
            None => rx
                .recv()
                .await
                .ok_or_else(|| anyhow::anyhow!("notification channel closed")),
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
            .wait_response(id, Some(std::time::Duration::from_secs(1)))
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
