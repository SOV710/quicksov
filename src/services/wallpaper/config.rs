// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::BTreeMap;
use std::path::PathBuf;

use tracing::warn;

use crate::config::{paths, Config, WallpaperConfig};
use crate::wallpaper_contract::{
    default_wallpaper_decode_backend_order, normalize_wallpaper_decode_backend_order,
    WALLPAPER_RENDERER_BINARY,
};

use super::model::{SourceSpec, ViewState};

pub(super) const DEFAULT_TRANSITION: &str = "fade";
pub(super) const DEFAULT_TRANSITION_DURATION_MS: u64 = 320;
pub(super) const DEFAULT_RENDERER_BACKEND: &str = "wayland-ffmpeg";
pub(super) const DEFAULT_PRESENT_BACKEND: &str = "auto";
pub(super) const DEFAULT_DECODE_DEVICE_POLICY: &str = "same-as-render";
pub(super) const DEFAULT_RENDER_DEVICE_POLICY: &str = "same-as-compositor";
pub(super) const DEFAULT_ALLOW_CROSS_GPU: bool = false;
pub(super) const DEFAULT_VSYNC: bool = true;
pub(super) const DEFAULT_VIDEO_AUDIO: bool = false;
pub(super) const DEFAULT_SOURCE_LOOP: bool = true;
pub(super) const DEFAULT_SOURCE_MUTE: bool = true;
pub(super) const DEFAULT_VIEW_FIT: &str = "cover";
pub(super) const DEFAULT_RENDERER_BINARY: &str = WALLPAPER_RENDERER_BINARY;

#[derive(Clone, Debug)]
pub(super) struct WallpaperCfg {
    pub(super) socket_path: String,
    pub(super) directory: PathBuf,
    pub(super) renderer_process: String,
    pub(super) transition_type: String,
    pub(super) transition_duration_ms: u64,
    pub(super) renderer_backend: String,
    pub(super) decode_backend_order: Vec<String>,
    pub(super) decode_device_policy: String,
    pub(super) render_device_policy: String,
    pub(super) allow_cross_gpu: bool,
    pub(super) present_backend: String,
    pub(super) present_mode: Option<String>,
    pub(super) vsync: bool,
    pub(super) video_audio: bool,
    pub(super) configured_sources: BTreeMap<String, SourceSpec>,
    pub(super) configured_views: BTreeMap<String, ViewState>,
}

impl WallpaperCfg {
    pub(super) fn from_config(cfg: &Config) -> Self {
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

        let renderer_backend = match wallpaper.and_then(|entry| entry.renderer.as_deref()) {
            Some("wayland-ffmpeg")
            | Some("native-wayland-ffmpeg")
            | Some("quickshell-ffmpeg")
            | None => DEFAULT_RENDERER_BACKEND.to_string(),
            Some(other) => {
                warn!(
                    renderer = %other,
                    "unsupported wallpaper renderer configured; falling back to wayland-ffmpeg"
                );
                DEFAULT_RENDERER_BACKEND.to_string()
            }
        };

        let present_backend = match wallpaper.and_then(|entry| entry.present_backend.as_deref()) {
            Some("auto" | "shm" | "dmabuf") | None => wallpaper
                .and_then(|entry| entry.present_backend.clone())
                .unwrap_or_else(|| DEFAULT_PRESENT_BACKEND.to_string()),
            Some(other) => {
                warn!(
                    present_backend = %other,
                    "unsupported wallpaper present backend configured; falling back to auto"
                );
                DEFAULT_PRESENT_BACKEND.to_string()
            }
        };

        let decode_device_policy = normalize_gpu_policy(
            wallpaper.and_then(|entry| entry.decode_device_policy.as_deref()),
            DEFAULT_DECODE_DEVICE_POLICY,
            "decode_device_policy",
        );

        let render_device_policy = normalize_gpu_policy(
            wallpaper.and_then(|entry| entry.render_device_policy.as_deref()),
            DEFAULT_RENDER_DEVICE_POLICY,
            "render_device_policy",
        );

        let decode_backend_order = wallpaper
            .and_then(|entry| entry.decode_backend_order.as_ref())
            .map(|order| normalize_configured_decode_backend_order(order))
            .unwrap_or_else(default_wallpaper_decode_backend_order);

        Self {
            socket_path: cfg.daemon.socket_path.clone(),
            directory: wallpaper
                .and_then(|entry| entry.directory.clone())
                .map(PathBuf::from)
                .unwrap_or_else(default_wallpaper_directory),
            renderer_process: resolve_renderer_binary_path().display().to_string(),
            transition_type,
            transition_duration_ms: wallpaper
                .and_then(|entry| entry.transition_duration_ms)
                .unwrap_or(DEFAULT_TRANSITION_DURATION_MS),
            renderer_backend,
            decode_backend_order,
            decode_device_policy,
            render_device_policy,
            allow_cross_gpu: wallpaper
                .and_then(|entry| entry.allow_cross_gpu)
                .unwrap_or(DEFAULT_ALLOW_CROSS_GPU),
            present_backend,
            present_mode: wallpaper.and_then(|entry| entry.present_mode.clone()),
            vsync: wallpaper
                .and_then(|entry| entry.vsync)
                .unwrap_or(DEFAULT_VSYNC),
            video_audio: wallpaper
                .and_then(|entry| entry.video_audio)
                .unwrap_or(DEFAULT_VIDEO_AUDIO),
            configured_sources: configured_sources(wallpaper),
            configured_views: configured_views(wallpaper),
        }
    }
}

pub(super) fn resolve_renderer_binary_path() -> PathBuf {
    if let Ok(path) = std::env::var("QSOV_WALLPAPER_RENDERER") {
        return PathBuf::from(path);
    }

    let mut candidates = Vec::<PathBuf>::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(DEFAULT_RENDERER_BINARY));
            if let Some(target_dir) = dir.parent() {
                if let Some(repo_root) = target_dir.parent() {
                    candidates.push(
                        repo_root
                            .join(".build")
                            .join("cpp")
                            .join("wallpaper")
                            .join("renderer")
                            .join(DEFAULT_RENDERER_BINARY),
                    );
                }
            }
        }
    }

    candidates.push(PathBuf::from(DEFAULT_RENDERER_BINARY));

    for candidate in candidates {
        if candidate.exists() {
            return candidate;
        }
    }

    PathBuf::from(DEFAULT_RENDERER_BINARY)
}

fn normalize_configured_decode_backend_order(order: &[String]) -> Vec<String> {
    let (normalized, unsupported) = normalize_wallpaper_decode_backend_order(order);

    for backend in unsupported {
        warn!(
            decode_backend = %backend,
            "unsupported wallpaper decode backend configured; dropping entry"
        );
    }

    normalized
}

fn normalize_gpu_policy(value: Option<&str>, default: &str, field: &str) -> String {
    let Some(value) = value else {
        return default.to_string();
    };

    match value {
        "auto" | "same-as-compositor" | "same-as-render" | "prefer-discrete"
        | "prefer-integrated" | "nvidia" | "amdgpu" | "intel" => value.to_string(),
        other => {
            warn!(
                policy_field = field,
                value = %other,
                fallback = %default,
                "unsupported wallpaper gpu policy configured; falling back to default"
            );
            default.to_string()
        }
    }
}

fn configured_sources(wallpaper: Option<&WallpaperConfig>) -> BTreeMap<String, SourceSpec> {
    let Some(wallpaper) = wallpaper else {
        return BTreeMap::new();
    };

    wallpaper
        .sources
        .iter()
        .filter_map(|(id, source)| {
            if id.is_empty() {
                warn!("ignoring wallpaper source with empty id");
                return None;
            }
            SourceSpec::from_config(id, source).map(|spec| (id.clone(), spec))
        })
        .collect()
}

fn configured_views(wallpaper: Option<&WallpaperConfig>) -> BTreeMap<String, ViewState> {
    let Some(wallpaper) = wallpaper else {
        return BTreeMap::new();
    };

    wallpaper
        .views
        .iter()
        .filter_map(|(output, view)| {
            if output.is_empty() {
                warn!("ignoring wallpaper view with empty output name");
                return None;
            }
            ViewState::from_config(output, view).map(|state| (output.clone(), state))
        })
        .collect()
}

fn default_wallpaper_directory() -> PathBuf {
    paths::default_wallpaper_directory()
        .unwrap_or_else(|| PathBuf::from("$HOME/.config/quicksov/wallpapers"))
}

#[cfg(test)]
mod tests {
    use super::normalize_gpu_policy;

    #[test]
    fn unsupported_gpu_policy_falls_back() {
        assert_eq!(
            normalize_gpu_policy(Some("bogus"), "same-as-render", "decode_device_policy"),
            "same-as-render"
        );
    }

    #[test]
    fn supported_gpu_policy_is_preserved() {
        assert_eq!(
            normalize_gpu_policy(
                Some("prefer-discrete"),
                "same-as-render",
                "decode_device_policy"
            ),
            "prefer-discrete"
        );
    }
}
