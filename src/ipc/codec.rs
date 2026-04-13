// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors arising from MessagePack encode / decode operations.
#[derive(Debug, Error)]
pub enum CodecError {
    #[error("encode error: {0}")]
    Encode(#[from] rmp_serde::encode::Error),
    #[error("decode error: {0}")]
    Decode(#[from] rmp_serde::decode::Error),
}

/// Encode `value` to a MessagePack byte vector using named (map-key) encoding.
///
/// All wire messages use named encoding so that fields are identified by string
/// keys, making the format robust to struct reordering.
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, CodecError> {
    rmp_serde::to_vec_named(value).map_err(CodecError::Encode)
}

/// Decode a MessagePack byte slice into `T`.
pub fn decode<'de, T: Deserialize<'de>>(bytes: &'de [u8]) -> Result<T, CodecError> {
    rmp_serde::from_slice(bytes).map_err(CodecError::Decode)
}
