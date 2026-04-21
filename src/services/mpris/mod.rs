// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! `mpris` service — media player control via D-Bus MPRIS2.

use serde_json::Value;
use tokio::sync::{mpsc, watch};
use tracing::{debug, info, warn};

use crate::bus::{ServiceError, ServiceHandle, ServiceRequest};
use crate::config::Config;
use crate::session_env;
use crate::util::json_map;

use futures::StreamExt;

/// Spawn the `mpris` service and return its [`ServiceHandle`].
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
        ("active_player", Value::Null),
        ("players", Value::Array(vec![])),
    ])
}

// ---------------------------------------------------------------------------
// Internal state
// ---------------------------------------------------------------------------

struct MprisState {
    players: Vec<PlayerInfo>,
    active_player: Option<String>,
}

struct PlayerInfo {
    bus_name: String,
    identity: String,
    playback_status: String,
    title: String,
    artist: Vec<String>,
    album: String,
    art_url: String,
    length_us: i64,
    position_us: i64,
}

// ---------------------------------------------------------------------------
// Main loop
// ---------------------------------------------------------------------------

async fn run(mut request_rx: mpsc::Receiver<ServiceRequest>, state_tx: watch::Sender<Value>) {
    info!("mpris service started");
    loop {
        match connect_and_run(&mut request_rx, &state_tx).await {
            Ok(()) => break,
            Err(e) => {
                warn!(error = %e, "mpris D-Bus connection failed; retrying in 5 s");
                state_tx.send_replace(unavailable_snapshot());
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    }
    info!("mpris service stopped");
}

async fn connect_and_run(
    request_rx: &mut mpsc::Receiver<ServiceRequest>,
    state_tx: &watch::Sender<Value>,
) -> Result<(), MprisError> {
    let mut last_err = None;
    let mut connected = None;
    for candidate in session_env::session_bus_candidates() {
        match zbus::connection::Builder::address(candidate.value.as_str()) {
            Ok(builder) => match builder.build().await {
                Ok(conn) => {
                    debug!(
                        source = candidate.source,
                        address = %candidate.value,
                        "connected mpris service to session bus"
                    );
                    connected = Some(conn);
                    break;
                }
                Err(err) => {
                    debug!(
                        source = candidate.source,
                        address = %candidate.value,
                        error = %err,
                        "failed mpris session bus candidate"
                    );
                    last_err = Some(err);
                }
            },
            Err(err) => {
                debug!(
                    source = candidate.source,
                    address = %candidate.value,
                    error = %err,
                    "failed to parse mpris session bus candidate"
                );
                last_err = Some(err);
            }
        }
    }

    let conn = match connected {
        Some(conn) => conn,
        None => {
            return Err(MprisError::from(last_err.unwrap_or_else(|| {
                zbus::Error::Failure("no usable session bus candidate found".to_string())
            })))
        }
    };

    let dbus_proxy = zbus::fdo::DBusProxy::new(&conn).await?;
    let mut state = scan_players(&conn, &dbus_proxy).await?;
    state_tx.send_replace(build_snapshot(&state));

    let mut name_changed = dbus_proxy.receive_name_owner_changed().await?;

    let mut poll = tokio::time::interval(std::time::Duration::from_secs(2));

    loop {
        tokio::select! {
            req = request_rx.recv() => {
                let Some(req) = req else { break };
                handle_request(req, &conn, &mut state).await;
                state_tx.send_replace(build_snapshot(&state));
            }
            signal = name_changed.next() => {
                if signal.is_none() { break; }
                if let Ok(new_state) = scan_players(&conn, &dbus_proxy).await {
                    state = new_state;
                    state_tx.send_replace(build_snapshot(&state));
                }
            }
            _ = poll.tick() => {
                // Refresh positions and statuses periodically
                if let Ok(new_state) = scan_players(&conn, &dbus_proxy).await {
                    // Preserve active_player selection
                    let active = state.active_player.clone();
                    state = new_state;
                    if active.is_some() {
                        state.active_player = active;
                    }
                    state_tx.send_replace(build_snapshot(&state));
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Player scanning
// ---------------------------------------------------------------------------

async fn scan_players(
    conn: &zbus::Connection,
    dbus_proxy: &zbus::fdo::DBusProxy<'_>,
) -> Result<MprisState, MprisError> {
    let names = dbus_proxy
        .list_names()
        .await
        .map_err(|e| MprisError::Dbus(e.to_string()))?;

    let mpris_names: Vec<String> = names
        .into_iter()
        .filter(|n| n.as_str().starts_with("org.mpris.MediaPlayer2."))
        .map(|n| n.to_string())
        .collect();

    let mut players = Vec::new();
    for bus_name in &mpris_names {
        match read_player(conn, bus_name).await {
            Ok(p) => players.push(p),
            Err(e) => debug!(bus = %bus_name, error = %e, "failed to read MPRIS player"),
        }
    }

    let active = pick_active(&players);
    Ok(MprisState {
        players,
        active_player: active,
    })
}

async fn read_player(conn: &zbus::Connection, bus_name: &str) -> Result<PlayerInfo, MprisError> {
    let mp2 = zbus::Proxy::new(
        conn,
        bus_name,
        "/org/mpris/MediaPlayer2",
        "org.mpris.MediaPlayer2",
    )
    .await?;

    let player = zbus::Proxy::new(
        conn,
        bus_name,
        "/org/mpris/MediaPlayer2",
        "org.mpris.MediaPlayer2.Player",
    )
    .await?;

    let identity: String = mp2
        .get_property("Identity")
        .await
        .unwrap_or_else(|_| bus_name.to_string());

    let playback_status: String = player
        .get_property("PlaybackStatus")
        .await
        .unwrap_or_else(|_| "Stopped".to_string());

    let position: i64 = player.get_property("Position").await.unwrap_or(0);

    let metadata = read_metadata(&player).await;

    Ok(PlayerInfo {
        bus_name: bus_name.to_string(),
        identity,
        playback_status,
        title: metadata.title,
        artist: metadata.artist,
        album: metadata.album,
        art_url: metadata.art_url,
        length_us: metadata.length_us,
        position_us: position,
    })
}

struct Metadata {
    title: String,
    artist: Vec<String>,
    album: String,
    art_url: String,
    track_id: Option<zbus::zvariant::OwnedObjectPath>,
    length_us: i64,
}

async fn read_metadata(player: &zbus::Proxy<'_>) -> Metadata {
    let raw: Result<std::collections::HashMap<String, zbus::zvariant::OwnedValue>, _> =
        player.get_property("Metadata").await;

    let map = match raw {
        Ok(m) => m,
        Err(_) => {
            return Metadata {
                title: String::new(),
                artist: vec![],
                album: String::new(),
                art_url: String::new(),
                track_id: None,
                length_us: 0,
            }
        }
    };

    let title = map
        .get("xesam:title")
        .and_then(|v| <&str>::try_from(v).ok())
        .map(|s| s.to_string())
        .unwrap_or_default();

    let artist = map
        .get("xesam:artist")
        .and_then(|v| {
            let arr: Result<Vec<String>, _> = v.clone().try_into();
            arr.ok()
        })
        .unwrap_or_default();

    let album = map
        .get("xesam:album")
        .and_then(|v| <&str>::try_from(v).ok())
        .map(|s| s.to_string())
        .unwrap_or_default();

    let art_url = map
        .get("mpris:artUrl")
        .and_then(|v| <&str>::try_from(v).ok())
        .map(|s| s.to_string())
        .unwrap_or_default();

    let track_id = map
        .get("mpris:trackid")
        .and_then(|v| zbus::zvariant::OwnedObjectPath::try_from(v.clone()).ok());

    let length_us = map
        .get("mpris:length")
        .and_then(|v| i64::try_from(v).ok())
        .unwrap_or(0);

    Metadata {
        title,
        artist,
        album,
        art_url,
        track_id,
        length_us,
    }
}

fn pick_active(players: &[PlayerInfo]) -> Option<String> {
    // Prefer playing player
    if let Some(p) = players.iter().find(|p| p.playback_status == "Playing") {
        return Some(p.bus_name.clone());
    }
    // Then paused
    if let Some(p) = players.iter().find(|p| p.playback_status == "Paused") {
        return Some(p.bus_name.clone());
    }
    // Then first
    players.first().map(|p| p.bus_name.clone())
}

// ---------------------------------------------------------------------------
// Snapshot
// ---------------------------------------------------------------------------

fn build_snapshot(state: &MprisState) -> Value {
    let active = match &state.active_player {
        Some(s) => Value::from(s.as_str()),
        None => Value::Null,
    };
    let players: Vec<Value> = state.players.iter().map(player_to_value).collect();
    json_map([
        ("active_player", active),
        ("players", Value::Array(players)),
    ])
}

fn player_to_value(p: &PlayerInfo) -> Value {
    let artists: Vec<Value> = p.artist.iter().map(|a| Value::from(a.as_str())).collect();
    json_map([
        ("bus_name", Value::from(p.bus_name.as_str())),
        ("identity", Value::from(p.identity.as_str())),
        ("playback_status", Value::from(p.playback_status.as_str())),
        ("title", Value::from(p.title.as_str())),
        ("artist", Value::Array(artists)),
        ("album", Value::from(p.album.as_str())),
        ("art_url", Value::from(p.art_url.as_str())),
        ("length_us", Value::from(p.length_us)),
        ("position_us", Value::from(p.position_us)),
    ])
}

// ---------------------------------------------------------------------------
// Request handling
// ---------------------------------------------------------------------------

async fn handle_request(req: ServiceRequest, conn: &zbus::Connection, state: &mut MprisState) {
    let result = match req.action.as_str() {
        "play_pause" => call_player_method(conn, &req.payload, state, "PlayPause").await,
        "next" => call_player_method(conn, &req.payload, state, "Next").await,
        "prev" => call_player_method(conn, &req.payload, state, "Previous").await,
        "stop" => call_player_method(conn, &req.payload, state, "Stop").await,
        "seek" => handle_seek(conn, &req.payload, state).await,
        "set_position" => handle_set_position(conn, &req.payload, state).await,
        "select_active" => handle_select_active(&req.payload, state),
        other => Err(ServiceError::ActionUnknown {
            action: other.to_string(),
        }),
    };
    req.reply.send(result).ok();
}

fn resolve_bus_name<'a>(
    payload: &'a Value,
    state: &'a MprisState,
) -> Result<&'a str, ServiceError> {
    if let Some(name) = extract_str(payload, "bus_name") {
        return Ok(name);
    }
    state
        .active_player
        .as_deref()
        .ok_or_else(|| ServiceError::ActionPayload {
            msg: "no active player".to_string(),
        })
}

async fn call_player_method(
    conn: &zbus::Connection,
    payload: &Value,
    state: &MprisState,
    method: &str,
) -> Result<Value, ServiceError> {
    let bus = resolve_bus_name(payload, state)?;
    let proxy = player_proxy(conn, bus).await?;
    let _: () = proxy
        .call(method, &())
        .await
        .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;
    Ok(Value::Null)
}

async fn handle_seek(
    conn: &zbus::Connection,
    payload: &Value,
    state: &MprisState,
) -> Result<Value, ServiceError> {
    let bus = resolve_bus_name(payload, state)?;
    let offset = extract_i64(payload, "offset_us").ok_or_else(|| ServiceError::ActionPayload {
        msg: "missing 'offset_us' field".to_string(),
    })?;
    let proxy = player_proxy(conn, bus).await?;
    let _: () = proxy
        .call("Seek", &(offset,))
        .await
        .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;
    Ok(Value::Null)
}

async fn handle_set_position(
    conn: &zbus::Connection,
    payload: &Value,
    state: &MprisState,
) -> Result<Value, ServiceError> {
    let bus = resolve_bus_name(payload, state)?;
    let pos = extract_i64(payload, "position_us").ok_or_else(|| ServiceError::ActionPayload {
        msg: "missing 'position_us' field".to_string(),
    })?;
    let proxy = player_proxy(conn, bus).await?;
    let metadata = read_metadata(&proxy).await;
    let track_id = metadata.track_id.ok_or_else(|| ServiceError::Internal {
        msg: format!("player {bus} did not expose mpris:trackid"),
    })?;
    let _: () = proxy
        .call("SetPosition", &(track_id, pos))
        .await
        .map_err(|e| ServiceError::Internal { msg: e.to_string() })?;
    Ok(Value::Null)
}

fn handle_select_active(payload: &Value, state: &mut MprisState) -> Result<Value, ServiceError> {
    let bus = extract_str(payload, "bus_name").ok_or_else(|| ServiceError::ActionPayload {
        msg: "missing 'bus_name' field".to_string(),
    })?;
    state.active_player = Some(bus.to_string());
    Ok(Value::Null)
}

async fn player_proxy<'a>(
    conn: &'a zbus::Connection,
    bus_name: &'a str,
) -> Result<zbus::Proxy<'a>, ServiceError> {
    zbus::Proxy::new(
        conn,
        bus_name,
        "/org/mpris/MediaPlayer2",
        "org.mpris.MediaPlayer2.Player",
    )
    .await
    .map_err(|e| ServiceError::Internal { msg: e.to_string() })
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

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
enum MprisError {
    #[error("zbus error: {0}")]
    Zbus(#[from] zbus::Error),
    #[error("D-Bus error: {0}")]
    Dbus(String),
}
