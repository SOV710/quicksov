// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;
use std::path::Path;

use zbus::zvariant::{OwnedObjectPath, OwnedValue};

pub(crate) const UPOWER_DEST: &str = "org.freedesktop.UPower";
pub(crate) const UPOWER_PATH: &str = "/org/freedesktop/UPower";
pub(crate) const UPOWER_IFACE: &str = "org.freedesktop.UPower";
const UPOWER_DEVICE_IFACE: &str = "org.freedesktop.UPower.Device";

const DEVICE_TYPE_BATTERY: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BatteryAvailability {
    Ready,
    NoBattery,
    BackendUnavailable,
}

impl BatteryAvailability {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::NoBattery => "no_battery",
            Self::BackendUnavailable => "backend_unavailable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChargeState {
    Charging,
    Discharging,
    FullyCharged,
    NotCharging,
    Empty,
    Unknown,
}

impl ChargeState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Charging => "charging",
            Self::Discharging => "discharging",
            Self::FullyCharged => "fully_charged",
            Self::NotCharging => "not_charging",
            Self::Empty => "empty",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct BatteryEntryState {
    pub(crate) name: String,
    pub(crate) present: bool,
    pub(crate) level: i64,
    pub(crate) state: ChargeState,
    pub(crate) health_percent: Option<f64>,
    pub(crate) energy_rate_w: Option<f64>,
    pub(crate) energy_now_wh: Option<f64>,
    pub(crate) energy_full_wh: Option<f64>,
    pub(crate) energy_design_wh: Option<f64>,
}

#[derive(Debug, Clone)]
pub(crate) struct BatteryTelemetry {
    pub(crate) availability: BatteryAvailability,
    pub(crate) present: bool,
    pub(crate) on_battery: bool,
    pub(crate) level: i64,
    pub(crate) state: ChargeState,
    pub(crate) time_to_empty_sec: Option<i64>,
    pub(crate) time_to_full_sec: Option<i64>,
    pub(crate) health_percent: Option<f64>,
    pub(crate) energy_rate_w: Option<f64>,
    pub(crate) energy_now_wh: Option<f64>,
    pub(crate) energy_full_wh: Option<f64>,
    pub(crate) energy_design_wh: Option<f64>,
    pub(crate) batteries: Vec<BatteryEntryState>,
}

impl BatteryTelemetry {
    pub(crate) fn backend_unavailable() -> Self {
        Self {
            availability: BatteryAvailability::BackendUnavailable,
            present: false,
            on_battery: false,
            level: 0,
            state: ChargeState::Unknown,
            time_to_empty_sec: None,
            time_to_full_sec: None,
            health_percent: None,
            energy_rate_w: None,
            energy_now_wh: None,
            energy_full_wh: None,
            energy_design_wh: None,
            batteries: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct RawBatteryDevice {
    name: String,
    present: bool,
    level: i64,
    state: ChargeState,
    health_percent: Option<f64>,
    energy_rate_w: Option<f64>,
    energy_now_wh: Option<f64>,
    energy_full_wh: Option<f64>,
    energy_design_wh: Option<f64>,
}

#[derive(Debug, Clone)]
struct RawDisplayDevice {
    level: i64,
    state: ChargeState,
    time_to_empty_sec: Option<i64>,
    time_to_full_sec: Option<i64>,
}

pub(crate) async fn read_battery_telemetry(
    conn: &zbus::Connection,
) -> Result<BatteryTelemetry, zbus::Error> {
    let upower_proxy = zbus::Proxy::new(conn, UPOWER_DEST, UPOWER_PATH, UPOWER_IFACE).await?;
    let upower_props = get_all_properties(conn, UPOWER_DEST, UPOWER_PATH, UPOWER_IFACE).await?;
    let on_battery =
        owned_prop::<bool>(&upower_props, &["OnBattery", "on-battery"]).unwrap_or(false);

    let display_path: OwnedObjectPath = upower_proxy.call("GetDisplayDevice", &()).await?;
    let display_props = get_all_properties(
        conn,
        UPOWER_DEST,
        display_path.as_str(),
        UPOWER_DEVICE_IFACE,
    )
    .await?;
    let display = parse_display_device(&display_props).ok_or_else(|| {
        zbus::Error::Failure("UPower display device snapshot incomplete".to_string())
    })?;

    let device_paths: Vec<OwnedObjectPath> = upower_proxy.call("EnumerateDevices", &()).await?;
    let mut batteries = Vec::new();
    for path in device_paths {
        if let Some(device) = read_battery_device(conn, path.as_str()).await? {
            batteries.push(device);
        }
    }
    batteries.sort_by(|left, right| left.name.cmp(&right.name));

    Ok(build_telemetry(display, batteries, on_battery))
}

async fn read_battery_device(
    conn: &zbus::Connection,
    path: &str,
) -> Result<Option<RawBatteryDevice>, zbus::Error> {
    let props = get_all_properties(conn, UPOWER_DEST, path, UPOWER_DEVICE_IFACE).await?;
    let device_type = owned_prop::<u32>(&props, &["Type", "type"]).unwrap_or_default();
    let power_supply =
        owned_prop::<bool>(&props, &["PowerSupply", "power-supply"]).unwrap_or(false);
    if device_type != DEVICE_TYPE_BATTERY || !power_supply {
        return Ok(None);
    }

    Ok(Some(parse_battery_device(path, &props)))
}

fn parse_display_device(props: &HashMap<String, OwnedValue>) -> Option<RawDisplayDevice> {
    Some(RawDisplayDevice {
        level: round_percent(owned_prop::<f64>(props, &["Percentage", "percentage"])?),
        state: map_charge_state(owned_prop::<u32>(props, &["State", "state"]).unwrap_or_default()),
        time_to_empty_sec: normalize_time(owned_prop::<i64>(
            props,
            &["TimeToEmpty", "time-to-empty"],
        )),
        time_to_full_sec: normalize_time(owned_prop::<i64>(props, &["TimeToFull", "time-to-full"])),
    })
}

fn parse_battery_device(path: &str, props: &HashMap<String, OwnedValue>) -> RawBatteryDevice {
    let energy_now_wh = finite_non_negative(owned_prop::<f64>(props, &["Energy", "energy"]));
    let energy_full_wh =
        finite_non_negative(owned_prop::<f64>(props, &["EnergyFull", "energy-full"]));
    let energy_design_wh = finite_non_negative(owned_prop::<f64>(
        props,
        &["EnergyFullDesign", "energy-full-design"],
    ));
    let health_percent = finite_percent(owned_prop::<f64>(props, &["Capacity", "capacity"]))
        .or_else(|| percentage_f64(energy_full_wh, energy_design_wh));
    let level = owned_prop::<f64>(props, &["Percentage", "percentage"])
        .map(round_percent)
        .or_else(|| percentage_from_totals(energy_now_wh, energy_full_wh))
        .unwrap_or(0);

    RawBatteryDevice {
        name: device_name(path, props),
        present: owned_prop::<bool>(props, &["IsPresent", "is-present"]).unwrap_or(true),
        level,
        state: map_charge_state(owned_prop::<u32>(props, &["State", "state"]).unwrap_or_default()),
        health_percent,
        energy_rate_w: finite_non_negative(
            owned_prop::<f64>(props, &["EnergyRate", "energy-rate"]).map(f64::abs),
        ),
        energy_now_wh,
        energy_full_wh,
        energy_design_wh,
    }
}

fn build_telemetry(
    display: RawDisplayDevice,
    batteries: Vec<RawBatteryDevice>,
    on_battery: bool,
) -> BatteryTelemetry {
    let entries = batteries
        .iter()
        .map(|battery| BatteryEntryState {
            name: battery.name.clone(),
            present: battery.present,
            level: battery.level,
            state: battery.state,
            health_percent: battery.health_percent,
            energy_rate_w: battery.energy_rate_w,
            energy_now_wh: battery.energy_now_wh,
            energy_full_wh: battery.energy_full_wh,
            energy_design_wh: battery.energy_design_wh,
        })
        .collect::<Vec<_>>();

    let present_samples = batteries
        .iter()
        .filter(|battery| battery.present)
        .collect::<Vec<_>>();
    if present_samples.is_empty() {
        return BatteryTelemetry {
            availability: BatteryAvailability::NoBattery,
            present: false,
            on_battery,
            level: 0,
            state: ChargeState::Unknown,
            time_to_empty_sec: None,
            time_to_full_sec: None,
            health_percent: None,
            energy_rate_w: None,
            energy_now_wh: None,
            energy_full_wh: None,
            energy_design_wh: None,
            batteries: entries,
        };
    }

    let energy_now_wh = sum_metric(present_samples.iter().map(|battery| battery.energy_now_wh));
    let energy_full_wh = sum_metric(present_samples.iter().map(|battery| battery.energy_full_wh));
    let energy_design_wh = sum_metric(
        present_samples
            .iter()
            .map(|battery| battery.energy_design_wh),
    );
    let energy_rate_w = sum_metric(present_samples.iter().map(|battery| battery.energy_rate_w));
    let health_percent = percentage_f64(energy_full_wh, energy_design_wh);

    BatteryTelemetry {
        availability: BatteryAvailability::Ready,
        present: true,
        on_battery,
        level: display.level,
        state: display.state,
        time_to_empty_sec: display.time_to_empty_sec,
        time_to_full_sec: display.time_to_full_sec,
        health_percent,
        energy_rate_w,
        energy_now_wh,
        energy_full_wh,
        energy_design_wh,
        batteries: entries,
    }
}

async fn get_all_properties(
    conn: &zbus::Connection,
    destination: &str,
    path: &str,
    iface: &str,
) -> Result<HashMap<String, OwnedValue>, zbus::Error> {
    let proxy =
        zbus::Proxy::new(conn, destination, path, "org.freedesktop.DBus.Properties").await?;
    proxy.call("GetAll", &(iface,)).await
}

fn device_name(path: &str, props: &HashMap<String, OwnedValue>) -> String {
    if let Some(native_path) = owned_prop::<String>(props, &["NativePath", "native-path"]) {
        if let Some(name) = Path::new(&native_path)
            .file_name()
            .and_then(|value| value.to_str())
        {
            if !name.is_empty() {
                return name.to_string();
            }
        }
    }

    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("battery")
        .to_string()
}

fn map_charge_state(raw: u32) -> ChargeState {
    match raw {
        1 => ChargeState::Charging,
        2 => ChargeState::Discharging,
        3 => ChargeState::Empty,
        4 => ChargeState::FullyCharged,
        5 | 6 => ChargeState::NotCharging,
        _ => ChargeState::Unknown,
    }
}

fn normalize_time(value: Option<i64>) -> Option<i64> {
    value.filter(|seconds| *seconds > 0)
}

fn round_percent(value: f64) -> i64 {
    value.round().clamp(0.0, 100.0) as i64
}

fn finite_non_negative(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite() && *value >= 0.0)
}

fn finite_percent(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite() && *value > 0.0 && *value <= 100.0)
}

fn percentage_f64(numerator: Option<f64>, denominator: Option<f64>) -> Option<f64> {
    match (numerator, denominator) {
        (Some(num), Some(den)) if den > 0.0 => Some((num / den * 100.0).clamp(0.0, 100.0)),
        _ => None,
    }
}

fn percentage_from_totals(numerator: Option<f64>, denominator: Option<f64>) -> Option<i64> {
    percentage_f64(numerator, denominator).map(round_percent_from_fraction)
}

fn round_percent_from_fraction(value: f64) -> i64 {
    value.round().clamp(0.0, 100.0) as i64
}

fn sum_metric(values: impl Iterator<Item = Option<f64>>) -> Option<f64> {
    let mut sum = 0.0;
    let mut seen = false;
    for value in values.flatten() {
        sum += value;
        seen = true;
    }
    seen.then_some(sum)
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
        build_telemetry, device_name, map_charge_state, parse_battery_device, parse_display_device,
        BatteryAvailability, ChargeState, RawBatteryDevice, RawDisplayDevice,
    };

    #[test]
    fn display_device_uses_upower_time_values() {
        let display = RawDisplayDevice {
            level: 82,
            state: ChargeState::Charging,
            time_to_empty_sec: None,
            time_to_full_sec: Some(1800),
        };
        let telemetry = build_telemetry(
            display,
            vec![RawBatteryDevice {
                name: "BAT0".to_string(),
                present: true,
                level: 82,
                state: ChargeState::Charging,
                health_percent: Some(91.0),
                energy_rate_w: Some(24.0),
                energy_now_wh: Some(48.0),
                energy_full_wh: Some(60.0),
                energy_design_wh: Some(66.0),
            }],
            false,
        );

        assert_eq!(telemetry.availability, BatteryAvailability::Ready);
        assert_eq!(telemetry.level, 82);
        assert_eq!(telemetry.state, ChargeState::Charging);
        assert_eq!(telemetry.time_to_empty_sec, None);
        assert_eq!(telemetry.time_to_full_sec, Some(1800));
    }

    #[test]
    fn aggregates_multiple_batteries_from_energy_totals() {
        let telemetry = build_telemetry(
            RawDisplayDevice {
                level: 50,
                state: ChargeState::Discharging,
                time_to_empty_sec: Some(7200),
                time_to_full_sec: None,
            },
            vec![
                RawBatteryDevice {
                    name: "BAT0".to_string(),
                    present: true,
                    level: 49,
                    state: ChargeState::Discharging,
                    health_percent: Some(82.0),
                    energy_rate_w: Some(10.0),
                    energy_now_wh: Some(20.0),
                    energy_full_wh: Some(40.0),
                    energy_design_wh: Some(50.0),
                },
                RawBatteryDevice {
                    name: "BAT1".to_string(),
                    present: true,
                    level: 52,
                    state: ChargeState::Discharging,
                    health_percent: Some(78.0),
                    energy_rate_w: Some(5.0),
                    energy_now_wh: Some(10.0),
                    energy_full_wh: Some(20.0),
                    energy_design_wh: Some(25.0),
                },
            ],
            true,
        );

        assert_eq!(telemetry.level, 50);
        assert_eq!(telemetry.health_percent, Some(80.0));
        assert_eq!(telemetry.energy_now_wh, Some(30.0));
        assert_eq!(telemetry.energy_full_wh, Some(60.0));
        assert_eq!(telemetry.energy_design_wh, Some(75.0));
        assert_eq!(telemetry.energy_rate_w, Some(15.0));
        assert_eq!(telemetry.state, ChargeState::Discharging);
    }

    #[test]
    fn reports_no_battery_when_no_present_cells_exist() {
        let telemetry = build_telemetry(
            RawDisplayDevice {
                level: 0,
                state: ChargeState::Unknown,
                time_to_empty_sec: None,
                time_to_full_sec: None,
            },
            vec![RawBatteryDevice {
                name: "BAT0".to_string(),
                present: false,
                level: 0,
                state: ChargeState::Unknown,
                health_percent: None,
                energy_rate_w: None,
                energy_now_wh: None,
                energy_full_wh: None,
                energy_design_wh: None,
            }],
            false,
        );

        assert_eq!(telemetry.availability, BatteryAvailability::NoBattery);
        assert!(!telemetry.present);
        assert_eq!(telemetry.batteries.len(), 1);
    }

    #[test]
    fn native_path_basename_wins_for_device_name() {
        fn ov_string(value: &str) -> OwnedValue {
            OwnedValue::try_from(Value::from(value)).expect("owned string value")
        }

        let mut props = HashMap::new();
        props.insert(
            "native-path".to_string(),
            ov_string("/sys/class/power_supply/BAT1"),
        );

        assert_eq!(
            device_name("/org/freedesktop/UPower/devices/battery_BAT1", &props),
            "BAT1"
        );
    }

    #[test]
    fn charge_state_maps_pending_values_to_not_charging() {
        assert_eq!(map_charge_state(5), ChargeState::NotCharging);
        assert_eq!(map_charge_state(6), ChargeState::NotCharging);
    }

    #[test]
    fn parse_battery_prefers_capacity_for_health_and_abs_rate() {
        let mut props = HashMap::new();
        props.insert("capacity".to_string(), OwnedValue::from(92.0_f64));
        props.insert("energy".to_string(), OwnedValue::from(48.0_f64));
        props.insert("energy-full".to_string(), OwnedValue::from(60.0_f64));
        props.insert("energy-full-design".to_string(), OwnedValue::from(66.0_f64));
        props.insert("energy-rate".to_string(), OwnedValue::from(-21.5_f64));
        props.insert("percentage".to_string(), OwnedValue::from(80.1_f64));
        props.insert("is-present".to_string(), OwnedValue::from(true));
        props.insert("state".to_string(), OwnedValue::from(1_u32));

        let parsed = parse_battery_device("/org/freedesktop/UPower/devices/battery_BAT0", &props);
        assert_eq!(parsed.health_percent, Some(92.0));
        assert_eq!(parsed.energy_rate_w, Some(21.5));
        assert_eq!(parsed.level, 80);
    }

    #[test]
    fn parse_display_device_treats_zero_times_as_unknown() {
        let mut props = HashMap::new();
        props.insert("percentage".to_string(), OwnedValue::from(87.0_f64));
        props.insert("state".to_string(), OwnedValue::from(2_u32));
        props.insert("time-to-empty".to_string(), OwnedValue::from(0_i64));
        props.insert("time-to-full".to_string(), OwnedValue::from(0_i64));

        let display = parse_display_device(&props).expect("display");
        assert_eq!(display.time_to_empty_sec, None);
        assert_eq!(display.time_to_full_sec, None);
    }
}
