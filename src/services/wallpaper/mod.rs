// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! `wallpaper` service — wallpaper source/view state plus renderer supervision.
//!
//! The daemon owns:
//! - wallpaper directory discovery
//! - source and per-output view selection
//! - process supervision for the dedicated wallpaper renderer
//!
//! The renderer is isolated in a separate process (`qsov-wallpaperd`) so the
//! main shell no longer carries wallpaper video decode / render load.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Map, Value};
use tokio::process::Command;
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};

use crate::bus::{ServiceError, ServiceHandle, ServiceRequest};
use crate::config::{
    Config, WallpaperConfig, WallpaperCropConfig, WallpaperSourceConfig, WallpaperViewConfig,
};
use crate::util::is_empty_object;

const DEFAULT_TRANSITION: &str = "fade";
const DEFAULT_TRANSITION_DURATION_MS: u64 = 320;
const DEFAULT_RENDERER_BACKEND: &str = "native-wayland-ffmpeg";
const DEFAULT_PRESENT_BACKEND: &str = "auto";
const DEFAULT_DECODE_DEVICE_POLICY: &str = "same-as-render";
const DEFAULT_RENDER_DEVICE_POLICY: &str = "same-as-compositor";
const DEFAULT_ALLOW_CROSS_GPU: bool = false;
const DEFAULT_VSYNC: bool = true;
const DEFAULT_VIDEO_AUDIO: bool = false;
const DEFAULT_SOURCE_LOOP: bool = true;
const DEFAULT_SOURCE_MUTE: bool = true;
const DEFAULT_VIEW_FIT: &str = "cover";
const DEFAULT_RENDERER_PROCESS: &str = "qsov-wallpaperd";
const IMAGE_EXTS: &[&str] = &["avif", "bmp", "jpeg", "jpg", "png", "svg", "webp"];
const VIDEO_EXTS: &[&str] = &["avi", "mkv", "mov", "mp4", "webm"];

/// Spawn the `wallpaper` service and return its [`ServiceHandle`].
pub fn spawn(cfg: &Config) -> ServiceHandle {
    let cfg = WallpaperCfg::from_config(cfg);
    let mut state = WallpaperState::new(&cfg);
    state.rescan();

    let (state_tx, state_rx) = watch::channel(state.snapshot());
    let (request_tx, request_rx) = mpsc::channel(16);
    let (renderer_tx, renderer_rx) = watch::channel(RendererRuntime::starting());

    tokio::spawn(supervise_renderer(cfg.clone(), renderer_tx));
    tokio::spawn(run(request_rx, state_tx, renderer_rx, state));

    ServiceHandle {
        request_tx,
        state_rx,
        events_tx: None,
    }
}

#[derive(Clone, Debug)]
struct WallpaperCfg {
    socket_path: String,
    directory: PathBuf,
    transition_type: String,
    transition_duration_ms: u64,
    renderer_backend: String,
    decode_backend_order: Vec<String>,
    decode_device_policy: String,
    render_device_policy: String,
    allow_cross_gpu: bool,
    present_backend: String,
    present_mode: Option<String>,
    vsync: bool,
    video_audio: bool,
    configured_sources: BTreeMap<String, SourceSpec>,
    configured_views: BTreeMap<String, ViewState>,
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

        let renderer_backend = match wallpaper.and_then(|entry| entry.renderer.as_deref()) {
            Some("native-wayland-ffmpeg") | Some("quickshell-ffmpeg") | None => {
                DEFAULT_RENDERER_BACKEND.to_string()
            }
            Some(other) => {
                warn!(
                    renderer = %other,
                    "unsupported wallpaper renderer configured; falling back to native-wayland-ffmpeg"
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

        Self {
            socket_path: cfg.daemon.socket_path.clone(),
            directory: wallpaper
                .and_then(|entry| entry.directory.clone())
                .map(PathBuf::from)
                .unwrap_or_else(default_wallpaper_directory),
            transition_type,
            transition_duration_ms: wallpaper
                .and_then(|entry| entry.transition_duration_ms)
                .unwrap_or(DEFAULT_TRANSITION_DURATION_MS),
            renderer_backend,
            decode_backend_order: wallpaper
                .and_then(|entry| entry.decode_backend_order.clone())
                .unwrap_or_else(|| {
                    vec![
                        "vaapi".to_string(),
                        "cuda".to_string(),
                        "software".to_string(),
                    ]
                }),
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

    fn from_hint(value: &str) -> Option<Self> {
        match value {
            "image" => Some(Self::Image),
            "video" => Some(Self::Video),
            _ => None,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RendererStatus {
    Starting,
    Running,
    Error,
}

impl RendererStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RendererRuntime {
    status: RendererStatus,
    pid: Option<u32>,
    last_error: Option<String>,
}

impl RendererRuntime {
    fn starting() -> Self {
        Self {
            status: RendererStatus::Starting,
            pid: None,
            last_error: None,
        }
    }

    fn running(pid: u32) -> Self {
        Self {
            status: RendererStatus::Running,
            pid: Some(pid),
            last_error: None,
        }
    }

    fn error(message: String) -> Self {
        Self {
            status: RendererStatus::Error,
            pid: None,
            last_error: Some(message),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CropRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl CropRect {
    fn from_config(config: &WallpaperCropConfig) -> Option<Self> {
        let crop = Self {
            x: config.x,
            y: config.y,
            width: config.width,
            height: config.height,
        };
        crop.is_valid().then_some(crop)
    }

    fn is_valid(self) -> bool {
        self.x >= 0.0
            && self.y >= 0.0
            && self.width > 0.0
            && self.height > 0.0
            && self.x + self.width <= 1.0
            && self.y + self.height <= 1.0
    }

    fn to_json(self) -> Value {
        json!({
            "x": self.x,
            "y": self.y,
            "width": self.width,
            "height": self.height,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SourceSpec {
    path: PathBuf,
    kind_hint: Option<WallpaperKind>,
    loop_enabled: bool,
    mute: bool,
}

impl SourceSpec {
    fn from_config(id: &str, source: &WallpaperSourceConfig) -> Option<Self> {
        let kind_hint = match source.kind.as_deref() {
            Some(value) => match WallpaperKind::from_hint(value) {
                Some(kind) => Some(kind),
                None => {
                    warn!(source = %id, kind = %value, "ignoring wallpaper source with unsupported kind");
                    return None;
                }
            },
            None => None,
        };

        Some(Self {
            path: PathBuf::from(&source.path),
            kind_hint,
            loop_enabled: source.loop_enabled.unwrap_or(DEFAULT_SOURCE_LOOP),
            mute: source.mute.unwrap_or(DEFAULT_SOURCE_MUTE),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ResolvedSource {
    id: String,
    path: String,
    name: String,
    kind: WallpaperKind,
    loop_enabled: bool,
    mute: bool,
}

impl ResolvedSource {
    fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "path": self.path,
            "name": self.name,
            "kind": self.kind.as_str(),
            "loop": self.loop_enabled,
            "mute": self.mute,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ViewFit {
    Cover,
}

impl ViewFit {
    fn from_str(value: &str) -> Option<Self> {
        match value {
            "cover" => Some(Self::Cover),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Cover => DEFAULT_VIEW_FIT,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ViewState {
    output: String,
    source: String,
    fit: ViewFit,
    crop: Option<CropRect>,
}

impl ViewState {
    fn from_config(output: &str, view: &WallpaperViewConfig) -> Option<Self> {
        let fit = match view.fit.as_deref() {
            Some(value) => match ViewFit::from_str(value) {
                Some(fit) => fit,
                None => {
                    warn!(output = %output, fit = %value, "ignoring wallpaper view with unsupported fit");
                    return None;
                }
            },
            None => ViewFit::Cover,
        };

        let crop = match view.crop.as_ref() {
            Some(crop) => match CropRect::from_config(crop) {
                Some(crop) => Some(crop),
                None => {
                    warn!(output = %output, "ignoring wallpaper view crop outside normalized bounds");
                    None
                }
            },
            None => None,
        };

        Some(Self {
            output: output.to_string(),
            source: view.source.clone(),
            fit,
            crop,
        })
    }

    fn to_json(&self) -> Value {
        json!({
            "output": self.output,
            "source": self.source,
            "fit": self.fit.as_str(),
            "crop": self.crop.map(CropRect::to_json),
        })
    }
}

#[derive(Debug)]
struct WallpaperState {
    directory: PathBuf,
    transition_type: String,
    transition_duration_ms: u64,
    renderer_backend: String,
    decode_backend_order: Vec<String>,
    decode_device_policy: String,
    render_device_policy: String,
    allow_cross_gpu: bool,
    present_backend: String,
    present_mode: Option<String>,
    vsync: bool,
    video_audio: bool,
    availability: WallpaperAvailability,
    availability_reason: WallpaperAvailabilityReason,
    entries: Vec<WallpaperEntry>,
    source_specs: BTreeMap<String, SourceSpec>,
    sources: BTreeMap<String, ResolvedSource>,
    views: BTreeMap<String, ViewState>,
    fallback_source: Option<String>,
    renderer: RendererRuntime,
}

impl WallpaperState {
    fn new(cfg: &WallpaperCfg) -> Self {
        Self {
            directory: cfg.directory.clone(),
            transition_type: cfg.transition_type.clone(),
            transition_duration_ms: cfg.transition_duration_ms,
            renderer_backend: cfg.renderer_backend.clone(),
            decode_backend_order: cfg.decode_backend_order.clone(),
            decode_device_policy: cfg.decode_device_policy.clone(),
            render_device_policy: cfg.render_device_policy.clone(),
            allow_cross_gpu: cfg.allow_cross_gpu,
            present_backend: cfg.present_backend.clone(),
            present_mode: cfg.present_mode.clone(),
            vsync: cfg.vsync,
            video_audio: cfg.video_audio,
            availability: WallpaperAvailability::Unavailable,
            availability_reason: WallpaperAvailabilityReason::DirectoryMissing,
            entries: Vec::new(),
            source_specs: cfg.configured_sources.clone(),
            sources: BTreeMap::new(),
            views: cfg.configured_views.clone(),
            fallback_source: None,
            renderer: RendererRuntime::starting(),
        }
    }

    fn set_renderer(&mut self, renderer: RendererRuntime) {
        self.renderer = renderer;
    }

    fn rescan(&mut self) {
        match scan_directory(&self.directory) {
            Ok(entries) => self.apply_entries(entries),
            Err(err) => self.apply_scan_error(err),
        }
    }

    fn apply_entries(&mut self, entries: Vec<WallpaperEntry>) {
        self.entries = entries;
        self.availability_reason = WallpaperAvailabilityReason::None;

        if self.entries.is_empty() {
            self.availability = WallpaperAvailability::Empty;
            self.sources.clear();
            self.fallback_source = None;
            return;
        }

        self.availability = WallpaperAvailability::Ready;
        self.ensure_default_source();
        self.resolve_sources();
        self.reconcile_views();
        self.fallback_source = self.sources.keys().next().cloned();
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
        self.sources.clear();
        self.fallback_source = None;
    }

    fn ensure_default_source(&mut self) {
        if !self.source_specs.is_empty() {
            return;
        }
        let Some(entry) = self.entries.first() else {
            return;
        };

        self.source_specs.insert(
            "default".to_string(),
            SourceSpec {
                path: PathBuf::from(&entry.path),
                kind_hint: Some(entry.kind),
                loop_enabled: DEFAULT_SOURCE_LOOP,
                mute: DEFAULT_SOURCE_MUTE,
            },
        );
    }

    fn resolve_sources(&mut self) {
        self.sources.clear();

        for (id, spec) in &self.source_specs {
            let resolved_path = resolve_source_path(&self.directory, &spec.path);
            let Some(kind) = spec.kind_hint.or_else(|| classify_path(&resolved_path)) else {
                warn!(source = %id, path = %resolved_path.display(), "skipping wallpaper source with unsupported file type");
                continue;
            };

            if !resolved_path.is_file() {
                warn!(source = %id, path = %resolved_path.display(), "skipping wallpaper source because file is missing");
                continue;
            }

            let path_string = resolved_path.to_string_lossy().into_owned();
            let name = resolved_path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| id.clone());

            self.sources.insert(
                id.clone(),
                ResolvedSource {
                    id: id.clone(),
                    path: path_string,
                    name,
                    kind,
                    loop_enabled: spec.loop_enabled,
                    mute: spec.mute,
                },
            );
        }
    }

    fn reconcile_views(&mut self) {
        self.views
            .retain(|_, view| self.sources.contains_key(&view.source));
    }

    fn snapshot(&self) -> Value {
        let entries = self
            .entries
            .iter()
            .map(WallpaperEntry::to_json)
            .collect::<Vec<_>>();

        let mut sources = Map::new();
        for (id, source) in &self.sources {
            sources.insert(id.clone(), source.to_json());
        }

        let mut views = Map::new();
        for (output, view) in &self.views {
            if self.sources.contains_key(&view.source) {
                views.insert(output.clone(), view.to_json());
            }
        }

        json!({
            "directory": self.directory.to_string_lossy(),
            "availability": self.availability.as_str(),
            "availability_reason": self.availability_reason.as_str(),
            "entries": entries,
            "fallback_source": self.fallback_source,
            "sources": Value::Object(sources),
            "views": Value::Object(views),
            "transition": {
                "type": self.transition_type,
                "duration_ms": self.transition_duration_ms,
            },
            "renderer": {
                "process": DEFAULT_RENDERER_PROCESS,
                "backend": self.renderer_backend,
                "status": self.renderer.status.as_str(),
                "pid": self.renderer.pid,
                "last_error": self.renderer.last_error,
                "decode_backend_order": self.decode_backend_order,
                "decode_device_policy": self.decode_device_policy,
                "render_device_policy": self.render_device_policy,
                "allow_cross_gpu": self.allow_cross_gpu,
                "present_backend": self.present_backend,
                "present_mode": self.present_mode,
                "vsync": self.vsync,
                "video_audio": self.video_audio,
            }
        })
    }

    fn set_output_source(&mut self, output: &str, source: &str) -> Result<(), ServiceError> {
        if !self.sources.contains_key(source) {
            return Err(ServiceError::ActionPayload {
                msg: "source is not a known wallpaper source".to_string(),
            });
        }

        self.views.insert(
            output.to_string(),
            ViewState {
                output: output.to_string(),
                source: source.to_string(),
                fit: self
                    .views
                    .get(output)
                    .map_or(ViewFit::Cover, |view| view.fit),
                crop: self.views.get(output).and_then(|view| view.crop),
            },
        );

        Ok(())
    }

    fn set_output_path(&mut self, output: &str, path: &str) -> Result<(), ServiceError> {
        let Some(entry) = self
            .entries
            .iter()
            .find(|entry| entry.path == path)
            .cloned()
        else {
            return Err(ServiceError::ActionPayload {
                msg: "path is not a known wallpaper entry".to_string(),
            });
        };

        let source_id = self.ensure_auto_source(&entry);
        self.set_output_source(output, &source_id)
    }

    fn step_output(&mut self, output: &str, step: isize) -> Result<(), ServiceError> {
        if self.entries.is_empty() {
            return Err(ServiceError::Unavailable);
        }

        let current_path = self
            .current_output_source(output)
            .and_then(|source_id| self.sources.get(source_id))
            .map(|source| source.path.clone());

        let current_idx = current_path
            .as_ref()
            .and_then(|path| self.entries.iter().position(|entry| entry.path == *path))
            .unwrap_or(0);

        let len = self.entries.len() as isize;
        let next_idx = (current_idx as isize + step).rem_euclid(len) as usize;
        let path = self.entries[next_idx].path.clone();
        self.set_output_path(output, &path)
    }

    fn set_output_crop(
        &mut self,
        output: &str,
        crop: Option<CropRect>,
    ) -> Result<(), ServiceError> {
        let source = self
            .current_output_source(output)
            .cloned()
            .or_else(|| self.fallback_source.clone())
            .ok_or(ServiceError::Unavailable)?;

        let fit = self
            .views
            .get(output)
            .map_or(ViewFit::Cover, |view| view.fit);

        self.views.insert(
            output.to_string(),
            ViewState {
                output: output.to_string(),
                source,
                fit,
                crop,
            },
        );

        Ok(())
    }

    fn current_output_source(&self, output: &str) -> Option<&String> {
        self.views
            .get(output)
            .map(|view| &view.source)
            .or(self.fallback_source.as_ref())
    }

    fn ensure_auto_source(&mut self, entry: &WallpaperEntry) -> String {
        if let Some((id, _)) = self
            .sources
            .iter()
            .find(|(_, source)| source.path == entry.path)
        {
            return id.clone();
        }

        let mut candidate = sanitize_auto_source_id(&entry.name);
        let mut suffix: u32 = 1;
        while self.source_specs.contains_key(&candidate) {
            suffix += 1;
            candidate = format!("{}-{}", sanitize_auto_source_id(&entry.name), suffix);
        }

        self.source_specs.insert(
            candidate.clone(),
            SourceSpec {
                path: PathBuf::from(&entry.path),
                kind_hint: Some(entry.kind),
                loop_enabled: entry.kind == WallpaperKind::Video,
                mute: DEFAULT_SOURCE_MUTE,
            },
        );
        self.resolve_sources();
        if self.fallback_source.is_none() {
            self.fallback_source = Some(candidate.clone());
        }
        candidate
    }
}

async fn run(
    mut request_rx: mpsc::Receiver<ServiceRequest>,
    state_tx: watch::Sender<Value>,
    mut renderer_rx: watch::Receiver<RendererRuntime>,
    mut state: WallpaperState,
) {
    info!(path = %state.directory.display(), "wallpaper service started");

    state.set_renderer(renderer_rx.borrow().clone());
    state_tx.send_replace(state.snapshot());

    loop {
        tokio::select! {
            maybe_req = request_rx.recv() => {
                let Some(req) = maybe_req else {
                    break;
                };

                let result = match req.action.as_str() {
                    "refresh" => handle_refresh(&req.payload, &mut state),
                    "set_output_source" => handle_set_output_source(&req.payload, &mut state),
                    "set_output_path" => handle_set_output_path(&req.payload, &mut state),
                    "next_output" => handle_step_output(&req.payload, &mut state, 1),
                    "prev_output" => handle_step_output(&req.payload, &mut state, -1),
                    "set_output_crop" => handle_set_output_crop(&req.payload, &mut state),
                    _ => Err(ServiceError::ActionUnknown {
                        action: req.action.clone(),
                    }),
                };

                state_tx.send_replace(state.snapshot());
                req.reply.send(result).ok();
            }
            changed = renderer_rx.changed() => {
                if changed.is_err() {
                    break;
                }
                state.set_renderer(renderer_rx.borrow_and_update().clone());
                state_tx.send_replace(state.snapshot());
            }
        }
    }

    info!("wallpaper service stopped");
}

async fn supervise_renderer(cfg: WallpaperCfg, state_tx: watch::Sender<RendererRuntime>) {
    let binary = renderer_binary_path();

    loop {
        let mut command = Command::new(&binary);
        command.env("QSOV_SOCKET", &cfg.socket_path);

        match command.spawn() {
            Ok(mut child) => {
                let pid = child.id().unwrap_or_default();
                info!(pid, path = %binary.display(), "spawned wallpaper renderer");
                state_tx.send_replace(RendererRuntime::running(pid));

                match child.wait().await {
                    Ok(status) if status.success() => {
                        warn!("wallpaper renderer exited cleanly; restarting");
                        state_tx.send_replace(RendererRuntime::error(
                            "renderer exited unexpectedly".to_string(),
                        ));
                    }
                    Ok(status) => {
                        warn!(status = %status, "wallpaper renderer exited with failure");
                        state_tx.send_replace(RendererRuntime::error(format!(
                            "renderer exited with status {status}"
                        )));
                    }
                    Err(err) => {
                        warn!(error = %err, "failed to wait for wallpaper renderer");
                        state_tx.send_replace(RendererRuntime::error(format!(
                            "failed to wait for renderer: {err}"
                        )));
                    }
                }
            }
            Err(err) => {
                warn!(
                    error = %err,
                    path = %binary.display(),
                    "failed to spawn wallpaper renderer"
                );
                state_tx.send_replace(RendererRuntime::error(format!(
                    "failed to spawn renderer: {err}"
                )));
            }
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
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

fn handle_set_output_source(
    payload: &Value,
    state: &mut WallpaperState,
) -> Result<Value, ServiceError> {
    let obj = action_object(payload, "set_output_source expects an object payload")?;
    let output = string_field(obj, "output", "set_output_source requires `output`")?;
    let source = string_field(obj, "source", "set_output_source requires `source`")?;
    state.set_output_source(output, source)?;
    Ok(Value::Null)
}

fn handle_set_output_path(
    payload: &Value,
    state: &mut WallpaperState,
) -> Result<Value, ServiceError> {
    let obj = action_object(payload, "set_output_path expects an object payload")?;
    let output = string_field(obj, "output", "set_output_path requires `output`")?;
    let path = string_field(obj, "path", "set_output_path requires `path`")?;
    state.set_output_path(output, path)?;
    Ok(Value::Null)
}

fn handle_step_output(
    payload: &Value,
    state: &mut WallpaperState,
    step: isize,
) -> Result<Value, ServiceError> {
    let obj = action_object(payload, "next_output/prev_output expect an object payload")?;
    let output = string_field(obj, "output", "next_output/prev_output require `output`")?;
    state.step_output(output, step)?;
    Ok(Value::Null)
}

fn handle_set_output_crop(
    payload: &Value,
    state: &mut WallpaperState,
) -> Result<Value, ServiceError> {
    let obj = action_object(payload, "set_output_crop expects an object payload")?;
    let output = string_field(obj, "output", "set_output_crop requires `output`")?;
    let crop = match obj.get("crop") {
        None | Some(Value::Null) => None,
        Some(value) => Some(parse_crop(value)?),
    };
    state.set_output_crop(output, crop)?;
    Ok(Value::Null)
}

fn parse_crop(value: &Value) -> Result<CropRect, ServiceError> {
    let obj = value
        .as_object()
        .ok_or_else(|| ServiceError::ActionPayload {
            msg: "crop must be an object or null".to_string(),
        })?;

    let crop = CropRect {
        x: number_field(obj, "x", "crop.x is required")?,
        y: number_field(obj, "y", "crop.y is required")?,
        width: number_field(obj, "width", "crop.width is required")?,
        height: number_field(obj, "height", "crop.height is required")?,
    };

    if crop.is_valid() {
        Ok(crop)
    } else {
        Err(ServiceError::ActionPayload {
            msg: "crop must be normalized to 0..1 and stay inside bounds".to_string(),
        })
    }
}

fn action_object<'a>(
    payload: &'a Value,
    message: &str,
) -> Result<&'a serde_json::Map<String, Value>, ServiceError> {
    payload
        .as_object()
        .ok_or_else(|| ServiceError::ActionPayload {
            msg: message.to_string(),
        })
}

fn string_field<'a>(
    obj: &'a serde_json::Map<String, Value>,
    key: &str,
    message: &str,
) -> Result<&'a str, ServiceError> {
    let value =
        obj.get(key)
            .and_then(Value::as_str)
            .ok_or_else(|| ServiceError::ActionPayload {
                msg: message.to_string(),
            })?;
    if value.is_empty() {
        return Err(ServiceError::ActionPayload {
            msg: message.to_string(),
        });
    }
    Ok(value)
}

fn number_field(
    obj: &serde_json::Map<String, Value>,
    key: &str,
    message: &str,
) -> Result<f64, ServiceError> {
    obj.get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| ServiceError::ActionPayload {
            msg: message.to_string(),
        })
}

fn resolve_source_path(directory: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        directory.join(path)
    }
}

fn sanitize_auto_source_id(name: &str) -> String {
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

fn renderer_binary_path() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(DEFAULT_RENDERER_PROCESS);
            if candidate.exists() {
                return candidate;
            }
        }
    }
    PathBuf::from(DEFAULT_RENDERER_PROCESS)
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
