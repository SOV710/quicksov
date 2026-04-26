// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs;
use std::path::Path;

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
struct BatterySample {
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

pub(crate) fn read_battery_telemetry(root: &Path) -> Result<BatteryTelemetry, std::io::Error> {
    let mut battery_samples = Vec::new();
    let mut external_online = false;

    for entry_result in fs::read_dir(root)? {
        let Ok(entry) = entry_result else {
            continue;
        };
        let path = entry.path();
        let Some(supply_type) = read_trimmed_optional(&path.join("type")) else {
            continue;
        };
        if supply_type == "Battery" {
            battery_samples.push(read_battery_sample(&path));
        } else if read_bool_optional(&path.join("online")).unwrap_or(false) {
            external_online = true;
        }
    }

    battery_samples.sort_by(|left, right| left.name.cmp(&right.name));

    let batteries = battery_samples
        .iter()
        .map(|sample| BatteryEntryState {
            name: sample.name.clone(),
            present: sample.present,
            level: sample.level,
            state: sample.state,
            health_percent: sample.health_percent,
            energy_rate_w: sample.energy_rate_w,
            energy_now_wh: sample.energy_now_wh,
            energy_full_wh: sample.energy_full_wh,
            energy_design_wh: sample.energy_design_wh,
        })
        .collect::<Vec<_>>();

    let present_samples = battery_samples
        .iter()
        .filter(|sample| sample.present)
        .collect::<Vec<_>>();

    if present_samples.is_empty() {
        return Ok(BatteryTelemetry {
            availability: BatteryAvailability::NoBattery,
            present: false,
            on_battery: false,
            level: 0,
            state: ChargeState::Unknown,
            health_percent: None,
            energy_rate_w: None,
            energy_now_wh: None,
            energy_full_wh: None,
            energy_design_wh: None,
            batteries,
        });
    }

    let energy_now_wh = sum_metric(present_samples.iter().map(|sample| sample.energy_now_wh));
    let energy_full_wh = sum_metric(present_samples.iter().map(|sample| sample.energy_full_wh));
    let energy_design_wh = sum_metric(present_samples.iter().map(|sample| sample.energy_design_wh));
    let energy_rate_w = sum_metric(present_samples.iter().map(|sample| sample.energy_rate_w));
    let level = percentage_from_totals(energy_now_wh, energy_full_wh).unwrap_or_else(|| {
        let total = present_samples
            .iter()
            .map(|sample| sample.level)
            .sum::<i64>();
        (total / present_samples.len() as i64).clamp(0, 100)
    });
    let health_percent = percentage_f64(energy_full_wh, energy_design_wh);

    Ok(BatteryTelemetry {
        availability: BatteryAvailability::Ready,
        present: true,
        on_battery: !external_online,
        level,
        state: aggregate_charge_state(&present_samples),
        health_percent,
        energy_rate_w,
        energy_now_wh,
        energy_full_wh,
        energy_design_wh,
        batteries,
    })
}

fn read_battery_sample(path: &Path) -> BatterySample {
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "BAT".to_string());
    let present = read_bool_optional(&path.join("present")).unwrap_or(true);
    let state = read_charge_state(path);
    let energy_now_wh = read_energy_wh(path, "energy_now", "charge_now");
    let energy_full_wh = read_energy_wh(path, "energy_full", "charge_full");
    let energy_design_wh = read_energy_wh(path, "energy_full_design", "charge_full_design");
    let health_percent = percentage_f64(energy_full_wh, energy_design_wh);
    let energy_rate_w = read_power_w(path);
    let level = read_capacity_percent(path)
        .or_else(|| percentage_from_totals(energy_now_wh, energy_full_wh))
        .unwrap_or(0);

    BatterySample {
        name,
        present,
        level,
        state,
        health_percent,
        energy_rate_w,
        energy_now_wh,
        energy_full_wh,
        energy_design_wh,
    }
}

fn aggregate_charge_state(samples: &[&BatterySample]) -> ChargeState {
    if samples
        .iter()
        .any(|sample| sample.state == ChargeState::Charging)
    {
        return ChargeState::Charging;
    }
    if samples
        .iter()
        .any(|sample| sample.state == ChargeState::Discharging)
    {
        return ChargeState::Discharging;
    }
    if samples
        .iter()
        .all(|sample| sample.state == ChargeState::FullyCharged)
    {
        return ChargeState::FullyCharged;
    }
    if samples
        .iter()
        .all(|sample| sample.state == ChargeState::Empty)
    {
        return ChargeState::Empty;
    }
    if samples
        .iter()
        .any(|sample| sample.state == ChargeState::NotCharging)
    {
        return ChargeState::NotCharging;
    }
    ChargeState::Unknown
}

fn read_charge_state(path: &Path) -> ChargeState {
    let Some(raw) = read_trimmed_optional(&path.join("status")) else {
        return ChargeState::Unknown;
    };
    match raw.as_str() {
        "Charging" => ChargeState::Charging,
        "Discharging" => ChargeState::Discharging,
        "Full" => ChargeState::FullyCharged,
        "Not charging" => ChargeState::NotCharging,
        "Empty" => ChargeState::Empty,
        _ => ChargeState::Unknown,
    }
}

fn read_energy_wh(path: &Path, energy_name: &str, charge_name: &str) -> Option<f64> {
    read_number_optional(&path.join(energy_name))
        .map(|raw| raw / 1_000_000.0)
        .or_else(|| {
            let charge = read_number_optional(&path.join(charge_name))?;
            let voltage = read_number_optional(&path.join("voltage_now"))
                .or_else(|| read_number_optional(&path.join("voltage_min_design")))?;
            finite_positive(charge * voltage / 1_000_000_000_000.0)
        })
}

fn read_power_w(path: &Path) -> Option<f64> {
    read_number_optional(&path.join("power_now"))
        .map(|raw| raw.abs() / 1_000_000.0)
        .or_else(|| {
            let current = read_number_optional(&path.join("current_now"))?;
            let voltage = read_number_optional(&path.join("voltage_now"))?;
            finite_positive((current * voltage).abs() / 1_000_000_000_000.0)
        })
}

fn read_capacity_percent(path: &Path) -> Option<i64> {
    read_number_optional(&path.join("capacity")).map(|value| value.round() as i64)
}

fn read_trimmed_optional(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
}

fn read_bool_optional(path: &Path) -> Option<bool> {
    match read_trimmed_optional(path)?.as_str() {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

fn read_number_optional(path: &Path) -> Option<f64> {
    read_trimmed_optional(path)?
        .parse::<f64>()
        .ok()
        .and_then(finite_positive)
}

fn finite_positive(value: f64) -> Option<f64> {
    (value.is_finite() && value >= 0.0).then_some(value)
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

fn percentage_f64(numerator: Option<f64>, denominator: Option<f64>) -> Option<f64> {
    match (numerator, denominator) {
        (Some(num), Some(den)) if den > 0.0 => Some((num / den * 100.0).clamp(0.0, 100.0)),
        _ => None,
    }
}

fn percentage_from_totals(numerator: Option<f64>, denominator: Option<f64>) -> Option<i64> {
    percentage_f64(numerator, denominator).map(|value| value.round() as i64)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{read_battery_telemetry, BatteryAvailability, ChargeState};

    struct TestDir {
        path: std::path::PathBuf,
    }

    impl TestDir {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "quicksov-battery-sysfs-{}-{}",
                std::process::id(),
                unique
            ));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }

        fn make_supply(&self, name: &str, files: &[(&str, &str)]) {
            let dir = self.path.join(name);
            fs::create_dir_all(&dir).expect("create supply dir");
            for (file_name, value) in files {
                fs::write(dir.join(file_name), value).expect("write supply file");
            }
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn parses_single_battery_with_external_power_state() {
        let dir = TestDir::new();
        dir.make_supply(
            "BAT0",
            &[
                ("type", "Battery"),
                ("present", "1"),
                ("status", "Charging"),
                ("energy_now", "30000000"),
                ("energy_full", "60000000"),
                ("energy_full_design", "70000000"),
                ("power_now", "15000000"),
                ("capacity", "50"),
            ],
        );
        dir.make_supply("AC", &[("type", "Mains"), ("online", "1")]);

        let telemetry = read_battery_telemetry(&dir.path).expect("read telemetry");
        assert_eq!(telemetry.availability, BatteryAvailability::Ready);
        assert!(telemetry.present);
        assert!(!telemetry.on_battery);
        assert_eq!(telemetry.level, 50);
        assert_eq!(telemetry.state, ChargeState::Charging);
        assert_eq!(telemetry.energy_now_wh, Some(30.0));
        assert_eq!(telemetry.energy_full_wh, Some(60.0));
        assert_eq!(telemetry.energy_rate_w, Some(15.0));
        assert_eq!(telemetry.batteries.len(), 1);
    }

    #[test]
    fn aggregates_multiple_batteries_by_energy_totals() {
        let dir = TestDir::new();
        dir.make_supply(
            "BAT0",
            &[
                ("type", "Battery"),
                ("present", "1"),
                ("status", "Discharging"),
                ("energy_now", "20000000"),
                ("energy_full", "40000000"),
                ("energy_full_design", "50000000"),
                ("power_now", "8000000"),
            ],
        );
        dir.make_supply(
            "BAT1",
            &[
                ("type", "Battery"),
                ("present", "1"),
                ("status", "Discharging"),
                ("energy_now", "10000000"),
                ("energy_full", "20000000"),
                ("energy_full_design", "25000000"),
                ("power_now", "5000000"),
            ],
        );

        let telemetry = read_battery_telemetry(&dir.path).expect("read telemetry");
        assert_eq!(telemetry.level, 50);
        assert_eq!(telemetry.health_percent, Some(80.0));
        assert_eq!(telemetry.energy_now_wh, Some(30.0));
        assert_eq!(telemetry.energy_full_wh, Some(60.0));
        assert_eq!(telemetry.energy_design_wh, Some(75.0));
        assert_eq!(telemetry.energy_rate_w, Some(13.0));
        assert_eq!(telemetry.state, ChargeState::Discharging);
        assert_eq!(telemetry.batteries.len(), 2);
    }

    #[test]
    fn reports_no_battery_when_no_present_cells_exist() {
        let dir = TestDir::new();
        dir.make_supply(
            "BAT0",
            &[("type", "Battery"), ("present", "0"), ("status", "Unknown")],
        );

        let telemetry = read_battery_telemetry(&dir.path).expect("read telemetry");
        assert_eq!(telemetry.availability, BatteryAvailability::NoBattery);
        assert!(!telemetry.present);
        assert_eq!(telemetry.batteries.len(), 1);
    }
}
