// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use tracing::debug;

use crate::util::prettify_app_id;

pub struct AppNameResolver {
    exact: HashMap<String, String>,
    normalized: HashMap<String, String>,
}

impl AppNameResolver {
    pub fn load() -> Self {
        let mut exact = HashMap::new();
        let mut normalized = HashMap::new();

        for dir in desktop_search_dirs() {
            index_desktop_dir(&dir, &mut exact, &mut normalized);
        }

        debug!(
            exact_entries = exact.len(),
            normalized_entries = normalized.len(),
            "loaded desktop entry app-name index"
        );

        Self { exact, normalized }
    }

    pub fn resolve(&self, app_id: &str) -> String {
        if let Some(name) = self.exact.get(app_id) {
            return name.clone();
        }

        let normalized_key = normalize_lookup_key(app_id);
        if let Some(name) = self.normalized.get(&normalized_key) {
            return name.clone();
        }

        prettify_app_id(app_id)
    }
}

fn index_desktop_dir(
    dir: &Path,
    exact: &mut HashMap<String, String>,
    normalized: &mut HashMap<String, String>,
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

            if path.extension().and_then(|s| s.to_str()) != Some("desktop") {
                continue;
            }

            if let Some(desktop) = parse_desktop_entry(&path) {
                insert_mapping(&desktop.desktop_id, &desktop.name, exact, normalized);
                if let Some(startup_wm_class) = desktop.startup_wm_class.as_deref() {
                    insert_mapping(startup_wm_class, &desktop.name, exact, normalized);
                }
            }
        }
    }
}

fn insert_mapping(
    key: &str,
    name: &str,
    exact: &mut HashMap<String, String>,
    normalized: &mut HashMap<String, String>,
) {
    if key.is_empty() || name.is_empty() {
        return;
    }

    exact
        .entry(key.to_string())
        .or_insert_with(|| name.to_string());
    normalized
        .entry(normalize_lookup_key(key))
        .or_insert_with(|| name.to_string());
}

struct DesktopEntry {
    desktop_id: String,
    name: String,
    startup_wm_class: Option<String>,
}

fn parse_desktop_entry(path: &Path) -> Option<DesktopEntry> {
    let desktop_id = path.file_stem()?.to_str()?.to_string();
    let text = fs::read_to_string(path).ok()?;

    let mut in_desktop_entry = false;
    let mut name = None;
    let mut startup_wm_class = None;
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
            "StartupWMClass" => {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    startup_wm_class = Some(trimmed.to_string());
                }
            }
            "NoDisplay" => no_display = parse_bool(value),
            "Hidden" => hidden = parse_bool(value),
            _ => {}
        }
    }

    if hidden || no_display {
        return None;
    }

    Some(DesktopEntry {
        desktop_id,
        name: name?,
        startup_wm_class,
    })
}

fn parse_bool(value: &str) -> bool {
    matches!(value.trim(), "true" | "True" | "1")
}

fn desktop_search_dirs() -> Vec<PathBuf> {
    let mut search_dirs = Vec::new();
    let mut seen = HashSet::new();

    if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
        push_unique(
            &mut search_dirs,
            &mut seen,
            PathBuf::from(dir).join("applications"),
        );
    } else if let Some(home) = dirs::home_dir() {
        push_unique(
            &mut search_dirs,
            &mut seen,
            home.join(".local").join("share").join("applications"),
        );
    }

    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    for dir in data_dirs.split(':').filter(|s| !s.is_empty()) {
        push_unique(
            &mut search_dirs,
            &mut seen,
            PathBuf::from(dir).join("applications"),
        );
    }

    search_dirs
}

fn push_unique(dirs: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>, path: PathBuf) {
    if seen.insert(path.clone()) {
        dirs.push(path);
    }
}

fn normalize_lookup_key(input: &str) -> String {
    input
        .trim()
        .trim_end_matches(".desktop")
        .to_ascii_lowercase()
}
