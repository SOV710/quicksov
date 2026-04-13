// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::Path;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

/// Maximum permitted payload length in bytes (16 MiB, as per §1 of the spec).
pub const MAX_MESSAGE_BYTES: u32 = 16 * 1024 * 1024;

/// Errors that can occur at the transport layer.
#[derive(Debug, Error)]
pub enum TransportError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("message length {0} bytes exceeds the 16 MiB limit")]
    MessageTooLarge(u32),
    #[error("another daemon instance is already listening on the socket")]
    AlreadyRunning,
}

/// Bind a UDS listener at `path`.
///
/// If a socket file already exists:
/// - connect succeeds → another daemon is alive → returns [`TransportError::AlreadyRunning`].
/// - connect fails   → stale file → unlinks it, then binds.
///
/// Creates the parent directory if it does not exist.
pub async fn bind(path: &Path) -> Result<UnixListener, TransportError> {
    if path.exists() {
        match UnixStream::connect(path).await {
            Ok(_) => return Err(TransportError::AlreadyRunning),
            Err(_) => {
                // Stale socket; best-effort removal — ignore errors.
                let _ = std::fs::remove_file(path);
            }
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let listener = UnixListener::bind(path)?;
    Ok(listener)
}

/// Read one length-prefixed frame from `reader`.
///
/// Returns `Ok(None)` on clean EOF (peer closed the connection).
/// Returns `Err(TransportError::MessageTooLarge)` if the length header
/// exceeds [`MAX_MESSAGE_BYTES`]; the caller must send `E_PROTO_MALFORMED`
/// and close the connection.
pub async fn read_frame<R>(reader: &mut R) -> Result<Option<Vec<u8>>, TransportError>
where
    R: AsyncReadExt + Unpin,
{
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(TransportError::Io(e)),
    }

    let len = u32::from_le_bytes(len_buf);
    if len > MAX_MESSAGE_BYTES {
        return Err(TransportError::MessageTooLarge(len));
    }

    let mut payload = vec![0u8; len as usize];
    reader.read_exact(&mut payload).await?;
    Ok(Some(payload))
}

/// Write one length-prefixed frame to `writer`.
pub async fn write_frame<W>(writer: &mut W, payload: &[u8]) -> Result<(), TransportError>
where
    W: AsyncWriteExt + Unpin,
{
    let len = payload.len() as u32;
    writer.write_all(&len.to_le_bytes()).await?;
    writer.write_all(payload).await?;
    writer.flush().await?;
    Ok(())
}
