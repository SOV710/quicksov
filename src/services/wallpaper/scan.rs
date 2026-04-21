// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::{Path, PathBuf};

use tracing::warn;

use super::model::{WallpaperEntry, WallpaperKind};

const IMAGE_EXTS: &[&str] = &["avif", "bmp", "jpeg", "jpg", "png", "svg", "webp"];
const VIDEO_EXTS: &[&str] = &["avi", "mkv", "mov", "mp4", "webm"];

#[derive(Debug)]
pub(super) enum ScanError {
    DirectoryMissing,
    PermissionDenied,
    ReadFailed(String),
}

impl ScanError {
    pub(super) fn message(&self) -> &str {
        match self {
            Self::DirectoryMissing => "directory does not exist",
            Self::PermissionDenied => "permission denied",
            Self::ReadFailed(message) => message.as_str(),
        }
    }
}

pub(super) fn scan_directory(directory: &Path) -> Result<Vec<WallpaperEntry>, ScanError> {
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

pub(super) fn classify_path(path: &Path) -> Option<WallpaperKind> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();

    if IMAGE_EXTS.contains(&ext.as_str()) {
        return Some(WallpaperKind::Image);
    }
    if VIDEO_EXTS.contains(&ext.as_str()) {
        return Some(WallpaperKind::Video);
    }
    None
}

pub(super) fn resolve_source_path(directory: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        directory.join(path)
    }
}

pub(super) fn sanitize_auto_source_id(name: &str) -> String {
    let mut out = String::from("auto");
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if ch == '-' || ch == '_' {
            out.push(ch);
        } else if out.as_bytes().last().copied() != Some(b'-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}
