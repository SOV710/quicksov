// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! `weather` service — Open-Meteo HTTP backend.

use rmpv::Value;
use tokio::sync::{mpsc, watch};
use tracing::{debug, info, warn};

use crate::bus::{ServiceError, ServiceHandle, ServiceRequest};
use crate::config::Config;
use crate::util::{rmpv_map, unix_now_secs};

/// Spawn the `weather` service and return its [`ServiceHandle`].
pub fn spawn(cfg: &Config) -> ServiceHandle {
    let lat = cfg.services.weather.as_ref().and_then(|w| w.latitude);
    let lon = cfg.services.weather.as_ref().and_then(|w| w.longitude);
    let name = cfg
        .services
        .weather
        .as_ref()
        .and_then(|w| w.location_name.clone())
        .unwrap_or_default();
    let poll_sec = cfg
        .services
        .weather
        .as_ref()
        .and_then(|w| w.poll_interval_sec)
        .unwrap_or(600);

    let initial = offline_snapshot();
    let (state_tx, state_rx) = watch::channel(initial);
    let (request_tx, request_rx) = mpsc::channel(16);

    let wc = WeatherCfg {
        lat,
        lon,
        name,
        poll_sec,
    };
    tokio::spawn(run(request_rx, state_tx, wc));

    ServiceHandle {
        request_tx,
        state_rx,
        events_tx: None,
    }
}

struct WeatherCfg {
    lat: Option<f64>,
    lon: Option<f64>,
    name: String,
    poll_sec: u64,
}

fn offline_snapshot() -> Value {
    rmpv_map([
        ("location", Value::Nil),
        ("current", Value::Nil),
        ("hourly", Value::Array(vec![])),
        ("updated_at", Value::Nil),
        ("offline", Value::Boolean(true)),
    ])
}

// ---------------------------------------------------------------------------
// Main loop
// ---------------------------------------------------------------------------

async fn run(
    mut request_rx: mpsc::Receiver<ServiceRequest>,
    state_tx: watch::Sender<Value>,
    wc: WeatherCfg,
) {
    info!("weather service started");

    let (Some(lat), Some(lon)) = (wc.lat, wc.lon) else {
        warn!("weather: no lat/lon configured, staying offline");
        // Just handle requests
        while let Some(req) = request_rx.recv().await {
            handle_request_offline(req);
        }
        return;
    };

    // Try loading from cache first
    if let Some(cached) = load_cache() {
        let snap = parse_api_response(&cached, lat, lon, &wc.name);
        state_tx.send_replace(snap);
    }

    // Initial fetch
    match fetch_weather(lat, lon).await {
        Ok(body) => {
            save_cache(&body);
            let snap = parse_api_response(&body, lat, lon, &wc.name);
            state_tx.send_replace(snap);
        }
        Err(e) => warn!(error = %e, "weather: initial fetch failed"),
    }

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(wc.poll_sec));
    interval.tick().await; // consume the immediate tick

    loop {
        tokio::select! {
            req = request_rx.recv() => {
                let Some(req) = req else { break };
                let result = match req.action.as_str() {
                    "refresh" => {
                        match fetch_weather(lat, lon).await {
                            Ok(body) => {
                                save_cache(&body);
                                let snap = parse_api_response(&body, lat, lon, &wc.name);
                                state_tx.send_replace(snap);
                                Ok(Value::Nil)
                            }
                            Err(e) => Err(ServiceError::Internal { msg: e.to_string() }),
                        }
                    }
                    other => Err(ServiceError::ActionUnknown { action: other.to_string() }),
                };
                req.reply.send(result).ok();
            }
            _ = interval.tick() => {
                debug!("weather: periodic fetch");
                match fetch_weather(lat, lon).await {
                    Ok(body) => {
                        save_cache(&body);
                        let snap = parse_api_response(&body, lat, lon, &wc.name);
                        state_tx.send_replace(snap);
                    }
                    Err(e) => warn!(error = %e, "weather: periodic fetch failed"),
                }
            }
        }
    }
    info!("weather service stopped");
}

fn handle_request_offline(req: ServiceRequest) {
    let result = match req.action.as_str() {
        "refresh" => Err(ServiceError::Unavailable),
        other => Err(ServiceError::ActionUnknown {
            action: other.to_string(),
        }),
    };
    req.reply.send(result).ok();
}

// ---------------------------------------------------------------------------
// HTTP fetching
// ---------------------------------------------------------------------------

async fn fetch_weather(lat: f64, lon: f64) -> Result<String, WeatherError> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?\
         latitude={lat}&longitude={lon}\
         &current=temperature_2m,apparent_temperature,relative_humidity_2m,\
         wind_speed_10m,weather_code\
         &hourly=temperature_2m,weather_code\
         &forecast_days=1&timezone=auto"
    );
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| WeatherError::Http(e.to_string()))?;
    let body = resp
        .text()
        .await
        .map_err(|e| WeatherError::Http(e.to_string()))?;
    Ok(body)
}

// ---------------------------------------------------------------------------
// Response parsing
// ---------------------------------------------------------------------------

fn parse_api_response(json: &str, lat: f64, lon: f64, loc_name: &str) -> Value {
    let val: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return offline_snapshot(),
    };

    let location = rmpv_map([
        ("name", Value::from(loc_name)),
        ("latitude", Value::from(lat)),
        ("longitude", Value::from(lon)),
    ]);

    let current = parse_current(&val);
    let hourly = parse_hourly(&val);

    rmpv_map([
        ("location", location),
        ("current", current),
        ("hourly", Value::Array(hourly)),
        ("updated_at", Value::from(unix_now_secs())),
        ("offline", Value::Boolean(false)),
    ])
}

fn parse_current(val: &serde_json::Value) -> Value {
    let Some(cur) = val.get("current") else {
        return Value::Nil;
    };
    let temp = cur
        .get("temperature_2m")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let apparent = cur
        .get("apparent_temperature")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let humidity = cur
        .get("relative_humidity_2m")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let wind = cur
        .get("wind_speed_10m")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let wmo = cur
        .get("weather_code")
        .and_then(|v| v.as_i64())
        .unwrap_or(-1);
    let (icon, desc) = wmo_to_icon_desc(wmo);

    rmpv_map([
        ("temperature_c", Value::from(temp)),
        ("apparent_c", Value::from(apparent)),
        ("humidity_pct", Value::from(humidity)),
        ("wind_kmh", Value::from(wind)),
        ("wmo_code", Value::from(wmo)),
        ("icon", Value::from(icon)),
        ("description", Value::from(desc)),
    ])
}

fn parse_hourly(val: &serde_json::Value) -> Vec<Value> {
    let Some(hourly) = val.get("hourly") else {
        return vec![];
    };
    let times = hourly
        .get("time")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let temps = hourly
        .get("temperature_2m")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let codes = hourly
        .get("weather_code")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    times
        .iter()
        .zip(temps.iter())
        .zip(codes.iter())
        .map(|((t, temp), code)| {
            let time_str = t.as_str().unwrap_or("");
            let temp_f = temp.as_f64().unwrap_or(0.0);
            let wmo = code.as_i64().unwrap_or(-1);
            rmpv_map([
                ("time", Value::from(time_str)),
                ("temperature_c", Value::from(temp_f)),
                ("wmo_code", Value::from(wmo)),
            ])
        })
        .collect()
}

fn wmo_to_icon_desc(code: i64) -> (&'static str, &'static str) {
    match code {
        0 => ("sun", "Clear sky"),
        1..=3 => ("cloud-sun", "Mainly clear / partly cloudy"),
        45 | 48 => ("cloud-fog", "Foggy"),
        51 | 53 | 55 => ("cloud-drizzle", "Drizzle"),
        61 | 63 | 65 => ("cloud-rain", "Rain"),
        71 | 73 | 75 => ("cloud-snow", "Snow"),
        80..=82 => ("cloud-showers-heavy", "Rain showers"),
        95 => ("cloud-lightning", "Thunderstorm"),
        96 | 99 => ("cloud-lightning-rain", "Thunderstorm with hail"),
        _ => ("cloud", "Unknown"),
    }
}

// ---------------------------------------------------------------------------
// Cache
// ---------------------------------------------------------------------------

fn cache_path() -> Option<std::path::PathBuf> {
    let home = dirs::home_dir()?;
    Some(
        home.join(".cache")
            .join("quicksov")
            .join("weather")
            .join("current.json"),
    )
}

fn load_cache() -> Option<String> {
    let path = cache_path()?;
    let metadata = std::fs::metadata(&path).ok()?;
    let modified = metadata.modified().ok()?;
    let age = modified.elapsed().ok()?;
    // Only use cache if < 1 hour old
    if age.as_secs() > 3600 {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

fn save_cache(body: &str) {
    if let Some(path) = cache_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, body);
    }
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
enum WeatherError {
    #[error("HTTP error: {0}")]
    Http(String),
}
