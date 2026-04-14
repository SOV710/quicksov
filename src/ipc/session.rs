// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::io::BufReader;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tracing::Instrument;

use crate::ipc::{
    codec,
    protocol::*,
    router::{ForwarderRegistry, Router},
    transport,
};

/// Timeout for the initial handshake exchange (§3 of the spec).
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(2);

/// Errors that can occur within a single IPC session.
#[derive(Debug, Error)]
enum SessionError {
    #[error("handshake timeout")]
    HandshakeTimeout,
    #[error("protocol version mismatch: client sent {0}")]
    ProtoVersion(String),
    #[error("malformed Hello: {0}")]
    MalformedHello(String),
    #[error("peer closed connection during handshake")]
    PeerClosed,
    #[error("transport error: {0}")]
    Transport(#[from] transport::TransportError),
    #[error("codec error: {0}")]
    Codec(String),
}

/// Entry point for a new session task.
///
/// Runs the full session lifecycle (handshake → dispatch loop) under a tracing
/// span keyed to `session_id`. Errors are logged as warnings and never
/// propagated to the caller — a session dying must not affect other sessions.
pub async fn run_session(
    stream: UnixStream,
    session_id: u64,
    router: Arc<Router>,
    capabilities: Vec<String>,
) {
    let span = tracing::info_span!("session", id = session_id);

    async move {
        tracing::info!("session opened");
        match drive_session(stream, session_id, router, capabilities).await {
            Ok(()) => tracing::info!("session closed cleanly"),
            Err(e) => tracing::warn!(error = %e, "session terminated with error"),
        }
    }
    .instrument(span)
    .await;
}

// ---------------------------------------------------------------------------
// Session driver
// ---------------------------------------------------------------------------

async fn drive_session(
    stream: UnixStream,
    session_id: u64,
    router: Arc<Router>,
    capabilities: Vec<String>,
) -> Result<(), SessionError> {
    let (read_half, write_half) = stream.into_split();

    // Outbound channel carries pre-encoded JSON lines.
    let (outbound_tx, outbound_rx) = mpsc::channel::<String>(64);

    tokio::spawn(writer_task(write_half, outbound_rx));

    let mut reader = BufReader::new(read_half);

    // --- Handshake (2 s timeout) -------------------------------------------
    let handshake_result = tokio::time::timeout(
        HANDSHAKE_TIMEOUT,
        do_handshake(&mut reader, &outbound_tx, session_id, capabilities),
    )
    .await;

    match handshake_result {
        Err(_elapsed) => {
            send_pre_handshake_error(&outbound_tx, E_HANDSHAKE_TIMEOUT, "handshake timeout").await;
            return Err(SessionError::HandshakeTimeout);
        }
        Ok(Err(e)) => return Err(e),
        Ok(Ok(())) => {}
    }

    tracing::info!("handshake complete");

    // --- Dispatch loop -------------------------------------------------------
    let mut fwd = ForwarderRegistry::new();

    loop {
        match transport::read_line(&mut reader).await {
            Ok(None) => {
                tracing::debug!("client closed connection");
                break;
            }
            Ok(Some(line)) => {
                let envelope: Envelope = match codec::decode(line.trim_end_matches('\n')) {
                    Ok(e) => e,
                    Err(e) => {
                        tracing::warn!(error = %e, "envelope decode failed");
                        send_err_post_handshake(
                            &outbound_tx,
                            0,
                            "",
                            E_PROTO_MALFORMED,
                            &format!("malformed envelope: {e}"),
                        )
                        .await;
                        break;
                    }
                };

                tracing::debug!(
                    kind = envelope.kind,
                    topic = %envelope.topic,
                    id = envelope.id,
                    "recv"
                );

                let kind = match Kind::try_from(envelope.kind) {
                    Ok(k) => k,
                    Err(_) => {
                        tracing::warn!(kind = envelope.kind, "unknown kind byte");
                        send_err_post_handshake(
                            &outbound_tx,
                            envelope.id,
                            &envelope.topic,
                            E_PROTO_MALFORMED,
                            &format!("unknown kind byte: {}", envelope.kind),
                        )
                        .await;
                        break;
                    }
                };

                router
                    .dispatch(kind, envelope, &outbound_tx, &mut fwd)
                    .await;
            }
            Err(transport::TransportError::MessageTooLarge) => {
                tracing::warn!("incoming line exceeds 16 MiB limit");
                send_err_post_handshake(
                    &outbound_tx,
                    0,
                    "",
                    E_PROTO_MALFORMED,
                    "message exceeds 16 MiB limit",
                )
                .await;
                break;
            }
            Err(e) => {
                tracing::debug!(error = %e, "transport read error");
                break;
            }
        }
    }

    fwd.abort_all();

    // outbound_tx is dropped here → writer_task exits when it drains its queue.
    Ok(())
}

// ---------------------------------------------------------------------------
// Handshake
// ---------------------------------------------------------------------------

async fn do_handshake(
    reader: &mut BufReader<OwnedReadHalf>,
    outbound_tx: &mpsc::Sender<String>,
    session_id: u64,
    capabilities: Vec<String>,
) -> Result<(), SessionError> {
    let line = transport::read_line(reader)
        .await
        .map_err(SessionError::Transport)?
        .ok_or(SessionError::PeerClosed)?;

    let hello: Hello = match codec::decode(line.trim_end_matches('\n')) {
        Ok(h) => h,
        Err(e) => {
            send_pre_handshake_error(
                outbound_tx,
                E_PROTO_MALFORMED,
                &format!("failed to parse Hello: {e}"),
            )
            .await;
            return Err(SessionError::MalformedHello(e.to_string()));
        }
    };

    if hello.proto_version != PROTO_VERSION {
        send_pre_handshake_error(
            outbound_tx,
            E_PROTO_VERSION,
            &format!("unsupported protocol version: {}", hello.proto_version),
        )
        .await;
        return Err(SessionError::ProtoVersion(hello.proto_version));
    }

    tracing::info!(
        client = %hello.client_name,
        client_version = %hello.client_version,
        "hello received"
    );

    let ack = HelloAck::new(
        env!("CARGO_PKG_VERSION").to_string(),
        capabilities,
        session_id,
    );

    let line = codec::encode(&ack).map_err(|e| SessionError::Codec(e.to_string()))?;
    outbound_tx.send(line).await.ok();

    Ok(())
}

// ---------------------------------------------------------------------------
// Writer task — owns the write half of the UDS stream
// ---------------------------------------------------------------------------

async fn writer_task(mut write_half: OwnedWriteHalf, mut outbound_rx: mpsc::Receiver<String>) {
    while let Some(line) = outbound_rx.recv().await {
        if let Err(e) = transport::write_line(&mut write_half, &line).await {
            tracing::debug!(error = %e, "write error; stopping writer task");
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers for encoding and sending error frames
// ---------------------------------------------------------------------------

/// Send a bare `ErrorBody` JSON line (used *before* handshake completes).
async fn send_pre_handshake_error(
    outbound_tx: &mpsc::Sender<String>,
    code: &'static str,
    message: &str,
) {
    let body = ErrorBody {
        code,
        message: message.to_string(),
        details: None,
    };
    match codec::encode(&body) {
        Ok(line) => {
            outbound_tx.send(line).await.ok();
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to encode pre-handshake error body");
        }
    }
}

/// Send an `ERR` envelope (used *after* handshake completes).
async fn send_err_post_handshake(
    outbound_tx: &mpsc::Sender<String>,
    id: u64,
    topic: &str,
    code: &'static str,
    message: &str,
) {
    crate::ipc::router::send_err_envelope(outbound_tx, id, topic, code, message).await;
}
