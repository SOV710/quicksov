// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors arising from JSON encode / decode operations.
#[derive(Debug, Error)]
pub enum CodecError {
    #[error("encode error: {0}")]
    Encode(#[from] serde_json::Error),
    #[error("decode error: {0}")]
    Decode(serde_json::Error),
}

/// Encode `value` to a JSON string (no trailing newline).
pub fn encode<T: Serialize>(value: &T) -> Result<String, CodecError> {
    serde_json::to_string(value).map_err(CodecError::Encode)
}

/// Decode a JSON string slice into `T`.
pub fn decode<'de, T: Deserialize<'de>>(s: &'de str) -> Result<T, CodecError> {
    serde_json::from_str(s).map_err(CodecError::Decode)
}
