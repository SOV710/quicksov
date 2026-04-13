// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::{AbortHandle, JoinSet};
use tracing::{debug, warn};

use crate::bus::{ServiceHandle, ServiceRequest};
use crate::ipc::{codec, protocol::*};

// ---------------------------------------------------------------------------
// ForwarderRegistry — per-session subscription state
// ---------------------------------------------------------------------------

/// Tracks active subscription forwarder tasks for a single session.
///
/// Dropping this struct aborts all remaining forwarder tasks automatically.
pub struct ForwarderRegistry {
    set: JoinSet<()>,
    handles: HashMap<String, AbortHandle>,
}

impl ForwarderRegistry {
    pub fn new() -> Self {
        Self {
            set: JoinSet::new(),
            handles: HashMap::new(),
        }
    }

    /// Insert a new forwarder task and record its abort handle under `topic`.
    pub fn insert(&mut self, topic: String, abort: AbortHandle) {
        self.handles.insert(topic, abort);
    }

    /// Abort the forwarder for `topic` (UNSUB).  No-op if not subscribed.
    pub fn remove(&mut self, topic: &str) {
        if let Some(h) = self.handles.remove(topic) {
            h.abort();
        }
    }

    /// Abort every forwarder in the registry (called on session disconnect).
    pub fn abort_all(&mut self) {
        self.handles.clear();
        self.set.abort_all();
    }

    /// Access the inner [`JoinSet`] to spawn new forwarder tasks into it.
    pub fn set_mut(&mut self) -> &mut JoinSet<()> {
        &mut self.set
    }
}

impl Default for ForwarderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Routes inbound IPC envelopes to the appropriate service handle.
///
/// The router is shared (via [`Arc`]) across all sessions; it holds an
/// immutable view of the services map created at startup.
pub struct Router {
    services: HashMap<String, ServiceHandle>,
}

impl Router {
    /// Construct a new router wrapping the given services map.
    pub fn new(services: HashMap<String, ServiceHandle>) -> Arc<Self> {
        Arc::new(Self { services })
    }

    /// Return the list of enabled topic names (used to populate `HelloAck.capabilities`).
    pub fn capabilities(&self) -> Vec<String> {
        self.services.keys().cloned().collect()
    }

    // -----------------------------------------------------------------------
    // Main dispatch entry-point
    // -----------------------------------------------------------------------

    /// Dispatch a decoded envelope to the correct handler.
    ///
    /// Takes a mutable reference to the per-session [`ForwarderRegistry`] so
    /// that SUB / UNSUB can spawn or abort forwarder tasks.
    pub async fn dispatch(
        &self,
        kind: Kind,
        envelope: Envelope,
        outbound_tx: &mpsc::Sender<Vec<u8>>,
        fwd: &mut ForwarderRegistry,
    ) {
        match kind {
            Kind::Req => self.handle_req(envelope, outbound_tx).await,
            Kind::Oneshot => self.handle_oneshot(envelope).await,
            Kind::Sub => self.handle_sub(envelope, outbound_tx, fwd).await,
            Kind::Unsub => self.handle_unsub(&envelope.topic, fwd),
            // REP, ERR, PUB are server-to-client only; a client sending them is a protocol
            // violation but we silently discard rather than hard-disconnect.
            Kind::Rep | Kind::Err | Kind::Pub => {
                warn!(kind = kind as u8, topic = %envelope.topic, "unexpected client-to-server kind; ignoring");
            }
        }
    }

    // -----------------------------------------------------------------------
    // REQ
    // -----------------------------------------------------------------------

    async fn handle_req(&self, envelope: Envelope, outbound_tx: &mpsc::Sender<Vec<u8>>) {
        let Some(handle) = self.services.get(&envelope.topic) else {
            debug!(topic = %envelope.topic, "REQ to unknown topic");
            send_err_envelope(
                outbound_tx,
                envelope.id,
                &envelope.topic,
                E_TOPIC_UNKNOWN,
                &format!("unknown topic: {}", envelope.topic),
            )
            .await;
            return;
        };

        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        let req = ServiceRequest {
            action: envelope.action.clone(),
            payload: envelope.payload,
            reply: reply_tx,
        };

        if handle.request_tx.send(req).await.is_err() {
            send_err_envelope(
                outbound_tx,
                envelope.id,
                &envelope.topic,
                E_SERVICE_UNAVAILABLE,
                "service channel closed",
            )
            .await;
            return;
        }

        match reply_rx.await {
            Ok(Ok(payload)) => {
                send_rep_envelope(outbound_tx, envelope.id, &envelope.topic, payload).await;
            }
            Ok(Err(e)) => {
                send_err_envelope(
                    outbound_tx,
                    envelope.id,
                    &envelope.topic,
                    e.code(),
                    &e.to_string(),
                )
                .await;
            }
            Err(_) => {
                send_err_envelope(
                    outbound_tx,
                    envelope.id,
                    &envelope.topic,
                    E_SERVICE_INTERNAL,
                    "service dropped reply channel",
                )
                .await;
            }
        }
    }

    // -----------------------------------------------------------------------
    // ONESHOT — fire-and-forget; no reply
    // -----------------------------------------------------------------------

    async fn handle_oneshot(&self, envelope: Envelope) {
        let Some(handle) = self.services.get(&envelope.topic) else {
            warn!(topic = %envelope.topic, "ONESHOT to unknown topic (no response sent)");
            return;
        };

        let (reply_tx, _reply_rx) = tokio::sync::oneshot::channel();
        let req = ServiceRequest {
            action: envelope.action,
            payload: envelope.payload,
            reply: reply_tx,
        };
        handle.request_tx.send(req).await.ok();
    }

    // -----------------------------------------------------------------------
    // SUB — subscribe to state updates
    // -----------------------------------------------------------------------

    async fn handle_sub(
        &self,
        envelope: Envelope,
        outbound_tx: &mpsc::Sender<Vec<u8>>,
        fwd: &mut ForwarderRegistry,
    ) {
        let Some(handle) = self.services.get(&envelope.topic) else {
            debug!(topic = %envelope.topic, "SUB to unknown topic");
            send_err_envelope(
                outbound_tx,
                0,
                &envelope.topic,
                E_TOPIC_UNKNOWN,
                &format!("unknown topic: {}", envelope.topic),
            )
            .await;
            return;
        };

        // Push initial snapshot immediately (per §4.3 "订阅时立即推送快照").
        let snapshot = handle.state_rx.borrow().clone();
        send_pub_envelope(outbound_tx, &envelope.topic, snapshot).await;

        // Spawn a forwarder task that watches for subsequent state changes.
        let mut state_rx = handle.state_rx.clone();
        let topic = envelope.topic.clone();
        let tx = outbound_tx.clone();

        let abort = fwd.set_mut().spawn(async move {
            loop {
                if state_rx.changed().await.is_err() {
                    break; // Sender dropped — service has exited.
                }
                let snap = state_rx.borrow_and_update().clone();
                send_pub_envelope(&tx, &topic, snap).await;
            }
        });

        fwd.insert(envelope.topic, abort);
    }

    // -----------------------------------------------------------------------
    // UNSUB
    // -----------------------------------------------------------------------

    fn handle_unsub(&self, topic: &str, fwd: &mut ForwarderRegistry) {
        debug!(topic = %topic, "UNSUB");
        fwd.remove(topic);
    }
}

// ---------------------------------------------------------------------------
// Frame-encoding helpers (shared by router and session)
// ---------------------------------------------------------------------------

/// Encode and enqueue a `REP` envelope.
pub async fn send_rep_envelope(
    outbound_tx: &mpsc::Sender<Vec<u8>>,
    id: u64,
    topic: &str,
    payload: rmpv::Value,
) {
    let env = Envelope {
        id,
        kind: Kind::Rep as u8,
        topic: topic.to_string(),
        action: String::new(),
        payload,
    };
    enqueue_envelope(outbound_tx, &env).await;
}

/// Encode and enqueue an `ERR` envelope.
pub async fn send_err_envelope(
    outbound_tx: &mpsc::Sender<Vec<u8>>,
    id: u64,
    topic: &str,
    code: &'static str,
    message: &str,
) {
    let err_body = ErrorBody {
        code,
        message: message.to_string(),
        details: None,
    };
    // Serialize the ErrorBody as msgpack bytes and decode back to rmpv::Value
    // so it can be embedded in the envelope's generic payload field.
    let payload = match body_to_value(&err_body) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "failed to encode error body as value");
            return;
        }
    };
    let env = Envelope {
        id,
        kind: Kind::Err as u8,
        topic: topic.to_string(),
        action: String::new(),
        payload,
    };
    enqueue_envelope(outbound_tx, &env).await;
}

/// Encode and enqueue a `PUB` envelope.
pub async fn send_pub_envelope(
    outbound_tx: &mpsc::Sender<Vec<u8>>,
    topic: &str,
    payload: rmpv::Value,
) {
    let env = Envelope {
        id: 0,
        kind: Kind::Pub as u8,
        topic: topic.to_string(),
        action: String::new(),
        payload,
    };
    enqueue_envelope(outbound_tx, &env).await;
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

async fn enqueue_envelope(outbound_tx: &mpsc::Sender<Vec<u8>>, env: &Envelope) {
    match codec::encode(env) {
        Ok(bytes) => {
            outbound_tx.send(bytes).await.ok();
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to encode outbound envelope");
        }
    }
}

/// Serialize `body` to msgpack bytes, then decode as an `rmpv::Value` so the
/// structured error can be embedded in an envelope's generic `payload` field.
fn body_to_value(body: &ErrorBody) -> Result<rmpv::Value, codec::CodecError> {
    let bytes = codec::encode(body)?;
    // rmpv::decode reads from a `Read` impl; use a slice cursor.
    let mut cursor = std::io::Cursor::new(bytes);
    rmpv::decode::value::read_value(&mut cursor).map_err(|e| {
        codec::CodecError::Decode(rmp_serde::decode::Error::InvalidMarkerRead(
            std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()),
        ))
    })
}
