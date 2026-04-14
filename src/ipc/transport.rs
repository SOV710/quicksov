// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::Path;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

/// Maximum permitted line length in bytes (16 MiB, as per §1 of the spec).
pub const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;

/// Errors that can occur at the transport layer.
#[derive(Debug, Error)]
pub enum TransportError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("message line exceeds the 16 MiB limit")]
    MessageTooLarge,
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

/// Read one NDJSON line from `reader`.
///
/// Returns `Ok(None)` on clean EOF (peer closed the connection).
/// Returns `Err(TransportError::MessageTooLarge)` if the line exceeds
/// [`MAX_LINE_BYTES`]; the caller must send `E_PROTO_MALFORMED` and close.
pub async fn read_line<R>(reader: &mut BufReader<R>) -> Result<Option<String>, TransportError>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut line = String::new();
    loop {
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            if line.is_empty() {
                return Ok(None); // clean EOF
            }
            // EOF without trailing newline — treat as a complete line
            break;
        }
        if line.len() > MAX_LINE_BYTES {
            return Err(TransportError::MessageTooLarge);
        }
        if line.ends_with('\n') {
            break;
        }
    }
    Ok(Some(line))
}

/// Write one NDJSON line to `writer` (appends `\n` if not already present).
pub async fn write_line<W>(writer: &mut W, line: &str) -> Result<(), TransportError>
where
    W: AsyncWriteExt + Unpin,
{
    writer.write_all(line.as_bytes()).await?;
    if !line.ends_with('\n') {
        writer.write_all(b"\n").await?;
    }
    writer.flush().await?;
    Ok(())
}
