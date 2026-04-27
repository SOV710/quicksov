// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;
use std::time::Duration;

use tracing::debug;
use zbus::zvariant::OwnedValue;

use crate::bus::ServiceError;

pub(crate) const PPD_DEST: &str = "org.freedesktop.UPower.PowerProfiles";
pub(crate) const PPD_PATH: &str = "/org/freedesktop/UPower/PowerProfiles";
pub(crate) const PPD_IFACE: &str = "org.freedesktop.UPower.PowerProfiles";

const PPD_ACTION_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProductPowerProfile {
    PowerSaver,
    Balanced,
    Performance,
    Custom,
    Unknown,
}

impl ProductPowerProfile {
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

    fn from_backend_str(value: &str) -> Option<Self> {
        match value.trim() {
            "power-saver" => Some(Self::PowerSaver),
            "balanced" => Some(Self::Balanced),
            "performance" => Some(Self::Performance),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PowerProfileBackend {
    PowerProfilesDaemon,
    None,
}

impl PowerProfileBackend {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::PowerProfilesDaemon => "power_profiles_daemon",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PowerProfileReason {
    Unsupported,
    ServiceUnavailable,
    PermissionDenied,
    WriteFailed,
}

impl PowerProfileReason {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::ServiceUnavailable => "service_unavailable",
            Self::PermissionDenied => "permission_denied",
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
    pub(crate) degraded_reason: Option<String>,
}

impl PowerProfileState {
    pub(crate) fn service_unavailable() -> Self {
        Self {
            profile: ProductPowerProfile::Unknown,
            available: false,
            backend: PowerProfileBackend::None,
            reason: Some(PowerProfileReason::ServiceUnavailable),
            choices: Vec::new(),
            degraded_reason: None,
        }
    }

    pub(crate) fn with_reason_override(&self, reason: PowerProfileReason) -> Self {
        let mut overridden = self.clone();
        overridden.available = false;
        overridden.reason = Some(reason);
        overridden
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SetPowerProfileError {
    #[error("power profile service is unavailable: {0}")]
    ServiceUnavailable(String),
    #[error("power profile change was denied: {0}")]
    PermissionDenied(String),
    #[error("power profile write failed: {0}")]
    WriteFailed(String),
    #[error("requested power profile is not supported on this system")]
    Unsupported,
}

impl SetPowerProfileError {
    pub(crate) fn reason(&self) -> Option<PowerProfileReason> {
        match self {
            Self::ServiceUnavailable(_) => Some(PowerProfileReason::ServiceUnavailable),
            Self::PermissionDenied(_) => Some(PowerProfileReason::PermissionDenied),
            Self::WriteFailed(_) => Some(PowerProfileReason::WriteFailed),
            Self::Unsupported => None,
        }
    }

    pub(crate) fn into_service_error(self) -> ServiceError {
        match self {
            Self::ServiceUnavailable(_) | Self::Unsupported => ServiceError::Unavailable,
            Self::PermissionDenied(msg) => ServiceError::Permission { msg },
            Self::WriteFailed(msg) => ServiceError::Internal { msg },
        }
    }
}

pub(crate) async fn read_power_profile_state(conn: &zbus::Connection) -> PowerProfileState {
    match read_power_profile_state_inner(conn).await {
        Ok(state) => state,
        Err(error) => {
            debug!(error = %error, "power-profiles-daemon state refresh failed");
            PowerProfileState::service_unavailable()
        }
    }
}

pub(crate) async fn set_power_profile(
    conn: &zbus::Connection,
    target: ProductPowerProfile,
) -> Result<(), SetPowerProfileError> {
    let current = match read_power_profile_state_inner(conn).await {
        Ok(state) => state,
        Err(error) => return Err(map_read_error("read power profile state", error)),
    };

    if current.backend == PowerProfileBackend::None {
        return Err(SetPowerProfileError::ServiceUnavailable(
            "power-profiles-daemon is unavailable".to_string(),
        ));
    }
    if !current.choices.contains(&target) {
        return Err(SetPowerProfileError::Unsupported);
    }

    let proxy = ppd_proxy(conn)
        .await
        .map_err(|error| map_read_error("open power profile proxy", error))?;
    tokio::time::timeout(
        PPD_ACTION_TIMEOUT,
        proxy.set_property("ActiveProfile", target.as_str()),
    )
    .await
    .map_err(|_| {
        SetPowerProfileError::ServiceUnavailable(
            "power-profiles-daemon request timed out".to_string(),
        )
    })?
    .map_err(|error| map_write_error("set power profile", error.into()))?;

    let active_profile: String =
        tokio::time::timeout(PPD_ACTION_TIMEOUT, proxy.get_property("ActiveProfile"))
            .await
            .map_err(|_| {
                SetPowerProfileError::ServiceUnavailable(
                    "power-profiles-daemon read-back timed out".to_string(),
                )
            })?
            .map_err(|error| map_read_error("read back active power profile", error))?;

    match map_current_profile(&active_profile) {
        ProductPowerProfile::PowerSaver
        | ProductPowerProfile::Balanced
        | ProductPowerProfile::Performance
            if map_current_profile(&active_profile) == target =>
        {
            Ok(())
        }
        _ => Err(SetPowerProfileError::WriteFailed(format!(
            "power-profiles-daemon applied unexpected profile {active_profile:?}"
        ))),
    }
}

async fn read_power_profile_state_inner(
    conn: &zbus::Connection,
) -> Result<PowerProfileState, zbus::Error> {
    let proxy = ppd_proxy(conn).await?;
    let active_profile: String = proxy.get_property("ActiveProfile").await?;
    let profiles: Vec<HashMap<String, OwnedValue>> = proxy.get_property("Profiles").await?;
    let degraded_reason: String = proxy.get_property("PerformanceDegraded").await?;

    let choices = parse_profiles(&profiles);
    let available = choices.len() >= 2;
    Ok(PowerProfileState {
        profile: map_current_profile(&active_profile),
        available,
        backend: PowerProfileBackend::PowerProfilesDaemon,
        reason: (!available).then_some(PowerProfileReason::Unsupported),
        choices,
        degraded_reason: normalize_degraded_reason(&degraded_reason),
    })
}

async fn ppd_proxy(conn: &zbus::Connection) -> Result<zbus::Proxy<'_>, zbus::Error> {
    zbus::Proxy::new(conn, PPD_DEST, PPD_PATH, PPD_IFACE).await
}

fn parse_profiles(profiles: &[HashMap<String, OwnedValue>]) -> Vec<ProductPowerProfile> {
    let mut mapped = Vec::new();
    for entry in profiles {
        let Some(name) = owned_prop::<String>(entry, &["Profile", "profile"]) else {
            continue;
        };
        let Some(profile) = ProductPowerProfile::from_backend_str(&name) else {
            continue;
        };
        if !mapped.contains(&profile) {
            mapped.push(profile);
        }
    }
    mapped
}

fn map_current_profile(raw: &str) -> ProductPowerProfile {
    match raw.trim() {
        "" => ProductPowerProfile::Unknown,
        other => {
            ProductPowerProfile::from_backend_str(other).unwrap_or(ProductPowerProfile::Custom)
        }
    }
}

fn normalize_degraded_reason(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn map_read_error(context: &str, error: zbus::Error) -> SetPowerProfileError {
    map_zbus_error(context, error, false)
}

fn map_write_error(context: &str, error: zbus::Error) -> SetPowerProfileError {
    map_zbus_error(context, error, true)
}

fn map_zbus_error(context: &str, error: zbus::Error, write: bool) -> SetPowerProfileError {
    match &error {
        zbus::Error::MethodError(name, detail, _) => {
            let name = name.as_str();
            if is_permission_error_name(name) {
                return SetPowerProfileError::PermissionDenied(render_error_message(
                    context,
                    detail.as_deref(),
                    "power profile change was denied",
                ));
            }
            if is_unavailable_error_name(name) {
                return SetPowerProfileError::ServiceUnavailable(render_error_message(
                    context,
                    detail.as_deref(),
                    "power profile service is unavailable",
                ));
            }
            if write {
                return SetPowerProfileError::WriteFailed(render_error_message(
                    context,
                    detail.as_deref(),
                    "power profile change failed",
                ));
            }
            SetPowerProfileError::ServiceUnavailable(render_error_message(
                context,
                detail.as_deref(),
                "power profile service is unavailable",
            ))
        }
        zbus::Error::FDO(fdo) => {
            let text = fdo.to_string();
            if is_permission_error_text(&text) {
                SetPowerProfileError::PermissionDenied(render_error_message(
                    context,
                    Some(&text),
                    "power profile change was denied",
                ))
            } else if is_unavailable_error_text(&text) {
                SetPowerProfileError::ServiceUnavailable(render_error_message(
                    context,
                    Some(&text),
                    "power profile service is unavailable",
                ))
            } else if write {
                SetPowerProfileError::WriteFailed(render_error_message(
                    context,
                    Some(&text),
                    "power profile change failed",
                ))
            } else {
                SetPowerProfileError::ServiceUnavailable(render_error_message(
                    context,
                    Some(&text),
                    "power profile service is unavailable",
                ))
            }
        }
        other => {
            let text = other.to_string();
            if is_permission_error_text(&text) {
                SetPowerProfileError::PermissionDenied(render_error_message(
                    context,
                    Some(&text),
                    "power profile change was denied",
                ))
            } else if is_unavailable_error_text(&text) {
                SetPowerProfileError::ServiceUnavailable(render_error_message(
                    context,
                    Some(&text),
                    "power profile service is unavailable",
                ))
            } else if write {
                SetPowerProfileError::WriteFailed(render_error_message(
                    context,
                    Some(&text),
                    "power profile change failed",
                ))
            } else {
                SetPowerProfileError::ServiceUnavailable(render_error_message(
                    context,
                    Some(&text),
                    "power profile service is unavailable",
                ))
            }
        }
    }
}

fn render_error_message(context: &str, detail: Option<&str>, fallback: &str) -> String {
    if let Some(detail) = clean_error_detail(detail) {
        return format!("{context}: {detail}");
    }
    fallback.to_string()
}

fn clean_error_detail(detail: Option<&str>) -> Option<String> {
    let detail = detail?.trim();
    if detail.is_empty() {
        return None;
    }
    if matches!(detail, "Failed" | "org.freedesktop.zbus.Error: Failed") {
        return None;
    }
    Some(detail.to_string())
}

fn is_permission_error_name(name: &str) -> bool {
    matches!(
        name,
        "org.freedesktop.DBus.Error.AccessDenied"
            | "org.freedesktop.DBus.Error.AuthFailed"
            | "org.freedesktop.DBus.Error.InteractiveAuthorizationRequired"
            | "org.freedesktop.PolicyKit1.Error.Cancelled"
            | "org.freedesktop.PolicyKit1.Error.NotAuthorized"
    ) || name.contains("AccessDenied")
        || name.contains("PermissionDenied")
        || name.contains("NotAuthorized")
        || name.contains("Cancelled")
}

fn is_unavailable_error_name(name: &str) -> bool {
    matches!(
        name,
        "org.freedesktop.DBus.Error.NameHasNoOwner"
            | "org.freedesktop.DBus.Error.NoReply"
            | "org.freedesktop.DBus.Error.ServiceUnknown"
            | "org.freedesktop.DBus.Error.TimedOut"
            | "org.freedesktop.DBus.Error.Timeout"
            | "org.freedesktop.DBus.Error.UnknownInterface"
            | "org.freedesktop.DBus.Error.UnknownMethod"
            | "org.freedesktop.DBus.Error.UnknownObject"
    ) || name.contains("NoReply")
        || name.contains("TimedOut")
        || name.contains("Timeout")
        || name.contains("ServiceUnknown")
        || name.contains("NameHasNoOwner")
        || name.contains("UnknownObject")
        || name.contains("UnknownInterface")
}

fn is_permission_error_text(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("access denied")
        || lower.contains("permission denied")
        || lower.contains("not authorized")
        || lower.contains("authorization")
        || lower.contains("cancelled")
}

fn is_unavailable_error_text(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("name has no owner")
        || lower.contains("service unknown")
        || lower.contains("unknown object")
        || lower.contains("unknown interface")
        || lower.contains("no reply")
        || lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("disconnected")
}

fn owned_prop<T>(props: &HashMap<String, OwnedValue>, keys: &[&str]) -> Option<T>
where
    T: TryFrom<OwnedValue>,
{
    for key in keys {
        if let Some(value) = props.get(*key) {
            if let Ok(parsed) = T::try_from(value.clone()) {
                return Some(parsed);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use zbus::zvariant::{OwnedValue, Value};

    use super::{
        is_permission_error_name, is_unavailable_error_name, map_current_profile,
        normalize_degraded_reason, parse_profiles, PowerProfileBackend, PowerProfileReason,
        ProductPowerProfile, SetPowerProfileError,
    };

    #[test]
    fn profiles_follow_backend_order_and_deduplicate() {
        fn ov_string(value: &str) -> OwnedValue {
            OwnedValue::try_from(Value::from(value)).expect("owned string value")
        }

        let profiles = parse_profiles(&[
            HashMap::from([("Profile".to_string(), ov_string("balanced"))]),
            HashMap::from([("Profile".to_string(), ov_string("performance"))]),
            HashMap::from([("Profile".to_string(), ov_string("balanced"))]),
            HashMap::from([("Profile".to_string(), ov_string("power-saver"))]),
        ]);

        assert_eq!(
            profiles,
            vec![
                ProductPowerProfile::Balanced,
                ProductPowerProfile::Performance,
                ProductPowerProfile::PowerSaver,
            ]
        );
    }

    #[test]
    fn degraded_reason_normalizes_empty_string() {
        assert_eq!(normalize_degraded_reason(""), None);
        assert_eq!(
            normalize_degraded_reason("high-operating-temperature"),
            Some("high-operating-temperature".to_string())
        );
    }

    #[test]
    fn maps_current_profile_to_custom_when_unknown() {
        assert_eq!(
            map_current_profile("balanced"),
            ProductPowerProfile::Balanced
        );
        assert_eq!(
            map_current_profile("vendor-special"),
            ProductPowerProfile::Custom
        );
    }

    #[test]
    fn recognizes_permission_and_unavailable_error_names() {
        assert!(is_permission_error_name(
            "org.freedesktop.DBus.Error.AccessDenied"
        ));
        assert!(is_unavailable_error_name(
            "org.freedesktop.DBus.Error.NameHasNoOwner"
        ));
    }

    #[test]
    fn override_reason_maps_to_service_error() {
        let error = SetPowerProfileError::PermissionDenied("denied".to_string());
        assert_eq!(error.reason(), Some(PowerProfileReason::PermissionDenied));
    }

    #[test]
    fn backend_string_matches_public_schema() {
        assert_eq!(
            PowerProfileBackend::PowerProfilesDaemon.as_str(),
            "power_profiles_daemon"
        );
    }
}
