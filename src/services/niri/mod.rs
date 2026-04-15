// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! `niri` service — Niri compositor IPC.

use std::collections::HashMap;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, watch};
use tracing::{debug, info, warn};

use crate::bus::{ServiceError, ServiceHandle, ServiceRequest};
use crate::config::Config;
use crate::util::json_map;

/// Spawn the `niri` service and return its [`ServiceHandle`].
pub fn spawn(cfg: &Config) -> ServiceHandle {
    let socket_path = resolve_socket(cfg);
    let initial = empty_snapshot();
    let (state_tx, state_rx) = watch::channel(initial);
    let (request_tx, request_rx) = mpsc::channel(16);

    tokio::spawn(run(request_rx, state_tx, socket_path));

    ServiceHandle {
        request_tx,
        state_rx,
        events_tx: None,
    }
}

fn resolve_socket(cfg: &Config) -> String {
    if let Some(niri) = cfg.services.niri.as_ref() {
        if let Some(s) = niri.socket.as_deref() {
            if !s.is_empty() {
                return s.to_string();
            }
        }
    }
    if let Ok(s) = std::env::var("NIRI_SOCKET") {
        return s;
    }
    // Default guess
    let uid = nix::unistd::getuid();
    format!("/run/user/{uid}/niri/socket")
}

fn empty_snapshot() -> Value {
    json_map([
        ("workspaces", Value::Array(vec![])),
        ("focused_window", Value::Null),
    ])
}

// ---------------------------------------------------------------------------
// Main loop
// ---------------------------------------------------------------------------

async fn run(
    mut request_rx: mpsc::Receiver<ServiceRequest>,
    state_tx: watch::Sender<Value>,
    socket_path: String,
) {
    info!(path = %socket_path, "niri service started");
    loop {
        match connect_and_run(&mut request_rx, &state_tx, &socket_path).await {
            Ok(()) => break,
            Err(e) => {
                warn!(error = %e, "niri IPC connection failed; retrying in 5 s");
                state_tx.send_replace(empty_snapshot());
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    }
    info!("niri service stopped");
}

async fn connect_and_run(
    request_rx: &mut mpsc::Receiver<ServiceRequest>,
    state_tx: &watch::Sender<Value>,
    socket_path: &str,
) -> Result<(), NiriError> {
    // Fetch initial state via separate one-shot connections
    let workspaces = niri_request(socket_path, r#""Workspaces""#).await?;
    let focused = niri_request(socket_path, r#""FocusedWindow""#).await?;
    let windows_json = niri_request(socket_path, r#""Windows""#).await?;

    let ws_list = parse_workspaces(&workspaces);
    let fw = parse_focused_window(&focused);
    // window_id → workspace_id: used to count windows per workspace
    let mut win_map: HashMap<i64, i64> = parse_window_workspace_map(&windows_json);

    state_tx.send_replace(build_snapshot(&ws_list, &fw, &win_map));

    // Open event stream connection
    let stream = UnixStream::connect(socket_path)
        .await
        .map_err(|e| NiriError::Io(e.to_string()))?;
    let (reader, mut writer) = stream.into_split();

    writer
        .write_all(b"\"EventStream\"\n")
        .await
        .map_err(|e| NiriError::Io(e.to_string()))?;

    let mut lines = BufReader::new(reader).lines();

    let mut ws_state = ws_list;
    let mut fw_state = fw;

    loop {
        tokio::select! {
            req = request_rx.recv() => {
                let Some(req) = req else { break };
                handle_request(req, socket_path).await;
                // Refresh state after action
                if let Ok(ws_json) = niri_request(socket_path, r#""Workspaces""#).await {
                    ws_state = parse_workspaces(&ws_json);
                }
                if let Ok(fw_json) = niri_request(socket_path, r#""FocusedWindow""#).await {
                    fw_state = parse_focused_window(&fw_json);
                }
                state_tx.send_replace(build_snapshot(&ws_state, &fw_state, &win_map));
            }
            line = lines.next_line() => {
                match line {
                    Ok(Some(text)) => {
                        process_event(&text, &mut ws_state, &mut fw_state, &mut win_map);
                        state_tx.send_replace(build_snapshot(&ws_state, &fw_state, &win_map));
                    }
                    Ok(None) => break,
                    Err(e) => {
                        warn!(error = %e, "niri event stream read error");
                        break;
                    }
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Niri IPC communication
// ---------------------------------------------------------------------------

async fn niri_request(socket_path: &str, request: &str) -> Result<String, NiriError> {
    let stream = UnixStream::connect(socket_path)
        .await
        .map_err(|e| NiriError::Io(e.to_string()))?;
    let (reader, mut writer) = stream.into_split();

    let msg = format!("{request}\n");
    writer
        .write_all(msg.as_bytes())
        .await
        .map_err(|e| NiriError::Io(e.to_string()))?;

    let mut lines = BufReader::new(reader);
    let mut response = String::new();
    lines
        .read_line(&mut response)
        .await
        .map_err(|e| NiriError::Io(e.to_string()))?;
    Ok(response)
}

// ---------------------------------------------------------------------------
// JSON parsing
// ---------------------------------------------------------------------------

fn parse_workspaces(json: &str) -> Vec<WorkspaceInfo> {
    let val: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    // Response: {"Ok":{"Workspaces":[...]}}
    let ws_arr = val
        .get("Ok")
        .and_then(|ok| ok.get("Workspaces"))
        .and_then(|ws| ws.as_array());

    let Some(arr) = ws_arr else { return vec![] };
    let mut list: Vec<WorkspaceInfo> = arr.iter().filter_map(parse_single_workspace).collect();
    // Stable sort ensures consistent display order regardless of niri event ordering.
    list.sort_by(|a, b| a.output.cmp(&b.output).then_with(|| a.idx.cmp(&b.idx)));
    list
}

fn parse_single_workspace(v: &serde_json::Value) -> Option<WorkspaceInfo> {
    Some(WorkspaceInfo {
        // niri's unique workspace ID (used in WorkspaceActivated events)
        id: v.get("id").and_then(|i| i.as_i64()).unwrap_or(0),
        idx: v.get("idx")?.as_i64()? as i32,
        name: v.get("name").and_then(|n| n.as_str()).map(String::from),
        output: v.get("output")?.as_str()?.to_string(),
        focused: v
            .get("is_focused")
            .and_then(|b| b.as_bool())
            .unwrap_or(false),
    })
}

/// Parse the `Windows` response into a window_id → workspace_id map.
fn parse_window_workspace_map(json: &str) -> HashMap<i64, i64> {
    let val: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return HashMap::new(),
    };
    let arr = match val
        .get("Ok")
        .and_then(|ok| ok.get("Windows"))
        .and_then(|w| w.as_array())
    {
        Some(a) => a,
        None => return HashMap::new(),
    };
    let mut map = HashMap::new();
    for win in arr {
        if let (Some(win_id), Some(ws_id)) = (
            win.get("id").and_then(|i| i.as_i64()),
            win.get("workspace_id").and_then(|w| w.as_i64()),
        ) {
            map.insert(win_id, ws_id);
        }
    }
    map
}

fn parse_focused_window(json: &str) -> Option<FocusedWindow> {
    let val: serde_json::Value = serde_json::from_str(json).ok()?;
    let win = val.get("Ok")?.get("FocusedWindow")?.as_object()?;
    Some(FocusedWindow {
        id: win.get("id")?.as_i64()?,
        app_id: win.get("app_id")?.as_str()?.to_string(),
        title: win.get("title")?.as_str()?.to_string(),
    })
}

fn process_event(
    text: &str,
    ws_state: &mut Vec<WorkspaceInfo>,
    fw_state: &mut Option<FocusedWindow>,
    win_map: &mut HashMap<i64, i64>,
) {
    let val: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => return,
    };

    if let Some(wsc) = val.get("WorkspacesChanged") {
        if let Some(arr) = wsc.get("workspaces").and_then(|w| w.as_array()) {
            let mut list: Vec<WorkspaceInfo> =
                arr.iter().filter_map(parse_single_workspace).collect();
            list.sort_by(|a, b| a.output.cmp(&b.output).then_with(|| a.idx.cmp(&b.idx)));
            *ws_state = list;
        }
    }

    if let Some(wfc) = val.get("WindowFocusChanged") {
        *fw_state = wfc.get("window").and_then(|w| {
            if w.is_null() {
                None
            } else {
                let id = w.get("id")?.as_i64()?;
                let app_id = w.get("app_id")?.as_str()?.to_string();
                let title = w.get("title")?.as_str()?.to_string();
                Some(FocusedWindow { id, app_id, title })
            }
        });
    }

    // WorkspaceActivated carries the workspace's unique `id`, not its `idx`.
    // We store `id` in WorkspaceInfo so we can match correctly here.
    if let Some(wa) = val.get("WorkspaceActivated") {
        let ws_id = wa.get("id").and_then(|i| i.as_i64());
        let focused = wa.get("focused").and_then(|b| b.as_bool()).unwrap_or(false);
        if let Some(event_id) = ws_id {
            for ws in ws_state.iter_mut() {
                ws.focused = ws.id == event_id && focused;
            }
        }
    }

    // Track window ↔ workspace mapping for per-workspace window counts.
    if let Some(evt) = val.get("WindowOpenedOrChanged") {
        if let Some(win) = evt.get("window") {
            if let Some(win_id) = win.get("id").and_then(|i| i.as_i64()) {
                match win.get("workspace_id").and_then(|w| w.as_i64()) {
                    Some(ws_id) => {
                        win_map.insert(win_id, ws_id);
                    }
                    None => {
                        // Window is floating / not in a workspace — remove from map
                        win_map.remove(&win_id);
                    }
                }
            }
        }
    }

    if let Some(evt) = val.get("WindowClosed") {
        if let Some(win_id) = evt.get("id").and_then(|i| i.as_i64()) {
            win_map.remove(&win_id);
        }
    }
}

// ---------------------------------------------------------------------------
// State types
// ---------------------------------------------------------------------------

struct WorkspaceInfo {
    /// Niri's unique workspace ID — used in `WorkspaceActivated` events.
    id: i64,
    /// Sequential 1-based position on the output.
    idx: i32,
    name: Option<String>,
    output: String,
    focused: bool,
}

struct FocusedWindow {
    id: i64,
    app_id: String,
    title: String,
}

// ---------------------------------------------------------------------------
// Snapshot
// ---------------------------------------------------------------------------

fn build_snapshot(
    workspaces: &[WorkspaceInfo],
    focused: &Option<FocusedWindow>,
    win_map: &HashMap<i64, i64>,
) -> Value {
    // Count windows per workspace unique ID
    let mut ws_window_count: HashMap<i64, i64> = HashMap::new();
    for &ws_id in win_map.values() {
        *ws_window_count.entry(ws_id).or_insert(0) += 1;
    }

    let ws: Vec<Value> = workspaces
        .iter()
        .map(|ws| workspace_to_value(ws, ws_window_count.get(&ws.id).copied().unwrap_or(0)))
        .collect();
    let fw = match focused {
        Some(w) => json_map([
            ("id", Value::from(w.id)),
            ("app_id", Value::from(w.app_id.as_str())),
            ("title", Value::from(w.title.as_str())),
        ]),
        None => Value::Null,
    };
    json_map([("workspaces", Value::Array(ws)), ("focused_window", fw)])
}

fn workspace_to_value(ws: &WorkspaceInfo, window_count: i64) -> Value {
    let name = match &ws.name {
        Some(n) => Value::from(n.as_str()),
        None => Value::Null,
    };
    json_map([
        ("idx", Value::from(ws.idx as i64)),
        ("name", name),
        ("output", Value::from(ws.output.as_str())),
        ("focused", Value::Bool(ws.focused)),
        ("windows", Value::from(window_count)),
    ])
}

// ---------------------------------------------------------------------------
// Request handling
// ---------------------------------------------------------------------------

async fn handle_request(req: ServiceRequest, socket_path: &str) {
    let result = match req.action.as_str() {
        "focus_workspace" => handle_focus_workspace(&req.payload, socket_path).await,
        "run_action" => handle_run_action(&req.payload, socket_path).await,
        other => Err(ServiceError::ActionUnknown {
            action: other.to_string(),
        }),
    };
    req.reply.send(result).ok();
}

async fn handle_focus_workspace(payload: &Value, socket_path: &str) -> Result<Value, ServiceError> {
    let idx = extract_i64(payload, "idx").ok_or_else(|| ServiceError::ActionPayload {
        msg: "missing 'idx' field".to_string(),
    })?;
    let cmd = format!(r#"{{"Action":{{"FocusWorkspace":{{"reference":{{"Index":{idx}}}}}}}}}"#);
    niri_action(socket_path, &cmd).await
}

async fn handle_run_action(payload: &Value, socket_path: &str) -> Result<Value, ServiceError> {
    let action = extract_str(payload, "action").ok_or_else(|| ServiceError::ActionPayload {
        msg: "missing 'action' field".to_string(),
    })?;
    let args = extract_value(payload, "args");
    let args_json = args
        .and_then(|v| serde_json::to_string(v).ok())
        .unwrap_or_else(|| "null".to_string());
    let cmd = format!(r#"{{"Action":{{"{action}":{args_json}}}}}"#);
    niri_action(socket_path, &cmd).await
}

async fn niri_action(socket_path: &str, cmd: &str) -> Result<Value, ServiceError> {
    let resp = niri_request(socket_path, cmd)
        .await
        .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;
    debug!(response = %resp, "niri action response");

    // Niri replies with either {"Ok": ...} or {"Err": "message"}.
    let parsed: Value =
        serde_json::from_str(&resp).map_err(|e| ServiceError::Internal { msg: e.to_string() })?;

    if let Some(err_msg) = parsed.get("Err") {
        let msg = err_msg
            .as_str()
            .unwrap_or("niri returned an error")
            .to_string();
        return Err(ServiceError::Internal { msg });
    }

    Ok(Value::Null)
}

// ---------------------------------------------------------------------------
// Payload helpers
// ---------------------------------------------------------------------------

fn extract_str<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.as_object()?.get(key)?.as_str()
}

fn extract_i64(v: &Value, key: &str) -> Option<i64> {
    v.as_object()?.get(key)?.as_i64()
}

fn extract_value<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.as_object()?.get(key)
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
enum NiriError {
    #[error("I/O error: {0}")]
    Io(String),
}
