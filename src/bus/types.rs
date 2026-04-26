// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use serde_json::Value;
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
pub struct ServiceRequest {
    /// The action name (corresponds to the `action` field of the inbound envelope).
    pub action: String,
    /// The action payload as a JSON value.
    pub payload: Value,
    /// One-shot sender over which the service returns its reply (or error).
    pub reply: oneshot::Sender<Result<Value, ServiceError>>,
}

/// Typed errors a service can return in response to a [`ServiceRequest`].
#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("unknown action: {action}")]
    ActionUnknown { action: String },
    #[error("invalid action payload: {msg}")]
    ActionPayload { msg: String },
    #[error("internal service error: {msg}")]
    #[allow(dead_code)]
    Internal { msg: String },
    #[error("service temporarily unavailable")]
    #[allow(dead_code)]
    Unavailable,
    #[error("permission denied: {msg}")]
    #[allow(dead_code)]
    Permission { msg: String },
}

impl ServiceError {
    /// The IPC error code string that corresponds to this error variant.
    pub fn code(&self) -> &'static str {
        match self {
            Self::ActionUnknown { .. } => "E_ACTION_UNKNOWN",
            Self::ActionPayload { .. } => "E_ACTION_PAYLOAD",
            Self::Internal { .. } => "E_SERVICE_INTERNAL",
            Self::Unavailable => "E_SERVICE_UNAVAILABLE",
            Self::Permission { .. } => "E_PERMISSION",
        }
    }
}
