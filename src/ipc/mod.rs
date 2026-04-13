// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

pub mod codec;
pub mod protocol;
pub mod router;
pub mod session;
pub mod transport;

use thiserror::Error;

/// Top-level IPC errors surfaced to the daemon's startup code.
#[derive(Debug, Error)]
pub enum IpcError {
    #[error("transport error: {0}")]
    Transport(#[from] transport::TransportError),
}
