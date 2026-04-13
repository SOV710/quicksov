// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use nix::sys::signal::Signal;
use thiserror::Error;

/// Errors arising from Linux platform setup operations.
#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("failed to set parent-death signal: {0}")]
    SetPdeathsig(#[source] nix::Error),
}

/// Arrange for SIGTERM to be delivered to this process when its parent exits.
///
/// Must be called once at daemon startup, before entering the async runtime.
pub fn set_parent_death_signal() -> Result<(), PlatformError> {
    nix::sys::prctl::set_pdeathsig(Signal::SIGTERM).map_err(PlatformError::SetPdeathsig)
}
