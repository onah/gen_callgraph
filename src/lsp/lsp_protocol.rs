//! # Protocol Layer
//!
//! Handles JSON-RPC 2.0 protocol processing and asynchronous message management.
//!
//! ## Responsibilities
//! - Abstracting protocol operations via the [`FramedTransport`] trait
//! - Asynchronously mapping sent requests to their responses (request ID tracking)
//! - Auto-responding to server-initiated requests
//! - Queuing server-to-client notifications
//!
//! ## Key Types
//! - [`FramedTransport`]: Abstract trait for the Protocol Layer; used to swap in mocks during testing
//! - [`FramedBox`]: Concrete implementation wrapping `LspTransport`; multiplexes I/O via a background task
//!
//! ## Internal Structure
//! [`FramedBox`] owns a background task ([`IoTask`]) that concurrently processes sends and receives
//! using `tokio::select!`. A dedicated `oneshot` channel is created per request to await its
//! response, allowing multiple concurrent in-flight requests.

use crate::error::LspError;
use crate::lsp::message_parser::{parse_message_from_slice, parse_server_request_from_slice};
use crate::lsp::transport::LspTransport;
use crate::lsp::types::{Message, Notification, Request};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, Mutex};

// ---------------------------------------------------------------------------
// FramedTransport trait
// ---------------------------------------------------------------------------

/// Abstract interface for the Protocol Layer.
///
/// Sits above the Transport Layer and provides send/receive operations at the
/// JSON-RPC message level. [`FramedBox`] is the concrete implementation; a mock
/// can be substituted during testing.
///
/// # Default Implementation
/// [`send_and_wait`] is provided as a convenience method that combines
/// [`send_request`] and [`wait_response`].
#[async_trait]
pub trait FramedTransport: Send + Sync {
    /// Sends a request and returns the assigned request ID.
    ///
    /// Hands the request off to the background task and registers it internally
    /// so that the corresponding response can be retrieved via [`wait_response`].
    async fn send_request(&mut self, request: Request) -> Result<i32, LspError>;

    /// Convenience method that sends a request and waits for its response.
    ///
    /// Calls [`send_request`] followed by [`wait_response`].
    async fn send_and_wait(
        &mut self,
        request: Request,
        timeout: Option<Duration>,
    ) -> Result<Message, LspError> {
        let id = self.send_request(request).await?;
        self.wait_response(id, timeout).await
    }

    /// Sends a notification (no response expected).
    async fn send_notification(&mut self, notification: Notification) -> Result<(), LspError>;

    /// Waits for the response corresponding to the given request ID.
    ///
    /// Returns a timeout error if `timeout` is `Some` and the duration elapses.
    async fn wait_response(
        &mut self,
        id: i32,
        timeout: Option<Duration>,
    ) -> Result<Message, LspError>;

    /// Waits for the next server-to-client notification.
    ///
    /// Returns a timeout error if `timeout` is `Some` and the duration elapses.
    async fn wait_notification(
        &mut self,
        timeout: Option<Duration>,
    ) -> Result<Notification, LspError>;
}

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

/// Variants of outgoing messages processed by [`IoTask`].
enum ClientOutgoing {
    Request(Request),
    Notification(Notification),
}

// ---------------------------------------------------------------------------
// FramedBox
// ---------------------------------------------------------------------------

/// Concrete implementation of the Protocol Layer that wraps [`LspTransport`].
///
/// Spawns an internal background task ([`IoTask`]) via `tokio::spawn` and
/// processes sends and receives concurrently using `tokio::select!`.
///
/// A `oneshot` channel is created per request to track request-response pairs,
/// enabling multiple concurrent in-flight requests.
pub struct FramedBox {
    outgoing_tx: mpsc::Sender<ClientOutgoing>,
    /// Sender-side channel map used by the background task when a response arrives
    pending_senders: Arc<Mutex<HashMap<i32, oneshot::Sender<Message>>>>,
    /// Receiver-side channel map used by [`wait_response`] callers
    pending_receivers: Arc<Mutex<HashMap<i32, oneshot::Receiver<Message>>>>,
    /// Queue of server-to-client notifications buffered by the background task
    notification_rx: Mutex<mpsc::Receiver<Notification>>,
}

// ---------------------------------------------------------------------------
// IoTask
// ---------------------------------------------------------------------------

/// Background I/O task owned by [`FramedBox`].
///
/// Receives outgoing messages from `outgoing_rx` and writes them to the transport,
/// while simultaneously reading from the transport and routing incoming messages.
/// The loop exits when all [`FramedBox`] handles are dropped or a fatal read error
/// occurs, at which point all pending requests are failed immediately.
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
        // Fail all pending requests so callers see an error immediately
        // rather than hanging until their timeout fires.
        self.drain_pending_senders().await;
    }

    /// Serializes and writes one outgoing message to the transport.
    ///
    /// On failure, removes the corresponding pending sender so that the
    /// `wait_response` caller sees a channel-close error promptly rather
    /// than waiting until its timeout fires.
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
                self.pending_senders.lock().await.remove(&id);
            }
            return Err(e);
        }
        Ok(())
    }

    /// Parses one incoming payload and routes it to the appropriate waiter.
    async fn dispatch_incoming(&mut self, buf: Vec<u8>) {
        match parse_server_request_from_slice(&buf) {
            Ok(Some((id, method))) => {
                if let Err(e) = self.respond_to_server_request(id, &method, &buf).await {
                    eprintln!("transport write error: {}", e);
                }
                return;
            }
            Ok(None) => {}
            Err(e) => {
                eprintln!("parse server request error: {}", e);
                return;
            }
        }

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
                // Discard if the buffer is full or the receiver has been dropped.
                if self.notification_tx.try_send(note).is_err() {}
            }
        }
    }

    /// Sends an appropriate protocol response to a server-initiated request.
    async fn respond_to_server_request(
        &mut self,
        id: i32,
        method: &str,
        raw_buf: &[u8],
    ) -> anyhow::Result<()> {
        match method {
            "client/registerCapability"
            | "client/unregisterCapability"
            | "window/workDoneProgress/create"
            | "window/showDocument" => {
                self.send_server_request_result(id, serde_json::Value::Null)
                    .await
            }
            "workspace/configuration" => {
                let result = Self::workspace_configuration_result(raw_buf)?;
                self.send_server_request_result(id, result).await
            }
            _ => self.send_method_not_found(id, method).await,
        }
    }

    fn workspace_configuration_result(raw_buf: &[u8]) -> anyhow::Result<serde_json::Value> {
        let json: serde_json::Value = serde_json::from_slice(raw_buf)?;
        let items_len = json
            .get("params")
            .and_then(|params| params.get("items"))
            .and_then(|items| items.as_array())
            .map(|items| items.len())
            .unwrap_or(0);
        let items = vec![serde_json::Value::Null; items_len];
        Ok(serde_json::Value::Array(items))
    }

    async fn send_server_request_result(
        &mut self,
        id: i32,
        result: serde_json::Value,
    ) -> anyhow::Result<()> {
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result
        });
        let bytes = serde_json::to_vec(&response)?;
        self.transport.write(&bytes).await
    }

    async fn send_method_not_found(&mut self, id: i32, method: &str) -> anyhow::Result<()> {
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32601,
                "message": format!("Method not found: {}", method)
            }
        });
        let bytes = serde_json::to_vec(&response)?;
        self.transport.write(&bytes).await
    }

    /// Delivers `msg` to the oneshot channel registered for `id`.
    async fn resolve_pending(&self, id: i32, msg: Message) {
        let mut senders = self.pending_senders.lock().await;
        if let Some(tx) = senders.remove(&id) {
            let _ = tx.send(msg);
        } else {
            eprintln!("no pending sender for id={}", id);
        }
    }

    /// Drops all pending senders so that every waiting `wait_response` caller
    /// wakes up with a channel-close error instead of hanging until its timeout.
    async fn drain_pending_senders(&self) {
        self.pending_senders.lock().await.clear();
    }
}

// ---------------------------------------------------------------------------
// FramedBox impl
// ---------------------------------------------------------------------------

impl FramedBox {
    /// Creates a new [`FramedBox`] wrapping the given transport.
    ///
    /// Spawns the background task via `tokio::spawn`, so this must be
    /// called within an async runtime.
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
    async fn send_request(&mut self, request: Request) -> Result<i32, LspError> {
        let id = request.id;
        let (tx, rx) = oneshot::channel();

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
            self.pending_senders.lock().await.remove(&id);
            self.pending_receivers.lock().await.remove(&id);
            return Err(LspError::RequestFailed {
                method: String::from("send"),
                reason: format!("outgoing channel closed: {}", e),
            });
        }

        Ok(id)
    }

    async fn send_notification(&mut self, notification: Notification) -> Result<(), LspError> {
        self.outgoing_tx
            .send(ClientOutgoing::Notification(notification))
            .await
            .map_err(|e| LspError::RequestFailed {
                method: String::from("send_notification"),
                reason: e.to_string(),
            })?;
        Ok(())
    }

    async fn wait_response(
        &mut self,
        id: i32,
        timeout: Option<Duration>,
    ) -> Result<Message, LspError> {
        let rx_opt = {
            let mut map = self.pending_receivers.lock().await;
            map.remove(&id)
        };

        if let Some(mut rx) = rx_opt {
            match timeout {
                Some(dur) => match tokio::time::timeout(dur, &mut rx).await {
                    Ok(Ok(msg)) => Ok(msg),
                    Ok(Err(_)) => Err(LspError::RequestFailed {
                        method: String::from("recv"),
                        reason: String::from("response channel closed"),
                    }),
                    Err(_elapsed) => Err(LspError::Timeout { timeout: dur }),
                },
                None => match rx.await {
                    Ok(msg) => Ok(msg),
                    Err(_) => Err(LspError::RequestFailed {
                        method: String::from("recv"),
                        reason: String::from("response channel closed"),
                    }),
                },
            }
        } else {
            Err(LspError::RequestFailed {
                method: String::from("recv"),
                reason: String::from("no pending receiver for id"),
            })
        }
    }

    async fn wait_notification(
        &mut self,
        timeout: Option<Duration>,
    ) -> Result<Notification, LspError> {
        let mut rx = self.notification_rx.lock().await;
        match timeout {
            Some(dur) => match tokio::time::timeout(dur, rx.recv()).await {
                Ok(Some(note)) => Ok(note),
                Ok(None) => Err(LspError::RequestFailed {
                    method: String::from("wait_notification"),
                    reason: String::from("notification channel closed"),
                }),
                Err(_elapsed) => Err(LspError::Timeout { timeout: dur }),
            },
            None => rx.recv().await.ok_or_else(|| LspError::RequestFailed {
                method: String::from("wait_notification"),
                reason: String::from("notification channel closed"),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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
            .wait_response(id, Some(Duration::from_secs(1)))
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

    #[tokio::test]
    async fn server_request_gets_method_not_found_response() -> Result<()> {
        let (to_server_tx, mut to_server_rx) = mpsc::channel::<Vec<u8>>(8);
        let (to_client_tx, to_client_rx) = mpsc::channel::<Vec<u8>>(8);

        let transport = MockTransport {
            write_tx: to_server_tx,
            read_rx: Arc::new(Mutex::new(to_client_rx)),
        };

        let _client = FramedBox::new(Box::new(transport));

        let server_request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "unknown/customMethod",
            "params": {}
        });
        to_client_tx
            .send(serde_json::to_vec(&server_request)?)
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        let written = tokio::time::timeout(Duration::from_secs(1), to_server_rx.recv())
            .await
            .map_err(|_| anyhow::anyhow!("timeout waiting for method-not-found response"))?
            .ok_or_else(|| anyhow::anyhow!("client write channel closed"))?;

        let json: serde_json::Value = serde_json::from_slice(&written)?;
        assert_eq!(json.get("id").and_then(|v| v.as_i64()), Some(7));
        assert_eq!(
            json.get("error")
                .and_then(|e| e.get("code"))
                .and_then(|v| v.as_i64()),
            Some(-32601)
        );

        Ok(())
    }

    #[tokio::test]
    async fn register_capability_gets_success_response() -> Result<()> {
        let (to_server_tx, mut to_server_rx) = mpsc::channel::<Vec<u8>>(8);
        let (to_client_tx, to_client_rx) = mpsc::channel::<Vec<u8>>(8);

        let transport = MockTransport {
            write_tx: to_server_tx,
            read_rx: Arc::new(Mutex::new(to_client_rx)),
        };

        let _client = FramedBox::new(Box::new(transport));

        let server_request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 8,
            "method": "client/registerCapability",
            "params": {"registrations": []}
        });
        to_client_tx
            .send(serde_json::to_vec(&server_request)?)
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        let written = tokio::time::timeout(Duration::from_secs(1), to_server_rx.recv())
            .await
            .map_err(|_| anyhow::anyhow!("timeout waiting for success response"))?
            .ok_or_else(|| anyhow::anyhow!("client write channel closed"))?;

        let json: serde_json::Value = serde_json::from_slice(&written)?;
        assert_eq!(json.get("id").and_then(|v| v.as_i64()), Some(8));
        assert!(json.get("result").is_some());
        assert!(json.get("error").is_none());

        Ok(())
    }
}
