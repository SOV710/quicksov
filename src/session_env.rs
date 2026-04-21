// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::config::paths;

#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    pub(crate) value: String,
    pub(crate) source: &'static str,
}

#[derive(Clone, Debug)]
pub(crate) struct NiriSocket {
    pub(crate) path: String,
    pub(crate) source: &'static str,
}

pub(crate) fn resolve_niri_socket(configured: Option<&str>) -> NiriSocket {
    let candidates = niri_socket_candidates(configured);
    let mut first_fallback: Option<NiriSocket> = None;

    for candidate in &candidates {
        if first_fallback.is_none() {
            first_fallback = Some(NiriSocket {
                path: candidate.value.clone(),
                source: candidate.source,
            });
        }

        if is_connectable_unix_socket(Path::new(&candidate.value)) {
            return NiriSocket {
                path: candidate.value.clone(),
                source: candidate.source,
            };
        }
    }

    first_fallback.unwrap_or_else(default_niri_socket)
}

pub(crate) fn session_bus_candidates() -> Vec<Candidate> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    push_candidate(
        &mut out,
        &mut seen,
        niri_process_env("DBUS_SESSION_BUS_ADDRESS"),
        "niri-process-env",
    );
    push_candidate(
        &mut out,
        &mut seen,
        std::env::var("DBUS_SESSION_BUS_ADDRESS").ok(),
        "process-env",
    );
    push_candidate(
        &mut out,
        &mut seen,
        Some(default_session_bus_address()),
        "runtime-dir",
    );

    out
}

fn niri_socket_candidates(configured: Option<&str>) -> Vec<Candidate> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    push_candidate(
        &mut out,
        &mut seen,
        configured.map(ToOwned::to_owned),
        "config",
    );
    push_candidate(
        &mut out,
        &mut seen,
        niri_process_env("NIRI_SOCKET"),
        "niri-process-env",
    );
    push_candidate(
        &mut out,
        &mut seen,
        std::env::var("NIRI_SOCKET").ok(),
        "process-env",
    );

    for path in scan_runtime_niri_sockets() {
        push_candidate(
            &mut out,
            &mut seen,
            Some(path.display().to_string()),
            "runtime-scan",
        );
    }

    let default = default_niri_socket();
    push_candidate(&mut out, &mut seen, Some(default.path), default.source);

    out
}

fn push_candidate(
    out: &mut Vec<Candidate>,
    seen: &mut HashSet<String>,
    value: Option<String>,
    source: &'static str,
) {
    let Some(value) = value.map(|value| value.trim().to_string()) else {
        return;
    };
    if value.is_empty() || !seen.insert(value.clone()) {
        return;
    }
    out.push(Candidate { value, source });
}

fn default_niri_socket() -> NiriSocket {
    NiriSocket {
        path: paths::default_niri_socket_path().display().to_string(),
        source: "default",
    }
}

fn default_session_bus_address() -> String {
    paths::default_session_bus_address()
}

fn scan_runtime_niri_sockets() -> Vec<PathBuf> {
    let runtime_dir = paths::runtime_dir();

    let mut entries = Vec::<(SystemTime, PathBuf)>::new();
    let Ok(read_dir) = fs::read_dir(runtime_dir) else {
        return Vec::new();
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if !name.starts_with("niri") || !name.ends_with(".sock") {
            continue;
        }

        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.file_type().is_socket() {
            continue;
        }

        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        entries.push((modified, path));
    }

    entries.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    entries.into_iter().map(|(_, path)| path).collect()
}

fn is_connectable_unix_socket(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if !meta.file_type().is_socket() {
        return false;
    }

    UnixStream::connect(path).is_ok()
}

fn niri_process_env(key: &str) -> Option<String> {
    let mut pids = Vec::<u32>::new();
    let read_dir = fs::read_dir("/proc").ok()?;
    for entry in read_dir.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_string_lossy().parse::<u32>().ok() else {
            continue;
        };
        let comm_path = entry.path().join("comm");
        let Ok(comm) = fs::read_to_string(comm_path) else {
            continue;
        };
        if comm.trim() == "niri" {
            pids.push(pid);
        }
    }

    pids.sort_unstable_by(|a, b| b.cmp(a));
    for pid in pids {
        if let Some(value) = read_proc_environ_value(pid, key) {
            return Some(value);
        }
    }

    None
}

fn read_proc_environ_value(pid: u32, key: &str) -> Option<String> {
    let path = format!("/proc/{pid}/environ");
    let bytes = fs::read(path).ok()?;
    parse_environ_value(&bytes, key)
}

fn parse_environ_value(bytes: &[u8], key: &str) -> Option<String> {
    for item in bytes.split(|byte| *byte == 0) {
        if item.is_empty() {
            continue;
        }
        let text = std::str::from_utf8(item).ok()?;
        let (name, value) = text.split_once('=')?;
        if name == key {
            return Some(value.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::parse_environ_value;

    #[test]
    fn parses_null_separated_environ_values() {
        let env = b"FOO=bar\0NIRI_SOCKET=/run/user/1000/niri.sock\0";
        assert_eq!(
            parse_environ_value(env, "NIRI_SOCKET").as_deref(),
            Some("/run/user/1000/niri.sock")
        );
        assert_eq!(parse_environ_value(env, "MISSING"), None);
    }
}
