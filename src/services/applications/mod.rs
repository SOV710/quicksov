// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared application metadata resolver used by multiple daemon services.
//!
//! The resolver builds two immutable indexes at startup:
//! - desktop entries (`.desktop`) for display names and declared icons
//! - icon files from XDG icon/pixmap directories for icon-name → file lookup
//!
//! Resolution intentionally supports multiple fallback paths because services
//! only see partial app identity:
//! - explicit icon hints (`app_icon`, `image-path`, PipeWire icon-name props)
//! - desktop-entry / app-id / WM class / binary
//! - process metadata (`/proc/<pid>` environ / exe / comm / cmdline)
//! - generic application icon fallback

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use tracing::debug;

use crate::util::prettify_label;

const GENERIC_APP_ICON_NAME: &str = "application-x-executable";

#[derive(Clone, Debug, Default)]
pub struct AppLookup {
    pub icon_hint: Option<String>,
    pub desktop_entry: Option<String>,
    pub app_id: Option<String>,
    pub wm_class: Option<String>,
    pub app_name: Option<String>,
    pub binary: Option<String>,
    pub process_id: Option<u32>,
}

impl AppLookup {
    pub fn is_empty(&self) -> bool {
        self.icon_hint.is_none()
            && self.desktop_entry.is_none()
            && self.app_id.is_none()
            && self.wm_class.is_none()
            && self.app_name.is_none()
            && self.binary.is_none()
            && self.process_id.is_none()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResolvedApp {
    pub display_name: String,
    pub icon: String,
    pub icon_name: String,
    pub desktop_entry: String,
    pub match_source: String,
}

#[derive(Clone, Debug)]
struct DesktopEntry {
    desktop_id: String,
    name: String,
    icon: Option<String>,
}

#[derive(Debug)]
pub struct AppResolver {
    desktop_exact: HashMap<String, DesktopEntry>,
    desktop_normalized: HashMap<String, DesktopEntry>,
    icon_exact: HashMap<String, PathBuf>,
    icon_normalized: HashMap<String, PathBuf>,
    desktop_entry_count: usize,
    icon_entry_count: usize,
}

impl AppResolver {
    pub fn load() -> Self {
        let resolver = Self::load_from_dirs(desktop_search_dirs(), icon_search_dirs());
        debug!(
            desktop_entries = resolver.desktop_entry_count,
            icon_entries = resolver.icon_entry_count,
            "loaded application metadata indexes"
        );
        resolver
    }

    fn load_from_dirs(desktop_dirs: Vec<PathBuf>, icon_dirs: Vec<PathBuf>) -> Self {
        let mut desktop_exact = HashMap::new();
        let mut desktop_normalized = HashMap::new();
        let mut desktop_seen = HashSet::new();

        for dir in desktop_dirs {
            index_desktop_dir(
                &dir,
                &mut desktop_exact,
                &mut desktop_normalized,
                &mut desktop_seen,
            );
        }

        let mut icon_exact = HashMap::new();
        let mut icon_normalized = HashMap::new();
        let mut icon_scores = HashMap::new();
        for dir in icon_dirs {
            index_icon_dir(
                &dir,
                &mut icon_exact,
                &mut icon_normalized,
                &mut icon_scores,
            );
        }

        Self {
            desktop_entry_count: desktop_seen.len(),
            icon_entry_count: icon_exact.len(),
            desktop_exact,
            desktop_normalized,
            icon_exact,
            icon_normalized,
        }
    }

    pub fn resolve(&self, lookup: &AppLookup) -> ResolvedApp {
        let process = lookup.process_id.and_then(read_process_info);
        let desktop = self.match_desktop(lookup, process.as_ref());
        let display_name = desktop
            .as_ref()
            .map(|(entry, _)| entry.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| fallback_display_name(lookup, process.as_ref()));

        let (icon, icon_name, match_source) =
            self.resolve_icon(lookup, desktop.as_ref(), process.as_ref());

        ResolvedApp {
            display_name,
            icon,
            icon_name,
            desktop_entry: desktop
                .as_ref()
                .map(|(entry, _)| entry.desktop_id.clone())
                .unwrap_or_default(),
            match_source: match_source.to_string(),
        }
    }

    pub fn desktop_entry_count(&self) -> usize {
        self.desktop_entry_count
    }

    pub fn icon_entry_count(&self) -> usize {
        self.icon_entry_count
    }

    fn match_desktop(
        &self,
        lookup: &AppLookup,
        process: Option<&ProcessInfo>,
    ) -> Option<(DesktopEntry, &'static str)> {
        for (source, candidate) in [
            ("desktop_entry", lookup.desktop_entry.as_deref()),
            ("app_id", lookup.app_id.as_deref()),
            ("wm_class", lookup.wm_class.as_deref()),
            (
                "process_env_desktop",
                process.and_then(|proc_info| proc_info.desktop_entry.as_deref()),
            ),
            ("binary", lookup.binary.as_deref()),
            (
                "process_exe",
                process.and_then(|proc_info| proc_info.exe.as_deref()),
            ),
            (
                "process_comm",
                process.and_then(|proc_info| proc_info.comm.as_deref()),
            ),
            (
                "process_cmdline",
                process.and_then(|proc_info| proc_info.cmdline.as_deref()),
            ),
            ("app_name", lookup.app_name.as_deref()),
        ] {
            if let Some(entry) = candidate.and_then(|value| self.lookup_desktop(value)) {
                return Some((entry, source));
            }
        }

        None
    }

    fn lookup_desktop(&self, key: &str) -> Option<DesktopEntry> {
        let trimmed = key.trim();
        if trimmed.is_empty() {
            return None;
        }

        if let Some(entry) = self.desktop_exact.get(trimmed) {
            return Some(entry.clone());
        }

        let base = lookup_base(trimmed);
        if base != trimmed {
            if let Some(entry) = self.desktop_exact.get(base.as_str()) {
                return Some(entry.clone());
            }
        }

        let normalized = normalize_lookup_key(trimmed);
        if let Some(entry) = self.desktop_normalized.get(&normalized) {
            return Some(entry.clone());
        }

        if base != trimmed {
            let normalized_base = normalize_lookup_key(base.as_str());
            if let Some(entry) = self.desktop_normalized.get(&normalized_base) {
                return Some(entry.clone());
            }
        }

        None
    }

    fn resolve_icon(
        &self,
        lookup: &AppLookup,
        desktop: Option<&(DesktopEntry, &'static str)>,
        process: Option<&ProcessInfo>,
    ) -> (String, String, &'static str) {
        if let Some(hint) = lookup.icon_hint.as_deref() {
            if let Some((icon, icon_name, source)) = self.resolve_icon_hint(hint, "icon_hint") {
                return (icon, icon_name, source);
            }
        }

        if let Some((entry, source)) = desktop {
            if let Some(icon_value) = entry.icon.as_deref() {
                if let Some((icon, icon_name, _)) = self.resolve_icon_hint(icon_value, source) {
                    return (icon, icon_name, source);
                }
            }
        }

        for (source, candidate) in [
            ("app_id_icon", lookup.app_id.as_deref()),
            ("binary_icon", lookup.binary.as_deref()),
            (
                "process_exe_icon",
                process.and_then(|proc_info| proc_info.exe.as_deref()),
            ),
            (
                "process_comm_icon",
                process.and_then(|proc_info| proc_info.comm.as_deref()),
            ),
        ] {
            if let Some((icon, icon_name)) =
                candidate.and_then(|value| self.resolve_icon_name(value))
            {
                return (icon, icon_name, source);
            }
        }

        if let Some((icon, icon_name)) = self.resolve_icon_name(GENERIC_APP_ICON_NAME) {
            return (icon, icon_name, "generic_fallback");
        }

        (String::new(), String::new(), "unresolved")
    }

    fn resolve_icon_hint(
        &self,
        hint: &str,
        name_source: &'static str,
    ) -> Option<(String, String, &'static str)> {
        let trimmed = hint.trim();
        if trimmed.is_empty() {
            return None;
        }

        if let Some(path) = local_path_from_uri(trimmed) {
            if path.exists() {
                return Some((
                    path_to_string(&path),
                    icon_name_from_path(&path),
                    "icon_path",
                ));
            }
        }

        let path = Path::new(trimmed);
        if path.is_absolute() && path.exists() {
            return Some((path_to_string(path), icon_name_from_path(path), "icon_path"));
        }

        let icon_name = lookup_base(trimmed);
        self.resolve_icon_name(icon_name.as_str())
            .map(|(icon, resolved_name)| (icon, resolved_name, name_source))
    }

    fn resolve_icon_name(&self, icon_name: &str) -> Option<(String, String)> {
        let base = lookup_base(icon_name);
        let variants = icon_name_variants(base.as_str());

        for variant in &variants {
            if let Some(path) = self.icon_exact.get(variant) {
                return Some((path_to_string(path), variant.clone()));
            }
        }

        for variant in &variants {
            let normalized = normalize_lookup_key(variant);
            if let Some(path) = self.icon_normalized.get(&normalized) {
                return Some((path_to_string(path), variant.clone()));
            }
        }

        None
    }
}

#[derive(Debug)]
struct ProcessInfo {
    desktop_entry: Option<String>,
    exe: Option<String>,
    comm: Option<String>,
    cmdline: Option<String>,
}

fn read_process_info(pid: u32) -> Option<ProcessInfo> {
    let proc_dir = PathBuf::from("/proc").join(pid.to_string());
    if !proc_dir.exists() {
        return None;
    }

    let desktop_entry = fs::read(proc_dir.join("environ"))
        .ok()
        .and_then(|bytes| process_desktop_entry_from_env_bytes(&bytes));
    let exe = fs::read_link(proc_dir.join("exe"))
        .ok()
        .and_then(|path| basename_string(path.as_path()));
    let comm = fs::read_to_string(proc_dir.join("comm"))
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty());
    let cmdline = fs::read(proc_dir.join("cmdline"))
        .ok()
        .and_then(|bytes| first_cmdline_binary(&bytes));

    Some(ProcessInfo {
        desktop_entry,
        exe,
        comm,
        cmdline,
    })
}

fn process_desktop_entry_from_env_bytes(bytes: &[u8]) -> Option<String> {
    for entry in bytes
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        let Ok(text) = std::str::from_utf8(entry) else {
            continue;
        };
        let Some((key, value)) = text.split_once('=') else {
            continue;
        };

        if matches!(key, "GIO_LAUNCHED_DESKTOP_FILE" | "BAMF_DESKTOP_FILE_HINT") {
            if let Some(desktop_id) = desktop_id_from_hint(value) {
                return Some(desktop_id);
            }
        }
    }

    None
}

fn first_cmdline_binary(bytes: &[u8]) -> Option<String> {
    let first = bytes
        .split(|byte| *byte == 0)
        .find(|entry| !entry.is_empty())?;
    let text = std::str::from_utf8(first).ok()?;
    let path = Path::new(text);
    basename_string(path)
}

fn fallback_display_name(lookup: &AppLookup, process: Option<&ProcessInfo>) -> String {
    for candidate in [
        lookup.app_name.as_deref(),
        lookup.app_id.as_deref(),
        lookup.wm_class.as_deref(),
        lookup.binary.as_deref(),
        process.and_then(|proc_info| proc_info.exe.as_deref()),
        process.and_then(|proc_info| proc_info.comm.as_deref()),
        process.and_then(|proc_info| proc_info.cmdline.as_deref()),
    ]
    .into_iter()
    .flatten()
    {
        let pretty = prettify_label(candidate);
        if !pretty.is_empty() {
            return pretty;
        }
    }

    "Unknown app".to_string()
}

fn index_desktop_dir(
    dir: &Path,
    exact: &mut HashMap<String, DesktopEntry>,
    normalized: &mut HashMap<String, DesktopEntry>,
    seen_desktop_ids: &mut HashSet<String>,
) {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }

            if path.extension().and_then(|ext| ext.to_str()) != Some("desktop") {
                continue;
            }

            let Some((desktop, startup_wm_class, exec_bin)) = parse_desktop_entry(&path) else {
                continue;
            };

            seen_desktop_ids.insert(desktop.desktop_id.clone());
            insert_desktop_mapping(&desktop.desktop_id, &desktop, exact, normalized);
            insert_desktop_mapping(&desktop.name, &desktop, exact, normalized);

            if let Some(startup_wm_class) = startup_wm_class.as_deref() {
                insert_desktop_mapping(startup_wm_class, &desktop, exact, normalized);
            }
            if let Some(exec_bin) = exec_bin.as_deref() {
                insert_desktop_mapping(exec_bin, &desktop, exact, normalized);
            }
        }
    }
}

fn parse_desktop_entry(path: &Path) -> Option<(DesktopEntry, Option<String>, Option<String>)> {
    let desktop_id = path.file_stem()?.to_str()?.to_string();
    let text = fs::read_to_string(path).ok()?;

    let mut in_desktop_entry = false;
    let mut name = None;
    let mut icon = None;
    let mut startup_wm_class = None;
    let mut exec_bin = None;
    let mut no_display = false;
    let mut hidden = false;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            in_desktop_entry = line == "[Desktop Entry]";
            continue;
        }

        if !in_desktop_entry {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        match key.trim() {
            "Name" => {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    name = Some(trimmed.to_string());
                }
            }
            "Icon" => {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    icon = Some(trimmed.to_string());
                }
            }
            "StartupWMClass" => {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    startup_wm_class = Some(trimmed.to_string());
                }
            }
            "Exec" if exec_bin.is_none() => exec_bin = parse_exec_binary(value),
            "NoDisplay" => no_display = parse_bool(value),
            "Hidden" => hidden = parse_bool(value),
            _ => {}
        }
    }

    if hidden || no_display {
        return None;
    }

    Some((
        DesktopEntry {
            desktop_id,
            name: name?,
            icon,
        },
        startup_wm_class,
        exec_bin,
    ))
}

fn parse_exec_binary(value: &str) -> Option<String> {
    for token in shell_tokens(value) {
        if token.is_empty() {
            continue;
        }

        if token == "env" {
            continue;
        }

        if !token.starts_with('/') && token.contains('=') {
            continue;
        }

        return basename_string(Path::new(token.as_str())).or(Some(token));
    }

    None
}

fn shell_tokens(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote = None;

    for ch in value.chars() {
        if let Some(active_quote) = quote {
            if ch == active_quote {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }

        match ch {
            '\'' | '"' => quote = Some(ch),
            ' ' | '\t' if !current.is_empty() => {
                tokens.push(std::mem::take(&mut current));
            }
            ' ' | '\t' => {}
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn index_icon_dir(
    dir: &Path,
    exact: &mut HashMap<String, PathBuf>,
    normalized: &mut HashMap<String, PathBuf>,
    scores: &mut HashMap<String, i32>,
) {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }

            if !is_supported_icon_file(&path) {
                continue;
            }

            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };

            insert_icon_mapping(stem, &path, exact, normalized, scores);
        }
    }
}

fn insert_desktop_mapping(
    key: &str,
    entry: &DesktopEntry,
    exact: &mut HashMap<String, DesktopEntry>,
    normalized: &mut HashMap<String, DesktopEntry>,
) {
    for variant in key_variants(key) {
        if variant.is_empty() {
            continue;
        }

        exact
            .entry(variant.clone())
            .or_insert_with(|| entry.clone());
        normalized
            .entry(normalize_lookup_key(variant.as_str()))
            .or_insert_with(|| entry.clone());
    }
}

fn insert_icon_mapping(
    key: &str,
    path: &Path,
    exact: &mut HashMap<String, PathBuf>,
    normalized: &mut HashMap<String, PathBuf>,
    scores: &mut HashMap<String, i32>,
) {
    let score = icon_preference_score(path);
    for variant in key_variants(key) {
        if variant.is_empty() {
            continue;
        }

        upsert_icon_key(variant.as_str(), path, score, exact, scores);
        let normalized_key = normalize_lookup_key(variant.as_str());
        upsert_icon_key(normalized_key.as_str(), path, score, normalized, scores);
    }
}

fn upsert_icon_key(
    key: &str,
    path: &Path,
    score: i32,
    map: &mut HashMap<String, PathBuf>,
    scores: &mut HashMap<String, i32>,
) {
    let replace = match scores.get(key) {
        Some(existing) => score > *existing,
        None => true,
    };

    if replace {
        map.insert(key.to_string(), path.to_path_buf());
        scores.insert(key.to_string(), score);
    }
}

fn icon_preference_score(path: &Path) -> i32 {
    let mut score = 0;
    let lower = path.to_string_lossy().to_ascii_lowercase();

    if lower.contains("/hicolor/") {
        score += 300;
    }
    if lower.contains("/apps/") {
        score += 150;
    }
    if lower.contains("/scalable/") {
        score += 220;
    }

    match path.extension().and_then(|ext| ext.to_str()) {
        Some("svg") => score += 160,
        Some("png") => score += 120,
        Some("webp") => score += 90,
        Some("jpg" | "jpeg") => score += 70,
        Some("xpm") => score += 30,
        Some("ico") => score += 20,
        _ => {}
    }

    for component in path.components() {
        let text = component.as_os_str().to_string_lossy();
        if let Some((width, height)) = text.split_once('x') {
            if let (Ok(width), Ok(height)) = (width.parse::<i32>(), height.parse::<i32>()) {
                score += width.min(height).min(256);
            }
        }
    }

    score - lower.len() as i32 / 16
}

fn desktop_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut seen = HashSet::new();

    if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
        push_unique(
            &mut dirs,
            &mut seen,
            PathBuf::from(dir).join("applications"),
        );
    } else if let Some(home) = dirs::home_dir() {
        push_unique(
            &mut dirs,
            &mut seen,
            home.join(".local").join("share").join("applications"),
        );
    }

    let data_dirs = xdg_data_dirs();
    for dir in &data_dirs {
        push_unique(&mut dirs, &mut seen, dir.join("applications"));
    }

    dirs
}

fn icon_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut seen = HashSet::new();

    if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
        let base = PathBuf::from(dir);
        push_unique(&mut dirs, &mut seen, base.join("icons"));
        push_unique(&mut dirs, &mut seen, base.join("pixmaps"));
    } else if let Some(home) = dirs::home_dir() {
        push_unique(
            &mut dirs,
            &mut seen,
            home.join(".local").join("share").join("icons"),
        );
        push_unique(
            &mut dirs,
            &mut seen,
            home.join(".local").join("share").join("pixmaps"),
        );
    }

    if let Some(home) = dirs::home_dir() {
        push_unique(&mut dirs, &mut seen, home.join(".icons"));
    }

    for dir in xdg_data_dirs() {
        push_unique(&mut dirs, &mut seen, dir.join("icons"));
        push_unique(&mut dirs, &mut seen, dir.join("pixmaps"));
    }

    dirs
}

fn xdg_data_dirs() -> Vec<PathBuf> {
    std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string())
        .split(':')
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn push_unique(dirs: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>, path: PathBuf) {
    if seen.insert(path.clone()) {
        dirs.push(path);
    }
}

fn key_variants(key: &str) -> Vec<String> {
    let mut variants = Vec::new();
    let base = lookup_base(key);

    if !base.is_empty() {
        variants.push(base.clone());
    }

    if let Some(alias) = base.rsplit('.').next() {
        if !alias.is_empty() && alias != base {
            variants.push(alias.to_string());
        }
    }

    variants.sort();
    variants.dedup();
    variants
}

fn icon_name_variants(icon_name: &str) -> Vec<String> {
    let mut variants = key_variants(icon_name);
    if let Some(stripped) = icon_name.strip_suffix("-symbolic") {
        variants.extend(key_variants(stripped));
    }
    variants.sort();
    variants.dedup();
    variants
}

fn normalize_lookup_key(input: &str) -> String {
    let base = lookup_base(input);
    let mut normalized = String::new();

    for ch in base.chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
        }
    }

    if normalized.is_empty() {
        base.to_ascii_lowercase()
    } else {
        normalized
    }
}

fn lookup_base(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if let Some(path) = local_path_from_uri(trimmed) {
        return basename_without_desktop(path.as_path()).unwrap_or_else(|| trimmed.to_string());
    }

    if trimmed.contains('/') {
        return basename_without_desktop(Path::new(trimmed)).unwrap_or_else(|| trimmed.to_string());
    }

    trimmed.trim_end_matches(".desktop").to_string()
}

fn local_path_from_uri(value: &str) -> Option<PathBuf> {
    value.strip_prefix("file://").map(PathBuf::from)
}

fn basename_without_desktop(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_str()?;
    Some(file_name.trim_end_matches(".desktop").to_string())
}

fn basename_string(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
}

fn desktop_id_from_hint(value: &str) -> Option<String> {
    let base = lookup_base(value);
    if base.is_empty() {
        None
    } else {
        Some(base)
    }
}

fn icon_name_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| stem.to_string())
        .unwrap_or_default()
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn parse_bool(value: &str) -> bool {
    matches!(value.trim(), "true" | "True" | "1")
}

fn is_supported_icon_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("svg" | "png" | "webp" | "jpg" | "jpeg" | "xpm" | "ico")
    )
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_lookup_key, parse_exec_binary, process_desktop_entry_from_env_bytes, AppLookup,
        AppResolver,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn normalize_lookup_key_collapses_common_separators() {
        assert_eq!(
            normalize_lookup_key("Visual Studio Code.desktop"),
            "visualstudiocode"
        );
        assert_eq!(
            normalize_lookup_key("org.wezfurlong.wezterm"),
            "orgwezfurlongwezterm"
        );
    }

    #[test]
    fn parse_exec_binary_skips_env_prefixes() {
        assert_eq!(
            parse_exec_binary("env GTK_USE_PORTAL=1 /usr/bin/firefox %u"),
            Some("firefox".to_string())
        );
        assert_eq!(
            parse_exec_binary("\"/opt/Google Chrome/chrome\" --profile-directory=Default"),
            Some("chrome".to_string())
        );
    }

    #[test]
    fn process_env_desktop_hint_extracts_desktop_id() {
        let env = b"FOO=1\0GIO_LAUNCHED_DESKTOP_FILE=/usr/share/applications/firefox.desktop\0";
        assert_eq!(
            process_desktop_entry_from_env_bytes(env),
            Some("firefox".to_string())
        );
    }

    #[test]
    fn resolver_uses_desktop_entry_for_name_and_icon() {
        let tmp = TestDir::new();
        let desktop_dir = tmp.path.join("applications");
        let icon_dir = tmp.path.join("icons");

        fs::create_dir_all(&desktop_dir).expect("create desktop dir");
        fs::create_dir_all(icon_dir.join("hicolor/scalable/apps")).expect("create icon dir");

        fs::write(
            desktop_dir.join("org.wezfurlong.wezterm.desktop"),
            "[Desktop Entry]\nName=WezTerm\nIcon=org.wezfurlong.wezterm\nExec=wezterm\n",
        )
        .expect("write desktop");
        fs::write(
            icon_dir.join("hicolor/scalable/apps/org.wezfurlong.wezterm.svg"),
            "<svg/>",
        )
        .expect("write icon");

        let resolver = AppResolver::load_from_dirs(vec![desktop_dir], vec![icon_dir]);
        let resolved = resolver.resolve(&AppLookup {
            app_id: Some("org.wezfurlong.wezterm".to_string()),
            ..AppLookup::default()
        });

        assert_eq!(resolved.display_name, "WezTerm");
        assert_eq!(resolved.icon_name, "org.wezfurlong.wezterm");
        assert_eq!(resolved.desktop_entry, "org.wezfurlong.wezterm");
        assert_eq!(resolved.match_source, "app_id");
        assert!(resolved.icon.ends_with("org.wezfurlong.wezterm.svg"));
    }

    #[test]
    fn resolver_prefers_explicit_icon_path_hint() {
        let tmp = TestDir::new();
        let desktop_dir = tmp.path.join("applications");
        let icon_dir = tmp.path.join("icons");
        let custom_icon = tmp.path.join("custom.png");

        fs::create_dir_all(&desktop_dir).expect("create desktop dir");
        fs::create_dir_all(icon_dir.join("hicolor/64x64/apps")).expect("create icon dir");
        fs::write(
            desktop_dir.join("firefox.desktop"),
            "[Desktop Entry]\nName=Firefox\nIcon=firefox\nExec=firefox %u\n",
        )
        .expect("write desktop");
        fs::write(icon_dir.join("hicolor/64x64/apps/firefox.png"), "png").expect("write icon");
        fs::write(&custom_icon, "custom").expect("write custom icon");

        let resolver = AppResolver::load_from_dirs(vec![desktop_dir], vec![icon_dir]);
        let resolved = resolver.resolve(&AppLookup {
            app_id: Some("firefox".to_string()),
            icon_hint: Some(custom_icon.to_string_lossy().into_owned()),
            ..AppLookup::default()
        });

        assert_eq!(resolved.display_name, "Firefox");
        assert_eq!(resolved.match_source, "icon_path");
        assert_eq!(resolved.icon_name, "custom");
        assert_eq!(resolved.icon, custom_icon.to_string_lossy());
    }

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "quicksov-app-resolver-{}-{}",
                std::process::id(),
                unique
            ));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
