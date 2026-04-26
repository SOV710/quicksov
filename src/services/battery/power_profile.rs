// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use nix::unistd::{getgid, getgroups, getuid};

use crate::config::paths::QSOSYSD_SOCKET_PATH;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProductPowerProfile {
    PowerSaver,
    Balanced,
    Performance,
    Custom,
    Unknown,
}

impl ProductPowerProfile {
    pub(crate) const SETTABLE: [Self; 3] = [Self::PowerSaver, Self::Balanced, Self::Performance];

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::PowerSaver => "power-saver",
            Self::Balanced => "balanced",
            Self::Performance => "performance",
            Self::Custom => "custom",
            Self::Unknown => "unknown",
        }
    }

    pub(crate) fn from_action_str(value: &str) -> Option<Self> {
        match value {
            "power-saver" => Some(Self::PowerSaver),
            "balanced" => Some(Self::Balanced),
            "performance" => Some(Self::Performance),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PowerProfileBackend {
    PlatformProfile,
    None,
}

impl PowerProfileBackend {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::PlatformProfile => "platform_profile",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PowerProfileReason {
    Unsupported,
    HelperUnavailable,
    PermissionDenied,
    BackendUnavailable,
    WriteFailed,
}

impl PowerProfileReason {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::HelperUnavailable => "helper_unavailable",
            Self::PermissionDenied => "permission_denied",
            Self::BackendUnavailable => "backend_unavailable",
            Self::WriteFailed => "write_failed",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PowerProfileState {
    pub(crate) profile: ProductPowerProfile,
    pub(crate) available: bool,
    pub(crate) backend: PowerProfileBackend,
    pub(crate) reason: Option<PowerProfileReason>,
    pub(crate) choices: Vec<ProductPowerProfile>,
}

impl PowerProfileState {
    pub(crate) fn with_reason_override(&self, reason: PowerProfileReason) -> Self {
        let mut overridden = self.clone();
        overridden.available = false;
        overridden.reason = Some(reason);
        overridden
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PlatformProfilePaths {
    pub(crate) profile_path: PathBuf,
    pub(crate) choices_path: PathBuf,
    pub(crate) helper_socket_path: PathBuf,
}

impl Default for PlatformProfilePaths {
    fn default() -> Self {
        Self {
            profile_path: PathBuf::from("/sys/firmware/acpi/platform_profile"),
            choices_path: PathBuf::from("/sys/firmware/acpi/platform_profile_choices"),
            helper_socket_path: PathBuf::from(QSOSYSD_SOCKET_PATH),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HelperAccess {
    Available,
    Missing,
    PermissionDenied,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum PlatformProfileWriteError {
    #[error("requested profile is not supported by platform_profile_choices")]
    Unsupported,
    #[error("permission denied while writing platform profile: {0}")]
    PermissionDenied(String),
    #[error("platform_profile backend unavailable: {0}")]
    BackendUnavailable(String),
    #[error("platform_profile write failed: {0}")]
    WriteFailed(String),
}

pub(crate) fn read_power_profile_state(paths: &PlatformProfilePaths) -> PowerProfileState {
    if !paths.profile_path.exists() || !paths.choices_path.exists() {
        return PowerProfileState {
            profile: ProductPowerProfile::Unknown,
            available: false,
            backend: PowerProfileBackend::None,
            reason: Some(PowerProfileReason::Unsupported),
            choices: Vec::new(),
        };
    }

    let raw_profile = match read_trimmed(&paths.profile_path) {
        Ok(profile) => profile,
        Err(_) => {
            return PowerProfileState {
                profile: ProductPowerProfile::Unknown,
                available: false,
                backend: PowerProfileBackend::PlatformProfile,
                reason: Some(PowerProfileReason::BackendUnavailable),
                choices: Vec::new(),
            };
        }
    };
    let raw_choices = match read_trimmed(&paths.choices_path) {
        Ok(choices) => parse_raw_choices(&choices),
        Err(_) => {
            return PowerProfileState {
                profile: map_current_profile(&raw_profile),
                available: false,
                backend: PowerProfileBackend::PlatformProfile,
                reason: Some(PowerProfileReason::BackendUnavailable),
                choices: Vec::new(),
            };
        }
    };

    let choices = product_choices_from_raw_choices(&raw_choices);
    let choices_complete = ProductPowerProfile::SETTABLE
        .iter()
        .all(|expected| choices.contains(expected));
    let backend_writable = fs::metadata(&paths.profile_path)
        .map(|metadata| !metadata.permissions().readonly())
        .unwrap_or(false);
    let helper_access = inspect_helper_socket(&paths.helper_socket_path);

    let reason = if !choices_complete {
        Some(PowerProfileReason::Unsupported)
    } else if !backend_writable {
        Some(PowerProfileReason::BackendUnavailable)
    } else {
        match helper_access {
            HelperAccess::Available => None,
            HelperAccess::Missing => Some(PowerProfileReason::HelperUnavailable),
            HelperAccess::PermissionDenied => Some(PowerProfileReason::PermissionDenied),
        }
    };

    PowerProfileState {
        profile: map_current_profile(&raw_profile),
        available: reason.is_none(),
        backend: PowerProfileBackend::PlatformProfile,
        reason,
        choices,
    }
}

pub(crate) fn write_platform_profile(
    paths: &PlatformProfilePaths,
    target: ProductPowerProfile,
) -> Result<String, PlatformProfileWriteError> {
    let choices_raw = read_trimmed(&paths.choices_path)
        .map(|raw| parse_raw_choices(&raw))
        .map_err(map_read_error)?;
    let raw_target =
        select_raw_choice(target, &choices_raw).ok_or(PlatformProfileWriteError::Unsupported)?;

    fs::write(&paths.profile_path, &raw_target).map_err(map_write_error)?;

    let readback = read_trimmed(&paths.profile_path).map_err(map_read_error)?;
    match map_raw_profile(&readback) {
        Some(actual) if actual == target => Ok(readback),
        _ => Err(PlatformProfileWriteError::WriteFailed(format!(
            "read-back mismatch after writing {raw_target:?}: {readback:?}"
        ))),
    }
}

pub(crate) fn parse_raw_choices(raw: &str) -> Vec<String> {
    let mut parsed = Vec::new();
    for token in raw.split_whitespace() {
        let choice = token.trim().trim_matches(['[', ']']);
        if choice.is_empty() || parsed.iter().any(|existing| existing == choice) {
            continue;
        }
        parsed.push(choice.to_string());
    }
    parsed
}

pub(crate) fn product_choices_from_raw_choices(raw_choices: &[String]) -> Vec<ProductPowerProfile> {
    let mut mapped = Vec::new();
    for profile in ProductPowerProfile::SETTABLE {
        if raw_choices
            .iter()
            .any(|raw| map_raw_profile(raw.as_str()) == Some(profile))
        {
            mapped.push(profile);
        }
    }
    mapped
}

pub(crate) fn select_raw_choice(
    target: ProductPowerProfile,
    raw_choices: &[String],
) -> Option<String> {
    preferred_raw_names(target)
        .iter()
        .find_map(|candidate| raw_choices.iter().find(|raw| raw.as_str() == *candidate))
        .cloned()
}

pub(crate) fn map_raw_profile(raw: &str) -> Option<ProductPowerProfile> {
    match raw.trim() {
        "quiet" | "cool" | "low-power" => Some(ProductPowerProfile::PowerSaver),
        "balanced" | "balanced-performance" => Some(ProductPowerProfile::Balanced),
        "performance" | "max-performance" => Some(ProductPowerProfile::Performance),
        _ => None,
    }
}

fn map_current_profile(raw: &str) -> ProductPowerProfile {
    match raw.trim() {
        "" => ProductPowerProfile::Unknown,
        other => map_raw_profile(other).unwrap_or(ProductPowerProfile::Custom),
    }
}

fn preferred_raw_names(target: ProductPowerProfile) -> &'static [&'static str] {
    match target {
        ProductPowerProfile::PowerSaver => &["quiet", "cool", "low-power"],
        ProductPowerProfile::Balanced => &["balanced", "balanced-performance"],
        ProductPowerProfile::Performance => &["performance", "max-performance"],
        ProductPowerProfile::Custom | ProductPowerProfile::Unknown => &[],
    }
}

fn inspect_helper_socket(path: &Path) -> HelperAccess {
    let Ok(metadata) = fs::metadata(path) else {
        return HelperAccess::Missing;
    };
    if !metadata.file_type().is_socket() {
        return HelperAccess::Missing;
    }
    if current_identity_can_write_socket(&metadata) {
        HelperAccess::Available
    } else {
        HelperAccess::PermissionDenied
    }
}

fn current_identity_can_write_socket(metadata: &fs::Metadata) -> bool {
    let mode = metadata.permissions().mode();
    let uid = metadata.uid();
    let gid = metadata.gid();
    let current_uid = getuid().as_raw();

    if current_uid == 0 {
        return (mode & 0o200) != 0;
    }
    if uid == current_uid {
        return (mode & 0o200) != 0;
    }
    if current_groups().iter().any(|candidate| *candidate == gid) {
        return (mode & 0o020) != 0;
    }
    (mode & 0o002) != 0
}

fn current_groups() -> Vec<u32> {
    let mut groups = Vec::new();
    groups.push(getgid().as_raw());
    if let Ok(extra) = getgroups() {
        groups.extend(extra.into_iter().map(|gid| gid.as_raw()));
    }
    groups.sort_unstable();
    groups.dedup();
    groups
}

fn read_trimmed(path: &Path) -> Result<String, std::io::Error> {
    fs::read_to_string(path).map(|value| value.trim().to_string())
}

fn map_read_error(error: std::io::Error) -> PlatformProfileWriteError {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
        PlatformProfileWriteError::PermissionDenied(error.to_string())
    } else {
        PlatformProfileWriteError::BackendUnavailable(error.to_string())
    }
}

fn map_write_error(error: std::io::Error) -> PlatformProfileWriteError {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
        PlatformProfileWriteError::PermissionDenied(error.to_string())
    } else {
        PlatformProfileWriteError::WriteFailed(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        map_raw_profile, parse_raw_choices, product_choices_from_raw_choices,
        read_power_profile_state, select_raw_choice, write_platform_profile, PlatformProfilePaths,
        PlatformProfileWriteError, PowerProfileBackend, PowerProfileReason, ProductPowerProfile,
    };

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
                "quicksov-power-profile-{}-{}",
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

    fn test_paths(dir: &TestDir) -> PlatformProfilePaths {
        PlatformProfilePaths {
            profile_path: dir.path.join("platform_profile"),
            choices_path: dir.path.join("platform_profile_choices"),
            helper_socket_path: dir.path.join("qsosysd.sock"),
        }
    }

    #[test]
    fn raw_profiles_map_to_product_profiles() {
        assert_eq!(
            map_raw_profile("low-power"),
            Some(ProductPowerProfile::PowerSaver)
        );
        assert_eq!(
            map_raw_profile("balanced-performance"),
            Some(ProductPowerProfile::Balanced)
        );
        assert_eq!(
            map_raw_profile("max-performance"),
            Some(ProductPowerProfile::Performance)
        );
        assert_eq!(map_raw_profile("vendor-custom"), None);
    }

    #[test]
    fn raw_choices_are_deduplicated_and_mapped_in_product_order() {
        let raw = parse_raw_choices("performance balanced max-performance low-power");
        assert_eq!(
            product_choices_from_raw_choices(&raw),
            vec![
                ProductPowerProfile::PowerSaver,
                ProductPowerProfile::Balanced,
                ProductPowerProfile::Performance
            ]
        );
    }

    #[test]
    fn select_raw_choice_prefers_product_ordering() {
        let raw_choices = vec![
            "cool".to_string(),
            "balanced-performance".to_string(),
            "max-performance".to_string(),
        ];
        assert_eq!(
            select_raw_choice(ProductPowerProfile::PowerSaver, &raw_choices),
            Some("cool".to_string())
        );
        assert_eq!(
            select_raw_choice(ProductPowerProfile::Balanced, &raw_choices),
            Some("balanced-performance".to_string())
        );
        assert_eq!(
            select_raw_choice(ProductPowerProfile::Performance, &raw_choices),
            Some("max-performance".to_string())
        );
    }

    #[test]
    fn read_state_reports_custom_profile_but_enabled_choices() {
        let dir = TestDir::new();
        let paths = test_paths(&dir);

        fs::write(&paths.profile_path, "vendor-special").expect("write profile");
        fs::write(&paths.choices_path, "low-power balanced performance").expect("write choices");

        let state = read_power_profile_state(&paths);
        assert_eq!(state.backend, PowerProfileBackend::PlatformProfile);
        assert_eq!(state.profile, ProductPowerProfile::Custom);
        assert_eq!(
            state.choices,
            vec![
                ProductPowerProfile::PowerSaver,
                ProductPowerProfile::Balanced,
                ProductPowerProfile::Performance
            ]
        );
        assert_eq!(state.reason, Some(PowerProfileReason::HelperUnavailable));
        assert!(!state.available);
    }

    #[test]
    fn write_platform_profile_round_trips_successfully() {
        let dir = TestDir::new();
        let paths = test_paths(&dir);

        fs::write(&paths.profile_path, "balanced").expect("write profile");
        fs::write(&paths.choices_path, "low-power balanced performance").expect("write choices");

        let readback = write_platform_profile(&paths, ProductPowerProfile::PowerSaver)
            .expect("write should succeed");
        assert_eq!(readback, "low-power");
        let final_raw = fs::read_to_string(&paths.profile_path).expect("read final profile");
        assert_eq!(final_raw, "low-power");
    }

    #[test]
    fn write_platform_profile_rejects_unsupported_targets() {
        let dir = TestDir::new();
        let paths = test_paths(&dir);

        fs::write(&paths.profile_path, "balanced").expect("write profile");
        fs::write(&paths.choices_path, "balanced performance").expect("write choices");

        let error = write_platform_profile(&paths, ProductPowerProfile::PowerSaver)
            .expect_err("unsupported target should fail");
        assert!(matches!(error, PlatformProfileWriteError::Unsupported));
    }

    #[test]
    fn write_platform_profile_reports_read_back_mismatch() {
        let dir = TestDir::new();
        let paths = test_paths(&dir);

        fs::write(&paths.profile_path, "balanced").expect("write profile");
        fs::write(&paths.choices_path, "low-power balanced performance").expect("write choices");
        fs::remove_file(&paths.profile_path).expect("remove profile");
        fs::create_dir(&paths.profile_path).expect("replace profile with dir");

        let error = write_platform_profile(&paths, ProductPowerProfile::Performance)
            .expect_err("write failure should surface");
        assert!(matches!(error, PlatformProfileWriteError::WriteFailed(_)));
    }
}
