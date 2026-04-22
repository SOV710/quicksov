// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};
use tracing::warn;

use crate::bus::ServiceError;
use crate::config::{WallpaperCropConfig, WallpaperSourceConfig, WallpaperViewConfig};

use super::config::{WallpaperCfg, DEFAULT_SOURCE_LOOP, DEFAULT_SOURCE_MUTE, DEFAULT_VIEW_FIT};
use super::scan::{
    classify_path, resolve_source_path, sanitize_auto_source_id, scan_directory, ScanError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WallpaperKind {
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
pub(super) struct WallpaperEntry {
    pub(super) path: String,
    pub(super) name: String,
    pub(super) kind: WallpaperKind,
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
pub(super) enum WallpaperAvailability {
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
pub(super) struct RendererRuntime {
    status: RendererStatus,
    pid: Option<u32>,
    last_error: Option<String>,
}

impl RendererRuntime {
    pub(super) fn starting() -> Self {
        Self {
            status: RendererStatus::Starting,
            pid: None,
            last_error: None,
        }
    }

    pub(super) fn running(pid: u32) -> Self {
        Self {
            status: RendererStatus::Running,
            pid: Some(pid),
            last_error: None,
        }
    }

    pub(super) fn error(message: String) -> Self {
        Self {
            status: RendererStatus::Error,
            pid: None,
            last_error: Some(message),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct CropRect {
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) width: f64,
    pub(super) height: f64,
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

    pub(super) fn is_valid(self) -> bool {
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
pub(super) struct SourceSpec {
    path: PathBuf,
    kind_hint: Option<WallpaperKind>,
    loop_enabled: bool,
    mute: bool,
}

impl SourceSpec {
    pub(super) fn from_config(id: &str, source: &WallpaperSourceConfig) -> Option<Self> {
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
pub(super) struct ViewState {
    output: String,
    source: String,
    fit: ViewFit,
    crop: Option<CropRect>,
}

impl ViewState {
    pub(super) fn from_config(output: &str, view: &WallpaperViewConfig) -> Option<Self> {
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
pub(super) struct WallpaperState {
    directory: PathBuf,
    transition_type: String,
    transition_duration_ms: u64,
    renderer_process: String,
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
    pub(super) fn new(cfg: &WallpaperCfg) -> Self {
        Self {
            directory: cfg.directory.clone(),
            transition_type: cfg.transition_type.clone(),
            transition_duration_ms: cfg.transition_duration_ms,
            renderer_process: cfg.renderer_process.clone(),
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

    pub(super) fn directory(&self) -> &Path {
        &self.directory
    }

    pub(super) fn is_ready(&self) -> bool {
        self.availability == WallpaperAvailability::Ready
    }

    pub(super) fn set_renderer(&mut self, renderer: RendererRuntime) {
        self.renderer = renderer;
    }

    pub(super) fn rescan(&mut self) {
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
        self.availability_reason = match err {
            ScanError::DirectoryMissing => WallpaperAvailabilityReason::DirectoryMissing,
            ScanError::PermissionDenied => WallpaperAvailabilityReason::PermissionDenied,
            ScanError::ReadFailed(_) => WallpaperAvailabilityReason::ScanFailed,
        };
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

    pub(super) fn snapshot(&self) -> Value {
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
                "process": self.renderer_process,
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

    pub(super) fn set_output_source(
        &mut self,
        output: &str,
        source: &str,
    ) -> Result<(), ServiceError> {
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

    pub(super) fn set_output_path(&mut self, output: &str, path: &str) -> Result<(), ServiceError> {
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

    pub(super) fn step_output(&mut self, output: &str, step: isize) -> Result<(), ServiceError> {
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

    pub(super) fn set_output_crop(
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
