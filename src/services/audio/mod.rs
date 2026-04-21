// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! `audio` service — PipeWire + WirePlumber backend.
//!
//! Snapshot data is derived from `pw-dump` because it exposes accurate volume,
//! mute, and active stream metadata in a stable JSON form. Mutating actions use
//! `wpctl`, which is WirePlumber's supported control surface for PipeWire.

use std::process::{Command, Output};
use std::time::Duration;

use serde_json::Value;
use tokio::sync::{mpsc, watch};
use tokio::time::MissedTickBehavior;
use tracing::{info, warn};

use crate::bus::{ServiceError, ServiceHandle, ServiceRequest};
use crate::config::Config;
use crate::util::{json_map, prettify_label};

const DEFAULT_AUDIO_BACKEND: &str = "pipewire";
const AUDIO_POLL_INTERVAL_MS: u64 = 1000;
const MAX_VOLUME_PERCENT: u64 = 150;

/// Spawn the `audio` service and return its [`ServiceHandle`].
pub fn spawn(cfg: &Config) -> ServiceHandle {
    let initial = unavailable_snapshot();
    let (state_tx, state_rx) = watch::channel(initial);
    let (request_tx, request_rx) = mpsc::channel(16);
    let audio_cfg = AudioCfg::from_config(cfg);

    tokio::spawn(run(request_rx, state_tx, audio_cfg));

    ServiceHandle {
        request_tx,
        state_rx,
        events_tx: None,
    }
}

fn unavailable_snapshot() -> Value {
    json_map([
        ("default_sink", Value::from("")),
        ("default_source", Value::from("")),
        ("sinks", Value::Array(vec![])),
        ("sources", Value::Array(vec![])),
        ("streams", Value::Array(vec![])),
    ])
}

#[derive(Clone, Debug, Default, PartialEq)]
struct AudioSnapshot {
    default_sink: String,
    default_source: String,
    sinks: Vec<AudioNode>,
    sources: Vec<AudioNode>,
    streams: Vec<AudioStream>,
}

#[derive(Clone, Debug, PartialEq)]
struct AudioNode {
    id: u32,
    name: String,
    description: String,
    volume_pct: i64,
    muted: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct AudioStream {
    id: u32,
    app_name: String,
    binary: String,
    title: String,
    volume_pct: i64,
    muted: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct NodeState {
    volume: f64,
    muted: bool,
}

#[derive(Clone, Debug)]
struct AudioCfg {
    backend: String,
}

impl AudioCfg {
    fn from_config(cfg: &Config) -> Self {
        let configured = cfg
            .services
            .audio
            .as_ref()
            .and_then(|entry| entry.backend.as_deref())
            .unwrap_or(DEFAULT_AUDIO_BACKEND);

        let backend = if configured == DEFAULT_AUDIO_BACKEND {
            DEFAULT_AUDIO_BACKEND.to_string()
        } else {
            warn!(
                backend = %configured,
                fallback = DEFAULT_AUDIO_BACKEND,
                "unsupported audio backend configured; falling back to pipewire"
            );
            DEFAULT_AUDIO_BACKEND.to_string()
        };

        Self { backend }
    }
}

async fn run(
    mut request_rx: mpsc::Receiver<ServiceRequest>,
    state_tx: watch::Sender<Value>,
    audio_cfg: AudioCfg,
) {
    info!(backend = %audio_cfg.backend, "audio service started");

    let mut ticker = tokio::time::interval(Duration::from_millis(AUDIO_POLL_INTERVAL_MS));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut last_snapshot = unavailable_snapshot();
    let mut warned_refresh_failure = false;

    refresh_and_publish(&state_tx, &mut last_snapshot, &mut warned_refresh_failure).await;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                refresh_and_publish(
                    &state_tx,
                    &mut last_snapshot,
                    &mut warned_refresh_failure,
                ).await;
            }
            req = request_rx.recv() => {
                let Some(req) = req else { break };
                handle_request(req).await;
                refresh_and_publish(
                    &state_tx,
                    &mut last_snapshot,
                    &mut warned_refresh_failure,
                ).await;
            }
        }
    }

    info!("audio service stopped");
}

async fn refresh_and_publish(
    state_tx: &watch::Sender<Value>,
    last_snapshot: &mut Value,
    warned_refresh_failure: &mut bool,
) {
    match refresh_snapshot().await {
        Ok(snapshot) => {
            let next = snapshot_to_value(&snapshot);
            if next != *last_snapshot {
                state_tx.send_replace(next.clone());
                *last_snapshot = next;
            }
            *warned_refresh_failure = false;
        }
        Err(err) => {
            if !*warned_refresh_failure {
                warn!(error = %err, "failed to refresh audio snapshot");
                *warned_refresh_failure = true;
            }
        }
    }
}

fn snapshot_to_value(snapshot: &AudioSnapshot) -> Value {
    let sinks = snapshot.sinks.iter().map(node_to_value).collect();
    let sources = snapshot.sources.iter().map(node_to_value).collect();
    let streams = snapshot.streams.iter().map(stream_to_value).collect();

    json_map([
        ("default_sink", Value::from(snapshot.default_sink.as_str())),
        (
            "default_source",
            Value::from(snapshot.default_source.as_str()),
        ),
        ("sinks", Value::Array(sinks)),
        ("sources", Value::Array(sources)),
        ("streams", Value::Array(streams)),
    ])
}

fn node_to_value(node: &AudioNode) -> Value {
    json_map([
        ("id", Value::from(i64::from(node.id))),
        ("name", Value::from(node.name.as_str())),
        ("description", Value::from(node.description.as_str())),
        ("volume_pct", Value::from(node.volume_pct)),
        ("muted", Value::Bool(node.muted)),
    ])
}

fn stream_to_value(stream: &AudioStream) -> Value {
    json_map([
        ("id", Value::from(i64::from(stream.id))),
        ("app_name", Value::from(stream.app_name.as_str())),
        ("binary", Value::from(stream.binary.as_str())),
        ("title", Value::from(stream.title.as_str())),
        ("volume_pct", Value::from(stream.volume_pct)),
        ("muted", Value::Bool(stream.muted)),
    ])
}

async fn refresh_snapshot() -> Result<AudioSnapshot, AudioError> {
    tokio::task::spawn_blocking(refresh_snapshot_blocking)
        .await
        .map_err(|err| AudioError::Task(err.to_string()))?
}

fn refresh_snapshot_blocking() -> Result<AudioSnapshot, AudioError> {
    let output = run_command("pw-dump", &[])?;
    let objects: Vec<Value> =
        serde_json::from_slice(&output.stdout).map_err(|err| AudioError::Json(err.to_string()))?;

    let default_sink = find_metadata_name(&objects, "default.audio.sink")
        .or_else(|| find_metadata_name(&objects, "default.configured.audio.sink"))
        .unwrap_or_default();
    let default_source = find_metadata_name(&objects, "default.audio.source")
        .or_else(|| find_metadata_name(&objects, "default.configured.audio.source"))
        .unwrap_or_default();

    let mut sinks = collect_audio_nodes(&objects, "Audio/Sink");
    let mut sources = collect_audio_nodes(&objects, "Audio/Source");
    let mut streams = collect_audio_streams(&objects);

    sort_nodes(&mut sinks, &default_sink);
    sort_nodes(&mut sources, &default_source);
    streams.sort_by(|lhs, rhs| {
        lhs.app_name
            .to_ascii_lowercase()
            .cmp(&rhs.app_name.to_ascii_lowercase())
            .then_with(|| lhs.id.cmp(&rhs.id))
    });

    Ok(AudioSnapshot {
        default_sink,
        default_source,
        sinks,
        sources,
        streams,
    })
}

fn find_metadata_name(objects: &[Value], key: &str) -> Option<String> {
    for object in objects {
        let Some(entries) = object.get("metadata").and_then(Value::as_array) else {
            continue;
        };
        for entry in entries {
            if entry.get("key").and_then(Value::as_str) != Some(key) {
                continue;
            }

            let value = entry.get("value")?;
            if let Some(name) = value.get("name").and_then(Value::as_str) {
                return Some(name.to_string());
            }

            if let Some(text) = value.as_str() {
                let parsed: Value = serde_json::from_str(text).ok()?;
                if let Some(name) = parsed.get("name").and_then(Value::as_str) {
                    return Some(name.to_string());
                }
            }
        }
    }

    None
}

fn collect_audio_nodes(objects: &[Value], media_class: &str) -> Vec<AudioNode> {
    objects
        .iter()
        .filter_map(|object| parse_audio_node(object, media_class))
        .collect()
}

fn parse_audio_node(object: &Value, media_class: &str) -> Option<AudioNode> {
    let props = info_props(object)?;
    if string_prop(props, "media.class")? != media_class {
        return None;
    }

    let id = object_id(object)?;
    let name = string_prop(props, "node.name")?.to_string();
    let description = string_prop(props, "node.description")
        .or_else(|| string_prop(props, "node.nick"))
        .unwrap_or(name.as_str())
        .to_string();
    let state = node_state(object).unwrap_or_default();

    Some(AudioNode {
        id,
        name,
        description,
        volume_pct: volume_pct(state.volume),
        muted: state.muted,
    })
}

fn collect_audio_streams(objects: &[Value]) -> Vec<AudioStream> {
    objects.iter().filter_map(parse_audio_stream).collect()
}

fn parse_audio_stream(object: &Value) -> Option<AudioStream> {
    let props = info_props(object)?;
    if string_prop(props, "media.class")? != "Stream/Output/Audio" {
        return None;
    }

    let id = object_id(object)?;
    let binary = string_prop(props, "application.process.binary")
        .unwrap_or("")
        .to_string();
    let title = string_prop(props, "media.name").unwrap_or("").to_string();
    let state = node_state(object).unwrap_or_default();

    Some(AudioStream {
        id,
        app_name: preferred_stream_name(props),
        binary,
        title,
        volume_pct: volume_pct(state.volume),
        muted: state.muted,
    })
}

fn preferred_stream_name(props: &serde_json::Map<String, Value>) -> String {
    for text in [
        string_prop(props, "application.name"),
        string_prop(props, "application.process.binary"),
        string_prop(props, "node.description"),
        string_prop(props, "node.name"),
    ]
    .into_iter()
    .flatten()
    {
        let pretty = prettify_label(text);
        if !pretty.is_empty() {
            return pretty;
        }
    }

    "Unknown app".to_string()
}

fn sort_nodes(nodes: &mut [AudioNode], default_name: &str) {
    nodes.sort_by(|lhs, rhs| {
        let lhs_default = lhs.name == default_name;
        let rhs_default = rhs.name == default_name;
        rhs_default
            .cmp(&lhs_default)
            .then_with(|| {
                lhs.description
                    .to_ascii_lowercase()
                    .cmp(&rhs.description.to_ascii_lowercase())
            })
            .then_with(|| lhs.id.cmp(&rhs.id))
    });
}

fn info_props(object: &Value) -> Option<&serde_json::Map<String, Value>> {
    object.get("info")?.get("props")?.as_object()
}

fn string_prop<'a>(props: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a str> {
    props.get(key)?.as_str()
}

fn object_id(object: &Value) -> Option<u32> {
    object.get("id")?.as_u64().map(|id| id as u32)
}

fn node_state(object: &Value) -> Option<NodeState> {
    let props = object
        .get("info")?
        .get("params")?
        .get("Props")?
        .as_array()?
        .first()?
        .as_object()?;

    let volume = props
        .get("channelVolumes")
        .and_then(Value::as_array)
        .and_then(|values| max_channel_volume(values))
        .map(|channel| channel.cbrt())
        .or_else(|| props.get("volume").and_then(Value::as_f64))
        .unwrap_or(1.0);

    Some(NodeState {
        volume,
        muted: props.get("mute").and_then(Value::as_bool).unwrap_or(false),
    })
}

fn max_channel_volume(values: &[Value]) -> Option<f64> {
    values.iter().filter_map(Value::as_f64).reduce(f64::max)
}

fn volume_pct(volume: f64) -> i64 {
    (volume.clamp(0.0, max_volume_ratio()) * 100.0).round() as i64
}

async fn handle_request(req: ServiceRequest) {
    let result = match req.action.as_str() {
        "set_volume" => handle_set_volume(&req.payload).await,
        "set_mute" => handle_set_mute(&req.payload).await,
        "set_default_sink" => handle_set_default_sink(&req.payload).await,
        "set_stream_volume" => handle_set_stream_volume(&req.payload).await,
        other => Err(ServiceError::ActionUnknown {
            action: other.to_string(),
        }),
    };

    req.reply.send(result).ok();
}

async fn handle_set_volume(payload: &Value) -> Result<Value, ServiceError> {
    let sink_id = extract_u64(payload, "sink_id").ok_or_else(|| ServiceError::ActionPayload {
        msg: "missing 'sink_id' int field".to_string(),
    })? as u32;
    let volume_pct =
        extract_u64(payload, "volume_pct").ok_or_else(|| ServiceError::ActionPayload {
            msg: "missing 'volume_pct' int field".to_string(),
        })?;

    run_wpctl(vec![
        "set-volume".to_string(),
        sink_id.to_string(),
        format!("{:.2}", volume_ratio_from_pct(volume_pct)),
    ])
    .await?;

    Ok(Value::Null)
}

async fn handle_set_stream_volume(payload: &Value) -> Result<Value, ServiceError> {
    let stream_id =
        extract_u64(payload, "stream_id").ok_or_else(|| ServiceError::ActionPayload {
            msg: "missing 'stream_id' int field".to_string(),
        })? as u32;
    let volume_pct =
        extract_u64(payload, "volume_pct").ok_or_else(|| ServiceError::ActionPayload {
            msg: "missing 'volume_pct' int field".to_string(),
        })?;

    run_wpctl(vec![
        "set-volume".to_string(),
        stream_id.to_string(),
        format!("{:.2}", volume_ratio_from_pct(volume_pct)),
    ])
    .await?;

    Ok(Value::Null)
}

async fn handle_set_mute(payload: &Value) -> Result<Value, ServiceError> {
    let sink_id = extract_u64(payload, "sink_id").ok_or_else(|| ServiceError::ActionPayload {
        msg: "missing 'sink_id' int field".to_string(),
    })? as u32;
    let muted = extract_bool(payload, "muted").ok_or_else(|| ServiceError::ActionPayload {
        msg: "missing 'muted' bool field".to_string(),
    })?;

    run_wpctl(vec![
        "set-mute".to_string(),
        sink_id.to_string(),
        if muted { "1" } else { "0" }.to_string(),
    ])
    .await?;

    Ok(Value::Null)
}

async fn handle_set_default_sink(payload: &Value) -> Result<Value, ServiceError> {
    let sink_id = extract_u64(payload, "sink_id").ok_or_else(|| ServiceError::ActionPayload {
        msg: "missing 'sink_id' int field".to_string(),
    })? as u32;

    run_wpctl(vec!["set-default".to_string(), sink_id.to_string()]).await?;

    Ok(Value::Null)
}

async fn run_wpctl(args: Vec<String>) -> Result<(), ServiceError> {
    tokio::task::spawn_blocking(move || run_wpctl_blocking(&args))
        .await
        .map_err(|err| ServiceError::Internal {
            msg: format!("wpctl task failed: {err}"),
        })?
}

fn run_wpctl_blocking(args: &[String]) -> Result<(), ServiceError> {
    let output =
        Command::new("wpctl")
            .args(args)
            .output()
            .map_err(|err| ServiceError::Internal {
                msg: format!("failed to run wpctl: {err}"),
            })?;

    if output.status.success() {
        return Ok(());
    }

    Err(ServiceError::Internal {
        msg: format!(
            "wpctl {} failed: {}",
            args.join(" "),
            command_error_text(&output)
        ),
    })
}

fn run_command(program: &str, args: &[&str]) -> Result<Output, AudioError> {
    let output =
        Command::new(program)
            .args(args)
            .output()
            .map_err(|err| AudioError::CommandIo {
                program: program.to_string(),
                detail: err.to_string(),
            })?;

    if output.status.success() {
        return Ok(output);
    }

    Err(AudioError::CommandFailed {
        program: program.to_string(),
        stderr: command_error_text(&output),
    })
}

fn command_error_text(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        output.status.to_string()
    } else {
        stderr
    }
}

fn extract_u64(value: &Value, key: &str) -> Option<u64> {
    let value = value.get(key)?;
    value
        .as_u64()
        .or_else(|| value.as_i64().map(|int| int as u64))
}

fn extract_bool(value: &Value, key: &str) -> Option<bool> {
    value.as_object()?.get(key)?.as_bool()
}

fn max_volume_ratio() -> f64 {
    (MAX_VOLUME_PERCENT as f64) / 100.0
}

fn volume_ratio_from_pct(volume_pct: u64) -> f64 {
    (volume_pct.min(MAX_VOLUME_PERCENT) as f64) / 100.0
}

#[derive(Debug, thiserror::Error)]
enum AudioError {
    #[error("command `{program}` failed to start: {detail}")]
    CommandIo { program: String, detail: String },
    #[error("command `{program}` failed: {stderr}")]
    CommandFailed { program: String, stderr: String },
    #[error("invalid pw-dump json: {0}")]
    Json(String),
    #[error("blocking task failed: {0}")]
    Task(String),
}

#[cfg(test)]
mod tests {
    use crate::config::{AudioConfig, Config, ServicesConfig};

    use super::{volume_ratio_from_pct, AudioCfg, DEFAULT_AUDIO_BACKEND, MAX_VOLUME_PERCENT};

    #[test]
    fn unsupported_backend_falls_back_to_pipewire() {
        let cfg = Config {
            daemon: Default::default(),
            screens: Default::default(),
            power: Default::default(),
            services: ServicesConfig {
                enabled: Vec::new(),
                weather: None,
                wallpaper: None,
                network: None,
                audio: Some(AudioConfig {
                    backend: Some("alsa".to_string()),
                }),
                niri: None,
            },
        };

        let audio_cfg = AudioCfg::from_config(&cfg);
        assert_eq!(audio_cfg.backend, DEFAULT_AUDIO_BACKEND);
    }

    #[test]
    fn volume_ratio_clamps_to_maximum() {
        assert_eq!(
            volume_ratio_from_pct(MAX_VOLUME_PERCENT + 25),
            (MAX_VOLUME_PERCENT as f64) / 100.0
        );
    }
}
