// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Duration;

pub(super) const WEATHER_PROVIDER_OPEN_METEO: &str = "open-meteo";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct WeatherServicePolicy {
    pub(super) default_poll_sec: u64,
    pub(super) success_ttl_sec: i64,
    pub(super) fetch_timeout_sec: u64,
    pub(super) cache_version: u32,
}

impl WeatherServicePolicy {
    pub(super) fn fetch_timeout(self) -> Duration {
        Duration::from_secs(self.fetch_timeout_sec)
    }
}

pub(super) const WEATHER_SERVICE_POLICY: WeatherServicePolicy = WeatherServicePolicy {
    default_poll_sec: 600,
    success_ttl_sec: 1800,
    fetch_timeout_sec: 10,
    cache_version: 2,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct WeatherProviderRequestSpec {
    pub(super) name: &'static str,
    pub(super) base_url: &'static str,
    pub(super) current_fields: &'static str,
    pub(super) hourly_fields: &'static str,
    pub(super) forecast_days: &'static str,
    pub(super) timezone: &'static str,
}

impl WeatherProviderRequestSpec {
    pub(super) fn request_url(self, latitude: f64, longitude: f64) -> String {
        format!(
            "{base_url}?latitude={latitude}&longitude={longitude}&current={current_fields}&hourly={hourly_fields}&forecast_days={forecast_days}&timezone={timezone}",
            base_url = self.base_url,
            current_fields = self.current_fields,
            hourly_fields = self.hourly_fields,
            forecast_days = self.forecast_days,
            timezone = self.timezone,
        )
    }
}

pub(super) const OPEN_METEO_REQUEST_SPEC: WeatherProviderRequestSpec = WeatherProviderRequestSpec {
    name: WEATHER_PROVIDER_OPEN_METEO,
    base_url: "https://api.open-meteo.com/v1/forecast",
    current_fields:
        "temperature_2m,apparent_temperature,relative_humidity_2m,wind_speed_10m,weather_code",
    hourly_fields: "temperature_2m,weather_code",
    forecast_days: "1",
    timezone: "auto",
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct WeatherPresentation {
    pub(super) icon: &'static str,
    pub(super) description: &'static str,
}

const WMO_CLEAR_SKY: WeatherPresentation = WeatherPresentation {
    icon: "sun",
    description: "Clear sky",
};
const WMO_PARTLY_CLOUDY: WeatherPresentation = WeatherPresentation {
    icon: "cloud-sun",
    description: "Mainly clear / partly cloudy",
};
const WMO_FOG: WeatherPresentation = WeatherPresentation {
    icon: "cloud-fog",
    description: "Foggy",
};
const WMO_DRIZZLE: WeatherPresentation = WeatherPresentation {
    icon: "cloud-drizzle",
    description: "Drizzle",
};
const WMO_RAIN: WeatherPresentation = WeatherPresentation {
    icon: "cloud-rain",
    description: "Rain",
};
const WMO_SNOW: WeatherPresentation = WeatherPresentation {
    icon: "cloud-snow",
    description: "Snow",
};
const WMO_THUNDERSTORM: WeatherPresentation = WeatherPresentation {
    icon: "cloud-lightning",
    description: "Thunderstorm",
};
const WMO_UNKNOWN: WeatherPresentation = WeatherPresentation {
    icon: "cloud",
    description: "Unknown",
};

pub(super) fn wmo_presentation(code: i64) -> WeatherPresentation {
    match code {
        0 => WMO_CLEAR_SKY,
        1..=3 => WMO_PARTLY_CLOUDY,
        45 | 48 => WMO_FOG,
        51 | 53 | 55 | 56 | 57 => WMO_DRIZZLE,
        61 | 63 | 65 | 66 | 67 | 80..=82 => WMO_RAIN,
        71 | 73 | 75 | 77 | 85 | 86 => WMO_SNOW,
        95 | 96 | 99 => WMO_THUNDERSTORM,
        _ => WMO_UNKNOWN,
    }
}

#[cfg(test)]
mod tests {
    use super::{wmo_presentation, OPEN_METEO_REQUEST_SPEC, WMO_UNKNOWN};

    #[test]
    fn open_meteo_request_url_includes_expected_fields() {
        let url = OPEN_METEO_REQUEST_SPEC.request_url(35.0, 139.0);
        assert!(url.contains("latitude=35"));
        assert!(url.contains("longitude=139"));
        assert!(url.contains(OPEN_METEO_REQUEST_SPEC.current_fields));
        assert!(url.contains(OPEN_METEO_REQUEST_SPEC.hourly_fields));
        assert!(url.contains("forecast_days=1"));
    }

    #[test]
    fn unknown_wmo_code_uses_default_presentation() {
        assert_eq!(wmo_presentation(999), WMO_UNKNOWN);
    }
}
