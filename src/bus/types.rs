// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use rmpv::Value;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, oneshot, watch};

/// A cloneable handle to a running service task.
///
/// All cross-task communication goes through the channels held here;
/// no shared mutable state is used (no `Arc<Mutex<_>>`).
#[derive(Clone)]
pub struct ServiceHandle {
    /// Forward REQ / ONESHOT actions to the service's event loop.
    pub request_tx: mpsc::Sender<ServiceRequest>,
    /// Read the service's latest state snapshot; immediately available after clone.
    pub state_rx: watch::Receiver<Value>,
    /// Optional broadcast channel for discrete, non-mergeable events
    /// (e.g. individual notifications). `None` for most services.
    #[allow(dead_code)]
    pub events_tx: Option<broadcast::Sender<Value>>,
}

/// A request forwarded from the router to a service.
///
/// All fields are populated by the router and consumed by the receiving service.
/// Phase 1 has no services so these fields are never read, but they are required
/// by the router's dispatch logic for future phases.
#[allow(dead_code)]
pub struct ServiceRequest {
    /// The action name (corresponds to the `action` field of the inbound envelope).
    pub action: String,
    /// The action payload as a generic msgpack value.
    pub payload: Value,
    /// One-shot sender over which the service returns its reply (or error).
    pub reply: oneshot::Sender<Result<Value, ServiceError>>,
}

/// Typed errors a service can return in response to a [`ServiceRequest`].
///
/// Phase 1 has no service implementations so these variants are never constructed
/// directly; they are reserved for use in Phase 2+ service modules.
#[allow(dead_code)]
#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("unknown action: {action}")]
    ActionUnknown { action: String },
    #[error("invalid action payload: {msg}")]
    ActionPayload { msg: String },
    #[error("internal service error: {msg}")]
    Internal { msg: String },
    #[error("service temporarily unavailable")]
    Unavailable,
}

impl ServiceError {
    /// The IPC error code string that corresponds to this error variant.
    pub fn code(&self) -> &'static str {
        match self {
            Self::ActionUnknown { .. } => "E_ACTION_UNKNOWN",
            Self::ActionPayload { .. } => "E_ACTION_PAYLOAD",
            Self::Internal { .. } => "E_SERVICE_INTERNAL",
            Self::Unavailable => "E_SERVICE_UNAVAILABLE",
        }
    }
}
