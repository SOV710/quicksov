// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! `weather` service — scheduler + fetch-worker weather backend.

use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot, watch};
use tracing::{debug, info, warn};

use crate::bus::{ServiceError, ServiceHandle, ServiceRequest};
use crate::config::{paths, Config};
use crate::util::{is_empty_object, json_map, unix_now_secs};

const DEFAULT_POLL_SEC: u64 = 600;
const SUCCESS_TTL_SEC: i64 = 1800;
const FETCH_TIMEOUT_SEC: u64 = 10;
const CACHE_VERSION: u32 = 2;

/// Spawn the `weather` service and return its [`ServiceHandle`].
pub fn spawn(cfg: &Config) -> ServiceHandle {
    let weather_cfg = WeatherCfg::from_config(cfg);
    let initial = unavailable_snapshot(weather_cfg.provider_name(), weather_cfg.ttl_sec);
    let (state_tx, state_rx) = watch::channel(initial);
    let (request_tx, request_rx) = mpsc::channel(16);

    tokio::spawn(run(request_rx, state_tx, weather_cfg));

    ServiceHandle {
        request_tx,
        state_rx,
        events_tx: None,
    }
}

#[derive(Clone)]
struct WeatherCfg {
    provider: WeatherProviderCfg,
    latitude: Option<f64>,
    longitude: Option<f64>,
    location_name: String,
    poll_sec: u64,
    ttl_sec: i64,
}

impl WeatherCfg {
    fn from_config(cfg: &Config) -> Self {
        let weather = cfg.services.weather.as_ref();
        let backend = weather
            .and_then(|entry| entry.backend.as_deref())
            .unwrap_or("open-meteo");

        Self {
            provider: WeatherProviderCfg::from_backend(backend),
            latitude: weather.and_then(|entry| entry.latitude),
            longitude: weather.and_then(|entry| entry.longitude),
            location_name: weather
                .and_then(|entry| entry.location_name.clone())
                .unwrap_or_default(),
            poll_sec: weather
                .and_then(|entry| entry.poll_interval_sec)
                .unwrap_or(DEFAULT_POLL_SEC),
            ttl_sec: SUCCESS_TTL_SEC,
        }
    }

    fn provider_name(&self) -> &str {
        self.provider.name()
    }

    fn configured_location(&self) -> Option<WeatherLocation> {
        Some(WeatherLocation {
            name: self.location_name.clone(),
            latitude: self.latitude?,
            longitude: self.longitude?,
        })
    }

    fn validate(&self) -> Result<(WeatherProvider, f64, f64), WeatherFailure> {
        let provider = self.provider.build()?;
        let latitude = self
            .latitude
            .ok_or_else(WeatherFailure::missing_coordinates)?;
        let longitude = self
            .longitude
            .ok_or_else(WeatherFailure::missing_coordinates)?;
        Ok((provider, latitude, longitude))
    }
}

#[derive(Clone)]
enum WeatherProviderCfg {
    OpenMeteo,
    Unsupported(String),
}

impl WeatherProviderCfg {
    fn from_backend(name: &str) -> Self {
        match name {
            "open-meteo" => Self::OpenMeteo,
            other => Self::Unsupported(other.to_string()),
        }
    }

    fn name(&self) -> &str {
        match self {
            Self::OpenMeteo => "open-meteo",
            Self::Unsupported(name) => name.as_str(),
        }
    }

    fn build(&self) -> Result<WeatherProvider, WeatherFailure> {
        match self {
            Self::OpenMeteo => Ok(WeatherProvider::OpenMeteo(OpenMeteoProvider {
                client: Client::new(),
            })),
            Self::Unsupported(name) => Err(WeatherFailure::unsupported_provider(name)),
        }
    }
}

#[derive(Clone)]
enum WeatherProvider {
    OpenMeteo(OpenMeteoProvider),
}

impl WeatherProvider {
    fn name(&self) -> &'static str {
        match self {
            Self::OpenMeteo(_) => "open-meteo",
        }
    }

    async fn fetch(&self, job: &FetchJob) -> Result<WeatherSuccessData, WeatherFailure> {
        match self {
            Self::OpenMeteo(provider) => provider.fetch(job).await,
        }
    }
}

#[derive(Clone)]
struct OpenMeteoProvider {
    client: Client,
}

impl OpenMeteoProvider {
    async fn fetch(&self, job: &FetchJob) -> Result<WeatherSuccessData, WeatherFailure> {
        let url = format!(
            "https://api.open-meteo.com/v1/forecast?\
             latitude={lat}&longitude={lon}\
             &current=temperature_2m,apparent_temperature,relative_humidity_2m,\
             wind_speed_10m,weather_code\
             &hourly=temperature_2m,weather_code\
             &forecast_days=1&timezone=auto",
            lat = job.latitude,
            lon = job.longitude
        );

        let response = tokio::time::timeout(
            Duration::from_secs(FETCH_TIMEOUT_SEC),
            self.client.get(&url).send(),
        )
        .await
        .map_err(|_| WeatherFailure::timeout())?
        .map_err(WeatherFailure::http)?;

        let response = response.error_for_status().map_err(WeatherFailure::http)?;
        let payload = response
            .json::<OpenMeteoResponse>()
            .await
            .map_err(WeatherFailure::decode)?;

        let current = payload
            .current
            .ok_or_else(|| WeatherFailure::decode_message("missing current section"))?;
        let hourly = payload
            .hourly
            .ok_or_else(|| WeatherFailure::decode_message("missing hourly section"))?;

        let times = hourly.time;
        let temps = hourly.temperature_2m;
        let codes = hourly.weather_code;
        let count = times.len().min(temps.len()).min(codes.len());
        let hourly = (0..count)
            .map(|idx| WeatherHourlyPoint {
                time: times[idx].clone(),
                temperature_c: temps[idx],
                wmo_code: codes[idx],
            })
            .collect();

        let (icon, description) = wmo_to_icon_desc(current.weather_code);
        Ok(WeatherSuccessData {
            location: WeatherLocation {
                name: job.location_name.clone(),
                latitude: job.latitude,
                longitude: job.longitude,
            },
            current: WeatherCurrent {
                time: current.time,
                temperature_c: current.temperature_2m,
                apparent_c: current.apparent_temperature,
                humidity_pct: current.relative_humidity_2m,
                wind_kmh: current.wind_speed_10m,
                wmo_code: current.weather_code,
                icon: icon.to_string(),
                description: description.to_string(),
                timezone_abbreviation: payload.timezone_abbreviation.unwrap_or_default(),
            },
            hourly,
            last_success_at: unix_now_secs(),
        })
    }
}

#[derive(Clone)]
struct WeatherState {
    provider: String,
    ttl_sec: i64,
    configured_location: Option<WeatherLocation>,
    last_success: Option<WeatherSuccessData>,
    status: WeatherStatus,
    error: Option<WeatherErrorInfo>,
}

impl WeatherState {
    fn new(provider: &str, ttl_sec: i64, configured_location: Option<WeatherLocation>) -> Self {
        Self {
            provider: provider.to_string(),
            ttl_sec,
            configured_location,
            last_success: None,
            status: WeatherStatus::Loading,
            error: None,
        }
    }

    fn with_failure(
        provider: &str,
        ttl_sec: i64,
        configured_location: Option<WeatherLocation>,
        failure: WeatherFailure,
    ) -> Self {
        let mut state = Self::new(provider, ttl_sec, configured_location);
        state.apply_failure(failure);
        state
    }

    fn seed_success(&mut self, data: WeatherSuccessData) {
        self.last_success = Some(data);
        self.status = WeatherStatus::Ready;
        self.error = None;
    }

    fn mark_fetch_started(&mut self) {
        self.status = if self.last_success.is_some() {
            WeatherStatus::Refreshing
        } else {
            WeatherStatus::Loading
        };
        self.error = None;
    }

    fn apply_success(&mut self, data: WeatherSuccessData) {
        self.last_success = Some(data);
        self.status = WeatherStatus::Ready;
        self.error = None;
    }

    fn apply_failure(&mut self, failure: WeatherFailure) {
        self.status = if self.last_success.is_some() {
            WeatherStatus::RefreshFailed
        } else {
            WeatherStatus::InitFailed
        };
        self.error = Some(failure.into_error_info());
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WeatherStatus {
    Loading,
    Ready,
    Refreshing,
    InitFailed,
    RefreshFailed,
}

impl WeatherStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Loading => "loading",
            Self::Ready => "ready",
            Self::Refreshing => "refreshing",
            Self::InitFailed => "init_failed",
            Self::RefreshFailed => "refresh_failed",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FetchKind {
    Automatic,
    Manual,
}

#[derive(Clone)]
struct FetchJob {
    kind: FetchKind,
    latitude: f64,
    longitude: f64,
    location_name: String,
}

struct FetchOutcome {
    kind: FetchKind,
    result: Result<WeatherSuccessData, WeatherFailure>,
}

#[derive(Clone, Debug)]
struct WeatherFailure {
    kind: String,
    message: String,
}

impl WeatherFailure {
    fn missing_coordinates() -> Self {
        Self {
            kind: "config".to_string(),
            message: "weather service requires configured latitude and longitude".to_string(),
        }
    }

    fn unsupported_provider(name: &str) -> Self {
        Self {
            kind: "config".to_string(),
            message: format!("unsupported weather provider '{name}'"),
        }
    }

    fn timeout() -> Self {
        Self {
            kind: "timeout".to_string(),
            message: format!("weather fetch timed out after {FETCH_TIMEOUT_SEC}s"),
        }
    }

    fn http(err: reqwest::Error) -> Self {
        Self {
            kind: "http".to_string(),
            message: err.to_string(),
        }
    }

    fn decode(err: reqwest::Error) -> Self {
        Self {
            kind: "decode".to_string(),
            message: err.to_string(),
        }
    }

    fn decode_message(msg: &str) -> Self {
        Self {
            kind: "decode".to_string(),
            message: msg.to_string(),
        }
    }

    fn internal(msg: impl Into<String>) -> Self {
        Self {
            kind: "internal".to_string(),
            message: msg.into(),
        }
    }

    fn into_error_info(self) -> WeatherErrorInfo {
        WeatherErrorInfo {
            kind: self.kind,
            message: self.message,
            at: unix_now_secs(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct WeatherLocation {
    name: String,
    latitude: f64,
    longitude: f64,
}

#[derive(Clone, Serialize, Deserialize)]
struct WeatherCurrent {
    time: String,
    temperature_c: f64,
    apparent_c: f64,
    humidity_pct: i64,
    wind_kmh: f64,
    wmo_code: i64,
    icon: String,
    description: String,
    timezone_abbreviation: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct WeatherHourlyPoint {
    time: String,
    temperature_c: f64,
    wmo_code: i64,
}

#[derive(Clone, Serialize, Deserialize)]
struct WeatherSuccessData {
    location: WeatherLocation,
    current: WeatherCurrent,
    hourly: Vec<WeatherHourlyPoint>,
    last_success_at: i64,
}

#[derive(Clone, Serialize, Deserialize)]
struct WeatherErrorInfo {
    kind: String,
    message: String,
    at: i64,
}

#[derive(Serialize, Deserialize)]
struct CachedWeatherSnapshot {
    version: u32,
    provider: String,
    ttl_sec: i64,
    success: WeatherSuccessData,
}

#[derive(Deserialize)]
struct OpenMeteoResponse {
    current: Option<OpenMeteoCurrent>,
    hourly: Option<OpenMeteoHourly>,
    timezone_abbreviation: Option<String>,
}

#[derive(Deserialize)]
struct OpenMeteoCurrent {
    time: String,
    temperature_2m: f64,
    apparent_temperature: f64,
    relative_humidity_2m: i64,
    wind_speed_10m: f64,
    weather_code: i64,
}

#[derive(Deserialize)]
struct OpenMeteoHourly {
    time: Vec<String>,
    temperature_2m: Vec<f64>,
    weather_code: Vec<i64>,
}

fn unavailable_snapshot(provider: &str, ttl_sec: i64) -> Value {
    json_map([
        ("provider", Value::from(provider)),
        ("status", Value::from("loading")),
        ("ttl_sec", Value::from(ttl_sec)),
        ("location", Value::Null),
        ("current", Value::Null),
        ("hourly", Value::Array(vec![])),
        ("last_success_at", Value::Null),
        ("error", Value::Null),
    ])
}

// ---------------------------------------------------------------------------
// Main loop
// ---------------------------------------------------------------------------

async fn run(
    mut request_rx: mpsc::Receiver<ServiceRequest>,
    state_tx: watch::Sender<Value>,
    weather_cfg: WeatherCfg,
) {
    info!(provider = %weather_cfg.provider_name(), "weather service started");

    let configured_location = weather_cfg.configured_location();
    let mut state = WeatherState::new(
        weather_cfg.provider_name(),
        weather_cfg.ttl_sec,
        configured_location,
    );

    let (provider, latitude, longitude) = match weather_cfg.validate() {
        Ok(valid) => valid,
        Err(failure) => {
            state = WeatherState::with_failure(
                weather_cfg.provider_name(),
                weather_cfg.ttl_sec,
                state.configured_location.clone(),
                failure,
            );
            state_tx.send_replace(build_snapshot(&state));
            while let Some(req) = request_rx.recv().await {
                handle_request_unavailable(req);
            }
            info!("weather service stopped");
            return;
        }
    };

    if let Some(cached) = load_cache(&weather_cfg) {
        state.seed_success(cached);
    }

    let (fetch_tx, fetch_rx) = mpsc::channel(4);
    let (result_tx, mut result_rx) = mpsc::channel(4);
    tokio::spawn(run_fetch_worker(fetch_rx, result_tx, provider.clone()));

    let mut active_fetch: Option<FetchKind> = None;
    let mut manual_waiters: Vec<oneshot::Sender<Result<Value, ServiceError>>> = Vec::new();
    let mut poll_pending = false;
    let mut refresh_queued = false;

    if start_fetch(
        &fetch_tx,
        &weather_cfg,
        latitude,
        longitude,
        FetchKind::Automatic,
    )
    .await
    .is_ok()
    {
        active_fetch = Some(FetchKind::Automatic);
        state.mark_fetch_started();
    } else {
        state.apply_failure(WeatherFailure::internal(
            "weather fetch worker unavailable during startup",
        ));
    }
    state_tx.send_replace(build_snapshot(&state));

    let mut interval = tokio::time::interval(Duration::from_secs(weather_cfg.poll_sec));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    interval.tick().await;

    loop {
        tokio::select! {
            req = request_rx.recv() => {
                let Some(req) = req else { break };
                let mut ctx = RequestContext {
                    fetch_tx: &fetch_tx,
                    weather_cfg: &weather_cfg,
                    latitude,
                    longitude,
                    state: &mut state,
                    state_tx: &state_tx,
                    active_fetch: &mut active_fetch,
                    manual_waiters: &mut manual_waiters,
                    refresh_queued: &mut refresh_queued,
                };
                handle_request(req, &mut ctx).await;
            }
            result = result_rx.recv() => {
                let Some(result) = result else {
                    warn!("weather fetch worker stopped unexpectedly");
                    state.apply_failure(WeatherFailure::internal("weather fetch worker stopped"));
                    reply_manual_waiters(&mut manual_waiters, Err(ServiceError::Unavailable));
                    state_tx.send_replace(build_snapshot(&state));
                    break;
                };

                let completed_kind = active_fetch.take().unwrap_or(result.kind);
                match result.result {
                    Ok(success) => {
                        save_cache(&weather_cfg, &state.provider, state.ttl_sec, &success);
                        state.apply_success(success);
                        if completed_kind == FetchKind::Manual {
                            reply_manual_waiters(&mut manual_waiters, Ok(Value::Null));
                        }
                    }
                    Err(failure) => {
                        let service_error = ServiceError::Internal {
                            msg: failure.message.clone(),
                        };
                        state.apply_failure(failure);
                        if completed_kind == FetchKind::Manual {
                            reply_manual_waiters(&mut manual_waiters, Err(service_error));
                        }
                    }
                }
                state_tx.send_replace(build_snapshot(&state));

                if refresh_queued || !manual_waiters.is_empty() {
                    refresh_queued = false;
                    if start_fetch(
                        &fetch_tx,
                        &weather_cfg,
                        latitude,
                        longitude,
                        FetchKind::Manual,
                    )
                    .await
                    .is_ok()
                    {
                        active_fetch = Some(FetchKind::Manual);
                        state.mark_fetch_started();
                        state_tx.send_replace(build_snapshot(&state));
                    } else {
                        state.apply_failure(WeatherFailure::internal(
                            "failed to dispatch queued weather refresh",
                        ));
                        reply_manual_waiters(&mut manual_waiters, Err(ServiceError::Unavailable));
                        state_tx.send_replace(build_snapshot(&state));
                    }
                } else if poll_pending {
                    poll_pending = false;
                    if start_fetch(
                        &fetch_tx,
                        &weather_cfg,
                        latitude,
                        longitude,
                        FetchKind::Automatic,
                    )
                    .await
                    .is_ok()
                    {
                        active_fetch = Some(FetchKind::Automatic);
                        state.mark_fetch_started();
                        state_tx.send_replace(build_snapshot(&state));
                    } else {
                        state.apply_failure(WeatherFailure::internal(
                            "failed to dispatch scheduled weather refresh",
                        ));
                        state_tx.send_replace(build_snapshot(&state));
                    }
                }
            }
            _ = interval.tick() => {
                debug!("weather: scheduler tick");
                if active_fetch.is_none() {
                    if start_fetch(
                        &fetch_tx,
                        &weather_cfg,
                        latitude,
                        longitude,
                        FetchKind::Automatic,
                    )
                    .await
                    .is_ok()
                    {
                        active_fetch = Some(FetchKind::Automatic);
                        state.mark_fetch_started();
                        state_tx.send_replace(build_snapshot(&state));
                    } else {
                        state.apply_failure(WeatherFailure::internal(
                            "failed to dispatch scheduled weather refresh",
                        ));
                        state_tx.send_replace(build_snapshot(&state));
                    }
                } else {
                    poll_pending = true;
                }
            }
        }
    }

    info!("weather service stopped");
}

struct RequestContext<'a> {
    fetch_tx: &'a mpsc::Sender<FetchJob>,
    weather_cfg: &'a WeatherCfg,
    latitude: f64,
    longitude: f64,
    state: &'a mut WeatherState,
    state_tx: &'a watch::Sender<Value>,
    active_fetch: &'a mut Option<FetchKind>,
    manual_waiters: &'a mut Vec<oneshot::Sender<Result<Value, ServiceError>>>,
    refresh_queued: &'a mut bool,
}

async fn handle_request(req: ServiceRequest, ctx: &mut RequestContext<'_>) {
    match req.action.as_str() {
        "refresh" => {
            if !is_empty_object(&req.payload) {
                req.reply
                    .send(Err(ServiceError::ActionPayload {
                        msg: "refresh expects an empty object payload".to_string(),
                    }))
                    .ok();
                return;
            }

            ctx.manual_waiters.push(req.reply);
            match ctx.active_fetch {
                None => {
                    if start_fetch(
                        ctx.fetch_tx,
                        ctx.weather_cfg,
                        ctx.latitude,
                        ctx.longitude,
                        FetchKind::Manual,
                    )
                    .await
                    .is_ok()
                    {
                        *ctx.active_fetch = Some(FetchKind::Manual);
                        ctx.state.mark_fetch_started();
                        ctx.state_tx.send_replace(build_snapshot(ctx.state));
                    } else {
                        ctx.state.apply_failure(WeatherFailure::internal(
                            "failed to dispatch manual weather refresh",
                        ));
                        let error = Err(ServiceError::Unavailable);
                        reply_manual_waiters(ctx.manual_waiters, error);
                        ctx.state_tx.send_replace(build_snapshot(ctx.state));
                    }
                }
                Some(FetchKind::Manual) => {}
                Some(FetchKind::Automatic) => {
                    *ctx.refresh_queued = true;
                }
            }
        }
        other => {
            req.reply
                .send(Err(ServiceError::ActionUnknown {
                    action: other.to_string(),
                }))
                .ok();
        }
    }
}

fn handle_request_unavailable(req: ServiceRequest) {
    let result = match req.action.as_str() {
        "refresh" => Err(ServiceError::Unavailable),
        other => Err(ServiceError::ActionUnknown {
            action: other.to_string(),
        }),
    };
    req.reply.send(result).ok();
}

async fn start_fetch(
    fetch_tx: &mpsc::Sender<FetchJob>,
    weather_cfg: &WeatherCfg,
    latitude: f64,
    longitude: f64,
    kind: FetchKind,
) -> Result<(), ()> {
    fetch_tx
        .send(FetchJob {
            kind,
            latitude,
            longitude,
            location_name: weather_cfg.location_name.clone(),
        })
        .await
        .map_err(|_| ())
}

async fn run_fetch_worker(
    mut fetch_rx: mpsc::Receiver<FetchJob>,
    result_tx: mpsc::Sender<FetchOutcome>,
    provider: WeatherProvider,
) {
    info!(provider = %provider.name(), "weather fetch worker started");
    while let Some(job) = fetch_rx.recv().await {
        let result = provider.fetch(&job).await;
        if result_tx
            .send(FetchOutcome {
                kind: job.kind,
                result,
            })
            .await
            .is_err()
        {
            break;
        }
    }
    info!(provider = %provider.name(), "weather fetch worker stopped");
}

fn reply_manual_waiters(
    waiters: &mut Vec<oneshot::Sender<Result<Value, ServiceError>>>,
    result: Result<Value, ServiceError>,
) {
    for waiter in waiters.drain(..) {
        let payload = match &result {
            Ok(value) => Ok(value.clone()),
            Err(ServiceError::ActionUnknown { action }) => Err(ServiceError::ActionUnknown {
                action: action.clone(),
            }),
            Err(ServiceError::ActionPayload { msg }) => {
                Err(ServiceError::ActionPayload { msg: msg.clone() })
            }
            Err(ServiceError::Internal { msg }) => Err(ServiceError::Internal { msg: msg.clone() }),
            Err(ServiceError::Unavailable) => Err(ServiceError::Unavailable),
        };
        waiter.send(payload).ok();
    }
}

// ---------------------------------------------------------------------------
// Snapshot / cache
// ---------------------------------------------------------------------------

fn build_snapshot(state: &WeatherState) -> Value {
    let location = state
        .last_success
        .as_ref()
        .map(|success| &success.location)
        .or(state.configured_location.as_ref());
    let current = state.last_success.as_ref().map(|success| &success.current);
    let hourly = state
        .last_success
        .as_ref()
        .map(|success| success.hourly.clone())
        .unwrap_or_default();
    let last_success_at = state
        .last_success
        .as_ref()
        .map(|success| Value::from(success.last_success_at))
        .unwrap_or(Value::Null);

    json_map([
        ("provider", Value::from(state.provider.as_str())),
        ("status", Value::from(state.status.as_str())),
        ("ttl_sec", Value::from(state.ttl_sec)),
        ("location", location.map(json_value).unwrap_or(Value::Null)),
        ("current", current.map(json_value).unwrap_or(Value::Null)),
        ("hourly", json_value(&hourly)),
        ("last_success_at", last_success_at),
        (
            "error",
            state.error.as_ref().map(json_value).unwrap_or(Value::Null),
        ),
    ])
}

fn cache_path() -> Option<std::path::PathBuf> {
    paths::weather_cache_path()
}

fn load_cache(weather_cfg: &WeatherCfg) -> Option<WeatherSuccessData> {
    let path = cache_path()?;
    let body = std::fs::read_to_string(path).ok()?;
    let cached: CachedWeatherSnapshot = serde_json::from_str(&body).ok()?;
    if cached.version != CACHE_VERSION || cached.provider != weather_cfg.provider_name() {
        return None;
    }
    let configured_location = weather_cfg.configured_location()?;
    if configured_location.latitude != cached.success.location.latitude
        || configured_location.longitude != cached.success.location.longitude
        || configured_location.name != cached.success.location.name
    {
        return None;
    }
    if unix_now_secs() - cached.success.last_success_at > cached.ttl_sec {
        return None;
    }
    Some(cached.success)
}

fn save_cache(
    weather_cfg: &WeatherCfg,
    provider: &str,
    ttl_sec: i64,
    success: &WeatherSuccessData,
) {
    if let Some(path) = cache_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let cached = CachedWeatherSnapshot {
            version: CACHE_VERSION,
            provider: provider.to_string(),
            ttl_sec,
            success: success.clone(),
        };
        if let Ok(serialized) = serde_json::to_string(&cached) {
            let _ = std::fs::write(path, serialized);
        }
    } else {
        warn!(provider = %weather_cfg.provider_name(), "weather cache path unavailable");
    }
}

fn json_value<T>(value: &T) -> Value
where
    T: Serialize,
{
    serde_json::to_value(value).unwrap_or(Value::Null)
}

// ---------------------------------------------------------------------------
// WMO mapping
// ---------------------------------------------------------------------------

fn wmo_to_icon_desc(code: i64) -> (&'static str, &'static str) {
    match code {
        0 => ("sun", "Clear sky"),
        1..=3 => ("cloud-sun", "Mainly clear / partly cloudy"),
        45 | 48 => ("cloud-fog", "Foggy"),
        51 | 53 | 55 | 56 | 57 => ("cloud-drizzle", "Drizzle"),
        61 | 63 | 65 | 66 | 67 | 80..=82 => ("cloud-rain", "Rain"),
        71 | 73 | 75 | 77 | 85 | 86 => ("cloud-snow", "Snow"),
        95 | 96 | 99 => ("cloud-lightning", "Thunderstorm"),
        _ => ("cloud", "Unknown"),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::unavailable_snapshot;

    #[test]
    fn unavailable_snapshot_uses_configured_provider_and_ttl() {
        let snapshot = unavailable_snapshot("custom-provider", 42);
        assert_eq!(snapshot.get("provider"), Some(&Value::from("custom-provider")));
        assert_eq!(snapshot.get("ttl_sec"), Some(&Value::from(42)));
    }
}
