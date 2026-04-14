// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! `audio` service — PipeWire + WirePlumber backend.
//!
//! Uses the `pipewire` crate for node discovery via registry events,
//! bridged to tokio through channels.  Volume/mute control uses `wpctl`
//! because PipeWire SPA pod manipulation is prohibitively complex.

use serde_json::Value;
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};

use crate::bus::{ServiceError, ServiceHandle, ServiceRequest};
use crate::config::Config;
use crate::util::json_map;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Spawn the `audio` service and return its [`ServiceHandle`].
pub fn spawn(_cfg: &Config) -> ServiceHandle {
    let initial = unavailable_snapshot();
    let (state_tx, state_rx) = watch::channel(initial);
    let (request_tx, request_rx) = mpsc::channel(16);

    tokio::spawn(run(request_rx, state_tx));

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
    ])
}

// ---------------------------------------------------------------------------
// Snapshot types bridged from the PipeWire thread
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
struct AudioSnapshot {
    default_sink: String,
    default_source: String,
    sinks: Vec<AudioNode>,
    sources: Vec<AudioNode>,
}

#[derive(Clone, Debug)]
struct AudioNode {
    id: u32,
    name: String,
    description: String,
    volume_pct: i64,
    muted: bool,
}

#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
enum PwCommand {
    SetVolume { sink_id: u32, volume_pct: u32 },
    SetMute { sink_id: u32, muted: bool },
    SetDefaultSink { sink_id: u32 },
}

// ---------------------------------------------------------------------------
// Tokio task
// ---------------------------------------------------------------------------

async fn run(mut request_rx: mpsc::Receiver<ServiceRequest>, state_tx: watch::Sender<Value>) {
    info!("audio service started");

    // Channel from the PW thread to tokio
    let (snap_tx, mut snap_rx) = mpsc::channel::<AudioSnapshot>(16);
    // Channel from tokio to the PW thread
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<PwCommand>();

    // Spawn the PipeWire thread
    let pw_handle = std::thread::Builder::new()
        .name("audio-pw".into())
        .spawn(move || pw_thread(snap_tx, cmd_rx))
        .ok();

    if pw_handle.is_none() {
        warn!("failed to spawn PipeWire thread");
    }

    let mut current: Option<AudioSnapshot> = None;

    loop {
        tokio::select! {
            req = request_rx.recv() => {
                let Some(req) = req else { break };
                handle_request(req, &cmd_tx).await;
            }
            snap = snap_rx.recv() => {
                match snap {
                    Some(s) => {
                        current = Some(s.clone());
                        state_tx.send_replace(snapshot_to_value(&s));
                    }
                    None => break,
                }
            }
        }
    }
    drop(current);
    info!("audio service stopped");
}

fn snapshot_to_value(snap: &AudioSnapshot) -> Value {
    let sinks: Vec<Value> = snap.sinks.iter().map(node_to_value).collect();
    let sources: Vec<Value> = snap.sources.iter().map(node_to_value).collect();
    json_map([
        ("default_sink", Value::from(snap.default_sink.as_str())),
        ("default_source", Value::from(snap.default_source.as_str())),
        ("sinks", Value::Array(sinks)),
        ("sources", Value::Array(sources)),
    ])
}

fn node_to_value(n: &AudioNode) -> Value {
    json_map([
        ("id", Value::from(n.id as i64)),
        ("name", Value::from(n.name.as_str())),
        ("description", Value::from(n.description.as_str())),
        ("volume_pct", Value::from(n.volume_pct)),
        ("muted", Value::Bool(n.muted)),
    ])
}

// ---------------------------------------------------------------------------
// Request handling
// ---------------------------------------------------------------------------

async fn handle_request(req: ServiceRequest, cmd_tx: &std::sync::mpsc::Sender<PwCommand>) {
    let result = match req.action.as_str() {
        "set_volume" => handle_set_volume(&req.payload, cmd_tx),
        "set_mute" => handle_set_mute(&req.payload, cmd_tx),
        "set_default_sink" => handle_set_default_sink(&req.payload, cmd_tx),
        other => Err(ServiceError::ActionUnknown {
            action: other.to_string(),
        }),
    };
    req.reply.send(result).ok();
}

fn handle_set_volume(
    payload: &Value,
    cmd_tx: &std::sync::mpsc::Sender<PwCommand>,
) -> Result<Value, ServiceError> {
    let sink_id = extract_u64(payload, "sink_id").ok_or_else(|| ServiceError::ActionPayload {
        msg: "missing 'sink_id' int field".to_string(),
    })? as u32;
    let volume_pct =
        extract_u64(payload, "volume_pct").ok_or_else(|| ServiceError::ActionPayload {
            msg: "missing 'volume_pct' int field".to_string(),
        })? as u32;
    cmd_tx
        .send(PwCommand::SetVolume {
            sink_id,
            volume_pct,
        })
        .map_err(|_| ServiceError::Internal {
            msg: "PipeWire thread not running".to_string(),
        })?;
    Ok(Value::Null)
}

fn handle_set_mute(
    payload: &Value,
    cmd_tx: &std::sync::mpsc::Sender<PwCommand>,
) -> Result<Value, ServiceError> {
    let sink_id = extract_u64(payload, "sink_id").ok_or_else(|| ServiceError::ActionPayload {
        msg: "missing 'sink_id' int field".to_string(),
    })? as u32;
    let muted = extract_bool(payload, "muted").ok_or_else(|| ServiceError::ActionPayload {
        msg: "missing 'muted' bool field".to_string(),
    })?;
    cmd_tx
        .send(PwCommand::SetMute { sink_id, muted })
        .map_err(|_| ServiceError::Internal {
            msg: "PipeWire thread not running".to_string(),
        })?;
    Ok(Value::Null)
}

fn handle_set_default_sink(
    payload: &Value,
    cmd_tx: &std::sync::mpsc::Sender<PwCommand>,
) -> Result<Value, ServiceError> {
    let sink_id = extract_u64(payload, "sink_id").ok_or_else(|| ServiceError::ActionPayload {
        msg: "missing 'sink_id' int field".to_string(),
    })? as u32;
    cmd_tx
        .send(PwCommand::SetDefaultSink { sink_id })
        .map_err(|_| ServiceError::Internal {
            msg: "PipeWire thread not running".to_string(),
        })?;
    Ok(Value::Null)
}

// ---------------------------------------------------------------------------
// PipeWire thread
// ---------------------------------------------------------------------------

fn pw_thread(snap_tx: mpsc::Sender<AudioSnapshot>, cmd_rx: std::sync::mpsc::Receiver<PwCommand>) {
    if let Err(e) = pw_thread_inner(snap_tx, cmd_rx) {
        tracing::error!(error = %e, "PipeWire thread exited with error");
    }
}

fn pw_thread_inner(
    snap_tx: mpsc::Sender<AudioSnapshot>,
    cmd_rx: std::sync::mpsc::Receiver<PwCommand>,
) -> Result<(), AudioError> {
    pipewire::init();
    let mainloop = pipewire::main_loop::MainLoopBox::new(None)
        .map_err(|_| AudioError::Pw("failed to create MainLoop".into()))?;
    let context = pipewire::context::ContextBox::new(mainloop.loop_(), None)
        .map_err(|_| AudioError::Pw("failed to create Context".into()))?;
    let core = context
        .connect(None)
        .map_err(|_| AudioError::Pw("failed to connect to PipeWire".into()))?;
    let registry = core
        .get_registry()
        .map_err(|_| AudioError::Pw("no registry".into()))?;

    let state = std::rc::Rc::new(std::cell::RefCell::new(PwState::default()));

    // Registry listener for global objects
    let state_add = state.clone();
    let snap_tx_add = snap_tx.clone();
    let _listener = registry
        .add_listener_local()
        .global(move |global| {
            handle_global_add(&state_add, global);
            send_snapshot(&state_add, &snap_tx_add);
        })
        .global_remove(move |id| {
            state.borrow_mut().nodes.remove(&id);
            let _ = snap_tx.blocking_send(build_pw_snapshot(&state.borrow()));
        })
        .register();

    // Process commands from tokio via a timer callback
    let loop_ref = mainloop.loop_();
    let _timer = loop_ref.add_timer(move |_| {
        while let Ok(cmd) = cmd_rx.try_recv() {
            process_pw_command(&cmd);
        }
    });
    // Trigger timer periodically (100ms)
    _timer.update_timer(
        Some(std::time::Duration::from_millis(100)),
        Some(std::time::Duration::from_millis(100)),
    );

    mainloop.run();
    Ok(())
}

#[derive(Default)]
struct PwState {
    nodes: std::collections::HashMap<u32, PwNode>,
}

struct PwNode {
    id: u32,
    name: String,
    description: String,
    media_class: String,
}

fn handle_global_add(
    state: &std::rc::Rc<std::cell::RefCell<PwState>>,
    global: &pipewire::registry::GlobalObject<&pipewire::spa::utils::dict::DictRef>,
) {
    if global.type_ != pipewire::types::ObjectType::Node {
        return;
    }
    let Some(props) = global.props else { return };
    let media_class = props.get("media.class").unwrap_or("");
    if media_class != "Audio/Sink" && media_class != "Audio/Source" {
        return;
    }
    let node = PwNode {
        id: global.id,
        name: props.get("node.name").unwrap_or("").to_string(),
        description: props.get("node.description").unwrap_or("").to_string(),
        media_class: media_class.to_string(),
    };
    state.borrow_mut().nodes.insert(global.id, node);
}

fn send_snapshot(
    state: &std::rc::Rc<std::cell::RefCell<PwState>>,
    snap_tx: &mpsc::Sender<AudioSnapshot>,
) {
    let snap = build_pw_snapshot(&state.borrow());
    let _ = snap_tx.blocking_send(snap);
}

fn build_pw_snapshot(state: &PwState) -> AudioSnapshot {
    let mut sinks = Vec::new();
    let mut sources = Vec::new();
    let mut default_sink = String::new();
    let mut default_source = String::new();

    for node in state.nodes.values() {
        let audio_node = AudioNode {
            id: node.id,
            name: node.name.clone(),
            description: node.description.clone(),
            volume_pct: 100, // accurate reading requires SPA pod parsing
            muted: false,
        };
        if node.media_class == "Audio/Sink" {
            if default_sink.is_empty() {
                default_sink = node.name.clone();
            }
            sinks.push(audio_node);
        } else {
            if default_source.is_empty() {
                default_source = node.name.clone();
            }
            sources.push(audio_node);
        }
    }

    AudioSnapshot {
        default_sink,
        default_source,
        sinks,
        sources,
    }
}

fn process_pw_command(cmd: &PwCommand) {
    match cmd {
        PwCommand::SetVolume {
            sink_id,
            volume_pct,
        } => {
            let vol_f = *volume_pct as f64 / 100.0;
            let _ = std::process::Command::new("wpctl")
                .args(["set-volume", &format!("{sink_id}"), &format!("{vol_f:.2}")])
                .output();
        }
        PwCommand::SetMute { sink_id, muted } => {
            let val = if *muted { "1" } else { "0" };
            let _ = std::process::Command::new("wpctl")
                .args(["set-mute", &format!("{sink_id}"), val])
                .output();
        }
        PwCommand::SetDefaultSink { sink_id } => {
            let _ = std::process::Command::new("wpctl")
                .args(["set-default", &format!("{sink_id}")])
                .output();
        }
    }
}

// ---------------------------------------------------------------------------
// Payload helpers
// ---------------------------------------------------------------------------

fn extract_u64(v: &Value, key: &str) -> Option<u64> {
    let val = v.get(key)?;
    val.as_u64().or_else(|| val.as_i64().map(|i| i as u64))
}

fn extract_bool(v: &Value, key: &str) -> Option<bool> {
    v.as_object()?.get(key)?.as_bool()
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
enum AudioError {
    #[error("PipeWire error: {0}")]
    Pw(String),
}
