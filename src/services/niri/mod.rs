// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! `niri` service — Niri compositor IPC.

mod app_names;

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, watch};
use tracing::{debug, info, warn};

use crate::bus::{ServiceError, ServiceHandle, ServiceRequest};
use crate::config::Config;
use crate::session_env;
use crate::util::json_map;
use app_names::AppNameResolver;

/// Spawn the `niri` service and return its [`ServiceHandle`].
pub fn spawn(cfg: &Config) -> ServiceHandle {
    let configured_socket = cfg
        .services
        .niri
        .as_ref()
        .and_then(|niri| niri.socket.clone())
        .filter(|socket| !socket.is_empty());
    let app_names = Arc::new(AppNameResolver::load());
    let initial = empty_snapshot();
    let (state_tx, state_rx) = watch::channel(initial);
    let (request_tx, request_rx) = mpsc::channel(16);

    tokio::spawn(run(request_rx, state_tx, configured_socket, app_names));

    ServiceHandle {
        request_tx,
        state_rx,
        events_tx: None,
    }
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
    configured_socket: Option<String>,
    app_names: Arc<AppNameResolver>,
) {
    info!("niri service started");
    loop {
        let socket = session_env::resolve_niri_socket(configured_socket.as_deref());
        debug!(
            path = %socket.path,
            source = socket.source,
            "resolved niri IPC socket candidate"
        );

        match connect_and_run(&mut request_rx, &state_tx, &socket.path, &app_names).await {
            Ok(()) => break,
            Err(e) => {
                warn!(
                    error = %e,
                    path = %socket.path,
                    source = socket.source,
                    "niri IPC connection failed; retrying in 5 s"
                );
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
    app_names: &AppNameResolver,
) -> Result<(), NiriError> {
    // Fetch initial state via separate one-shot connections
    let workspaces = niri_request(socket_path, r#""Workspaces""#).await?;
    let focused = niri_request(socket_path, r#""FocusedWindow""#).await?;
    let windows_json = niri_request(socket_path, r#""Windows""#).await?;

    let ws_list = parse_workspaces(&workspaces);
    let fw = parse_focused_window(&focused, app_names);
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
                // Let niri's event stream publish the resulting state. A one-shot query
                // immediately after an action can observe a transient focus state and
                // make the UI animate to the wrong workspace before the event stream
                // corrects it.
            }
            line = lines.next_line() => {
                match line {
                    Ok(Some(text)) => {
                        let refresh_focus =
                            process_event(&text, &mut ws_state, &mut fw_state, &mut win_map, app_names);
                        if refresh_focus {
                            if let Ok(fw_json) = niri_request(socket_path, r#""FocusedWindow""#).await {
                                fw_state = parse_focused_window(&fw_json, app_names);
                            }
                        }
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

fn parse_focused_window(json: &str, app_names: &AppNameResolver) -> Option<FocusedWindow> {
    let val: serde_json::Value = serde_json::from_str(json).ok()?;
    focused_window_from_value(val.get("Ok")?.get("FocusedWindow")?, app_names)
}

fn focused_window_from_value(
    value: &serde_json::Value,
    app_names: &AppNameResolver,
) -> Option<FocusedWindow> {
    let win = if let Some(obj) = value.as_object() {
        if obj.contains_key("id") {
            obj
        } else if let Some(nested) = obj.get("Window").and_then(|w| w.as_object()) {
            nested
        } else {
            obj.get("window").and_then(|w| w.as_object())?
        }
    } else {
        return None;
    };

    let app_id = win.get("app_id")?.as_str()?.to_string();
    Some(FocusedWindow {
        id: win.get("id")?.as_i64()?,
        display_name: app_names.resolve(&app_id),
        app_id,
        title: win.get("title")?.as_str()?.to_string(),
    })
}

fn process_event(
    text: &str,
    ws_state: &mut Vec<WorkspaceInfo>,
    fw_state: &mut Option<FocusedWindow>,
    win_map: &mut HashMap<i64, i64>,
    app_names: &AppNameResolver,
) -> bool {
    let val: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let mut refresh_focus = false;

    if let Some(wsc) = val.get("WorkspacesChanged") {
        if let Some(arr) = wsc.get("workspaces").and_then(|w| w.as_array()) {
            let mut list: Vec<WorkspaceInfo> =
                arr.iter().filter_map(parse_single_workspace).collect();
            list.sort_by(|a, b| a.output.cmp(&b.output).then_with(|| a.idx.cmp(&b.idx)));
            let focused: Vec<String> = list
                .iter()
                .filter(|ws| ws.focused)
                .map(|ws| format!("{}:{}", ws.output, ws.idx))
                .collect();
            debug!(?focused, "niri WorkspacesChanged");
            *ws_state = list;
        }
        refresh_focus = true;
    }

    if let Some(wfc) = val.get("WindowFocusChanged") {
        *fw_state = wfc.get("window").and_then(|w| {
            if w.is_null() {
                None
            } else {
                focused_window_from_value(w, app_names)
            }
        });
        refresh_focus = true;
    }

    // WorkspaceActivated carries the workspace's unique `id`, not its `idx`.
    // We store `id` in WorkspaceInfo so we can match correctly here.
    if let Some(wa) = val.get("WorkspaceActivated") {
        let ws_id = wa.get("id").and_then(|i| i.as_i64());
        let focused = wa.get("focused").and_then(|b| b.as_bool()).unwrap_or(false);
        debug!(?ws_id, focused, "niri WorkspaceActivated");
        if let Some(event_id) = ws_id.filter(|_| focused) {
            for ws in ws_state.iter_mut() {
                ws.focused = ws.id == event_id;
            }
        }
        refresh_focus = true;
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
        refresh_focus = true;
    }

    if let Some(evt) = val.get("WindowClosed") {
        if let Some(win_id) = evt.get("id").and_then(|i| i.as_i64()) {
            win_map.remove(&win_id);
        }
        refresh_focus = true;
    }

    refresh_focus
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
    display_name: String,
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
            ("display_name", Value::from(w.display_name.as_str())),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_activated_unfocused_does_not_clear_global_focus() {
        let app_names = AppNameResolver::load();
        let mut workspaces = vec![
            WorkspaceInfo {
                id: 1,
                idx: 1,
                name: None,
                output: "HDMI-A-3".to_string(),
                focused: true,
            },
            WorkspaceInfo {
                id: 2,
                idx: 2,
                name: None,
                output: "HDMI-A-3".to_string(),
                focused: false,
            },
        ];
        let mut focused_window = None;
        let mut win_map = HashMap::new();

        let refresh = process_event(
            r#"{"WorkspaceActivated":{"id":2,"focused":false}}"#,
            &mut workspaces,
            &mut focused_window,
            &mut win_map,
            &app_names,
        );

        assert!(refresh);
        assert!(workspaces[0].focused);
        assert!(!workspaces[1].focused);
    }

    #[test]
    fn workspace_activated_focused_switches_global_focus() {
        let app_names = AppNameResolver::load();
        let mut workspaces = vec![
            WorkspaceInfo {
                id: 1,
                idx: 1,
                name: None,
                output: "HDMI-A-3".to_string(),
                focused: true,
            },
            WorkspaceInfo {
                id: 2,
                idx: 2,
                name: None,
                output: "HDMI-A-3".to_string(),
                focused: false,
            },
        ];
        let mut focused_window = None;
        let mut win_map = HashMap::new();

        let refresh = process_event(
            r#"{"WorkspaceActivated":{"id":2,"focused":true}}"#,
            &mut workspaces,
            &mut focused_window,
            &mut win_map,
            &app_names,
        );

        assert!(refresh);
        assert!(!workspaces[0].focused);
        assert!(workspaces[1].focused);
    }
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
