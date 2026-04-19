// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! `wallpaper` service — wallpaper directory scan + current selection state.
//!
//! The daemon owns wallpaper discovery and current-image selection. Rendering
//! remains entirely in QML via per-screen background layer-shell windows.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};

use crate::bus::{ServiceError, ServiceHandle, ServiceRequest};
use crate::config::Config;
use crate::util::is_empty_object;

const DEFAULT_TRANSITION: &str = "fade";
const DEFAULT_TRANSITION_DURATION_MS: u64 = 320;
const DEFAULT_VIDEO_ENABLED: bool = true;
const DEFAULT_VIDEO_AUDIO: bool = false;
const IMAGE_EXTS: &[&str] = &["avif", "bmp", "jpeg", "jpg", "png", "svg", "webp"];
const VIDEO_EXTS: &[&str] = &["avi", "mkv", "mov", "mp4", "webm"];

/// Spawn the `wallpaper` service and return its [`ServiceHandle`].
pub fn spawn(cfg: &Config) -> ServiceHandle {
    let cfg = WallpaperCfg::from_config(cfg);
    let mut state = WallpaperState::new(&cfg);
    state.rescan();

    let (state_tx, state_rx) = watch::channel(state.snapshot());
    let (request_tx, request_rx) = mpsc::channel(16);

    tokio::spawn(run(request_rx, state_tx, state));

    ServiceHandle {
        request_tx,
        state_rx,
        events_tx: None,
    }
}

#[derive(Clone, Debug)]
struct WallpaperCfg {
    directory: PathBuf,
    transition_type: String,
    transition_duration_ms: u64,
    video_enabled: bool,
    video_audio: bool,
}

impl WallpaperCfg {
    fn from_config(cfg: &Config) -> Self {
        let wallpaper = cfg.services.wallpaper.as_ref();

        let transition_type = match wallpaper.and_then(|entry| entry.transition.as_deref()) {
            Some("fade") | None => DEFAULT_TRANSITION.to_string(),
            Some(other) => {
                warn!(
                    transition = %other,
                    "unsupported wallpaper transition configured; falling back to fade"
                );
                DEFAULT_TRANSITION.to_string()
            }
        };

        Self {
            directory: wallpaper
                .and_then(|entry| entry.directory.clone())
                .map(PathBuf::from)
                .unwrap_or_else(default_wallpaper_directory),
            transition_type,
            transition_duration_ms: wallpaper
                .and_then(|entry| entry.transition_duration_ms)
                .unwrap_or(DEFAULT_TRANSITION_DURATION_MS),
            video_enabled: wallpaper
                .and_then(|entry| entry.video_enabled)
                .unwrap_or(DEFAULT_VIDEO_ENABLED),
            video_audio: wallpaper
                .and_then(|entry| entry.video_audio)
                .unwrap_or(DEFAULT_VIDEO_AUDIO),
        }
    }
}

fn default_wallpaper_directory() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        return home.join(".config").join("quicksov").join("wallpapers");
    }
    PathBuf::from("$HOME/.config/quicksov/wallpapers")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WallpaperKind {
    Image,
    Video,
}

impl WallpaperKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WallpaperEntry {
    path: String,
    name: String,
    kind: WallpaperKind,
}

impl WallpaperEntry {
    fn to_json(&self) -> Value {
        json!({
            "path": self.path,
            "name": self.name,
            "kind": self.kind.as_str(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WallpaperAvailability {
    Ready,
    Empty,
    Unavailable,
}

impl WallpaperAvailability {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Empty => "empty",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WallpaperAvailabilityReason {
    None,
    DirectoryMissing,
    PermissionDenied,
    ScanFailed,
}

impl WallpaperAvailabilityReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::DirectoryMissing => "directory_missing",
            Self::PermissionDenied => "permission_denied",
            Self::ScanFailed => "scan_failed",
        }
    }
}

#[derive(Debug)]
struct WallpaperState {
    directory: PathBuf,
    transition_type: String,
    transition_duration_ms: u64,
    video_enabled: bool,
    video_audio: bool,
    availability: WallpaperAvailability,
    availability_reason: WallpaperAvailabilityReason,
    entries: Vec<WallpaperEntry>,
    current_path: Option<String>,
}

impl WallpaperState {
    fn new(cfg: &WallpaperCfg) -> Self {
        Self {
            directory: cfg.directory.clone(),
            transition_type: cfg.transition_type.clone(),
            transition_duration_ms: cfg.transition_duration_ms,
            video_enabled: cfg.video_enabled,
            video_audio: cfg.video_audio,
            availability: WallpaperAvailability::Unavailable,
            availability_reason: WallpaperAvailabilityReason::DirectoryMissing,
            entries: Vec::new(),
            current_path: None,
        }
    }

    fn rescan(&mut self) {
        match scan_directory(&self.directory) {
            Ok(entries) => self.apply_entries(entries),
            Err(err) => self.apply_scan_error(err),
        }
    }

    fn apply_entries(&mut self, entries: Vec<WallpaperEntry>) {
        let first_entry = entries.first().map(|entry| entry.path.clone());
        let current_still_exists = self
            .current_path
            .as_ref()
            .is_some_and(|current| entries.iter().any(|entry| entry.path == *current));

        self.entries = entries;
        self.availability_reason = WallpaperAvailabilityReason::None;

        if self.entries.is_empty() {
            self.availability = WallpaperAvailability::Empty;
            self.current_path = None;
            return;
        }

        self.availability = WallpaperAvailability::Ready;
        if !current_still_exists {
            self.current_path = first_entry;
        }
    }

    fn apply_scan_error(&mut self, err: ScanError) {
        warn!(
            path = %self.directory.display(),
            reason = %err.message(),
            "failed to scan wallpaper directory"
        );
        self.availability = WallpaperAvailability::Unavailable;
        self.availability_reason = err.reason();
        self.entries.clear();
        self.current_path = None;
    }

    fn snapshot(&self) -> Value {
        let entries = self
            .entries
            .iter()
            .map(WallpaperEntry::to_json)
            .collect::<Vec<_>>();
        let current = self.current_entry().map(WallpaperEntry::to_json);

        json!({
            "directory": self.directory.to_string_lossy(),
            "availability": self.availability.as_str(),
            "availability_reason": self.availability_reason.as_str(),
            "entries": entries,
            "current": current,
            "transition": {
                "type": self.transition_type,
                "duration_ms": self.transition_duration_ms,
            },
            "render": {
                "backend": "mpv",
                "video_enabled": self.video_enabled,
                "video_audio": self.video_audio,
            }
        })
    }

    fn current_entry(&self) -> Option<&WallpaperEntry> {
        let current = self.current_path.as_deref()?;
        self.entries.iter().find(|entry| entry.path == current)
    }

    fn next_entry(&mut self, step: isize) -> Result<(), ServiceError> {
        if self.entries.is_empty() {
            return Err(ServiceError::Unavailable);
        }

        let len = self.entries.len() as isize;
        let current_idx = self
            .current_path
            .as_ref()
            .and_then(|current| self.entries.iter().position(|entry| &entry.path == current))
            .unwrap_or(0) as isize;
        let next_idx = (current_idx + step).rem_euclid(len) as usize;
        self.current_path = Some(self.entries[next_idx].path.clone());
        Ok(())
    }

    fn set_current_path(&mut self, path: &str) -> Result<(), ServiceError> {
        if self.entries.is_empty() {
            return Err(ServiceError::Unavailable);
        }

        let Some(entry) = self.entries.iter().find(|entry| entry.path == path) else {
            return Err(ServiceError::ActionPayload {
                msg: "path is not a known wallpaper entry".to_string(),
            });
        };

        self.current_path = Some(entry.path.clone());
        Ok(())
    }
}

async fn run(
    mut request_rx: mpsc::Receiver<ServiceRequest>,
    state_tx: watch::Sender<Value>,
    mut state: WallpaperState,
) {
    info!(path = %state.directory.display(), "wallpaper service started");

    while let Some(req) = request_rx.recv().await {
        let result = match req.action.as_str() {
            "refresh" => handle_refresh(&req.payload, &mut state),
            "next" => handle_step(&req.payload, &mut state, 1),
            "prev" => handle_step(&req.payload, &mut state, -1),
            "set_path" => handle_set_path(&req.payload, &mut state),
            _ => Err(ServiceError::ActionUnknown {
                action: req.action.clone(),
            }),
        };

        state_tx.send_replace(state.snapshot());
        req.reply.send(result).ok();
    }

    info!("wallpaper service stopped");
}

fn handle_refresh(payload: &Value, state: &mut WallpaperState) -> Result<Value, ServiceError> {
    if !is_empty_object(payload) {
        return Err(ServiceError::ActionPayload {
            msg: "refresh expects an empty object payload".to_string(),
        });
    }

    state.rescan();
    if state.availability == WallpaperAvailability::Ready {
        Ok(Value::Null)
    } else {
        Err(ServiceError::Unavailable)
    }
}

fn handle_step(
    payload: &Value,
    state: &mut WallpaperState,
    step: isize,
) -> Result<Value, ServiceError> {
    if !is_empty_object(payload) {
        return Err(ServiceError::ActionPayload {
            msg: "next/prev expect an empty object payload".to_string(),
        });
    }

    state.next_entry(step)?;
    Ok(Value::Null)
}

fn handle_set_path(payload: &Value, state: &mut WallpaperState) -> Result<Value, ServiceError> {
    let Some(obj) = payload.as_object() else {
        return Err(ServiceError::ActionPayload {
            msg: "set_path expects an object payload".to_string(),
        });
    };
    let Some(path) = obj.get("path").and_then(Value::as_str) else {
        return Err(ServiceError::ActionPayload {
            msg: "set_path requires a string field `path`".to_string(),
        });
    };
    if path.is_empty() {
        return Err(ServiceError::ActionPayload {
            msg: "set_path requires a non-empty `path`".to_string(),
        });
    }

    state.set_current_path(path)?;
    Ok(Value::Null)
}

#[derive(Debug)]
enum ScanError {
    DirectoryMissing,
    PermissionDenied,
    ReadFailed(String),
}

impl ScanError {
    fn reason(&self) -> WallpaperAvailabilityReason {
        match self {
            Self::DirectoryMissing => WallpaperAvailabilityReason::DirectoryMissing,
            Self::PermissionDenied => WallpaperAvailabilityReason::PermissionDenied,
            Self::ReadFailed(_) => WallpaperAvailabilityReason::ScanFailed,
        }
    }

    fn message(&self) -> &str {
        match self {
            Self::DirectoryMissing => "directory does not exist",
            Self::PermissionDenied => "permission denied",
            Self::ReadFailed(message) => message.as_str(),
        }
    }
}

fn scan_directory(directory: &Path) -> Result<Vec<WallpaperEntry>, ScanError> {
    if !directory.exists() {
        return Err(ScanError::DirectoryMissing);
    }

    let read_dir = std::fs::read_dir(directory).map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => ScanError::DirectoryMissing,
        std::io::ErrorKind::PermissionDenied => ScanError::PermissionDenied,
        _ => ScanError::ReadFailed(err.to_string()),
    })?;

    let mut entries = Vec::new();

    for entry in read_dir {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                warn!(error = %err, "skipping unreadable wallpaper directory entry");
                continue;
            }
        };

        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(err) => {
                warn!(path = %entry.path().display(), error = %err, "skipping wallpaper entry with unreadable file type");
                continue;
            }
        };
        if !file_type.is_file() {
            continue;
        }

        let path = entry.path();
        let Some(kind) = classify_path(&path) else {
            continue;
        };

        entries.push(WallpaperEntry {
            path: path.to_string_lossy().into_owned(),
            name: entry.file_name().to_string_lossy().into_owned(),
            kind,
        });
    }

    entries.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
            .then_with(|| left.path.cmp(&right.path))
    });

    Ok(entries)
}

fn classify_path(path: &Path) -> Option<WallpaperKind> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();

    if IMAGE_EXTS.contains(&ext.as_str()) {
        return Some(WallpaperKind::Image);
    }
    if VIDEO_EXTS.contains(&ext.as_str()) {
        return Some(WallpaperKind::Video);
    }
    None
}
