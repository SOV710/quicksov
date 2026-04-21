// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! `net.link` service — interface enumeration via rtnetlink.

use std::collections::HashMap;
use std::path::Path;

use futures::stream::TryStreamExt;
use futures::StreamExt;
use rtnetlink::packet_route::link::{
    InfoKind, LinkAttribute, LinkFlags, LinkInfo, LinkLayerType,
};
use serde_json::Value;
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};

use crate::bus::{ServiceError, ServiceHandle, ServiceRequest};
use crate::config::Config;
use crate::util::json_map;

/// Spawn the `net.link` service and return its [`ServiceHandle`].
pub fn spawn_link(_cfg: &Config) -> ServiceHandle {
    let initial = json_map([("interfaces", Value::Array(vec![]))]);
    let (state_tx, state_rx) = watch::channel(initial);
    let (request_tx, request_rx) = mpsc::channel(16);

    tokio::spawn(run(request_rx, state_tx));

    ServiceHandle {
        request_tx,
        state_rx,
        events_tx: None,
    }
}

// ---------------------------------------------------------------------------
// Internal state
// ---------------------------------------------------------------------------

#[derive(Default, Clone)]
struct IfaceInfo {
    name: String,
    kind: String,
    operstate: String,
    carrier: bool,
    mac: String,
    mtu: u32,
    ipv4: Vec<String>,
    ipv6: Vec<String>,
    #[allow(dead_code)]
    gateway: Option<String>,
    rx_bytes: u64,
    tx_bytes: u64,
}

// ---------------------------------------------------------------------------
// Main loop
// ---------------------------------------------------------------------------

async fn run(mut request_rx: mpsc::Receiver<ServiceRequest>, state_tx: watch::Sender<Value>) {
    info!("net.link service started");
    loop {
        match connect_and_run(&mut request_rx, &state_tx).await {
            Ok(()) => break,
            Err(e) => {
                warn!(error = %e, "net.link rtnetlink connection failed; retrying in 5 s");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    }
    info!("net.link service stopped");
}

async fn connect_and_run(
    request_rx: &mut mpsc::Receiver<ServiceRequest>,
    state_tx: &watch::Sender<Value>,
) -> Result<(), LinkError> {
    let (conn, handle, mut messages) =
        rtnetlink::new_connection().map_err(|e| LinkError::Io(e.to_string()))?;
    tokio::spawn(conn);

    let mut ifaces = initial_scan(&handle).await?;
    state_tx.send_replace(build_snapshot(&ifaces));

    loop {
        tokio::select! {
            req = request_rx.recv() => {
                let Some(req) = req else { break };
                // net.link has no actions
                req.reply.send(Err(ServiceError::ActionUnknown {
                    action: req.action.clone(),
                })).ok();
            }
            msg = messages.next() => {
                match msg {
                    Some((_change, _addr)) => {
                        // Re-scan on any netlink message
                        if let Ok(new_ifaces) = initial_scan(&handle).await {
                            ifaces = new_ifaces;
                            state_tx.send_replace(build_snapshot(&ifaces));
                        }
                    }
                    None => break,
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Scanning
// ---------------------------------------------------------------------------

async fn initial_scan(handle: &rtnetlink::Handle) -> Result<HashMap<u32, IfaceInfo>, LinkError> {
    let mut ifaces: HashMap<u32, IfaceInfo> = HashMap::new();

    // List links
    let mut links = handle.link().get().execute();
    while let Some(msg) = links
        .try_next()
        .await
        .map_err(|e| LinkError::Rtnetlink(e.to_string()))?
    {
        let idx = msg.header.index;
        let info = parse_link_msg(&msg);
        ifaces.insert(idx, info);
    }

    // List addresses
    let mut addrs = handle.address().get().execute();
    while let Some(msg) = addrs
        .try_next()
        .await
        .map_err(|e| LinkError::Rtnetlink(e.to_string()))?
    {
        let idx = msg.header.index;
        if let Some(iface) = ifaces.get_mut(&idx) {
            for attr in &msg.attributes {
                use rtnetlink::packet_route::address::AddressAttribute;
                if let AddressAttribute::Address(addr) = attr {
                    let s = format!("{addr}");
                    if msg.header.family == rtnetlink::packet_route::AddressFamily::Inet {
                        iface.ipv4.push(s);
                    } else if msg.header.family == rtnetlink::packet_route::AddressFamily::Inet6 {
                        iface.ipv6.push(s);
                    }
                }
            }
        }
    }

    Ok(ifaces)
}

fn parse_link_msg(msg: &rtnetlink::packet_route::link::LinkMessage) -> IfaceInfo {
    let mut info = IfaceInfo::default();
    let flags = msg.header.flags;
    let mut link_info_kind = None;
    info.carrier = flags.contains(LinkFlags::Running);

    for attr in &msg.attributes {
        match attr {
            LinkAttribute::IfName(name) => {
                info.name = name.clone();
            }
            LinkAttribute::Mtu(mtu) => info.mtu = *mtu,
            LinkAttribute::Address(mac_bytes) => {
                info.mac = format_mac(mac_bytes);
            }
            LinkAttribute::OperState(state) => {
                info.operstate = operstate_str(*state).to_string();
            }
            LinkAttribute::Stats64(stats) => {
                info.rx_bytes = stats.rx_bytes;
                info.tx_bytes = stats.tx_bytes;
            }
            LinkAttribute::LinkInfo(infos) => {
                link_info_kind = extract_link_info_kind(infos);
            }
            _ => {}
        }
    }

    if info.operstate.is_empty() {
        info.operstate = "unknown".to_string();
    }
    info.kind = classify_link_kind(
        is_wireless_interface(&info.name),
        msg.header.link_layer_type,
        link_info_kind.as_ref(),
    )
    .to_string();
    info
}

fn extract_link_info_kind(infos: &[LinkInfo]) -> Option<InfoKind> {
    infos.iter().find_map(|info| match info {
        LinkInfo::Kind(kind) => Some(kind.clone()),
        _ => None,
    })
}

fn classify_link_kind(
    is_wireless: bool,
    layer_type: LinkLayerType,
    info_kind: Option<&InfoKind>,
) -> &'static str {
    if matches!(layer_type, LinkLayerType::Loopback) {
        return "loopback";
    }

    if is_wireless
        || matches!(
            layer_type,
            LinkLayerType::Ieee80211
                | LinkLayerType::Ieee80211Prism
                | LinkLayerType::Ieee80211Radiotap
        )
    {
        return "wifi";
    }

    if info_kind.is_some() {
        return "other";
    }

    if matches!(
        layer_type,
        LinkLayerType::Ether | LinkLayerType::Eether | LinkLayerType::Ieee802
    ) {
        return "ethernet";
    }

    "other"
}

fn is_wireless_interface(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    let base = Path::new("/sys/class/net").join(name);
    base.join("wireless").exists() || base.join("phy80211").exists()
}

fn operstate_str(state: rtnetlink::packet_route::link::State) -> &'static str {
    use rtnetlink::packet_route::link::State;
    match state {
        State::Up => "up",
        State::Down => "down",
        State::Unknown => "unknown",
        State::NotPresent => "notpresent",
        State::LowerLayerDown => "lowerlayerdown",
        State::Testing => "testing",
        State::Dormant => "dormant",
        _ => "unknown",
    }
}

fn format_mac(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

// ---------------------------------------------------------------------------
// Snapshot builder
// ---------------------------------------------------------------------------

fn build_snapshot(ifaces: &HashMap<u32, IfaceInfo>) -> Value {
    let mut entries: Vec<Value> = ifaces.values().map(iface_to_value).collect();
    entries.sort_by(|a, b| {
        let name_a = extract_name(a);
        let name_b = extract_name(b);
        name_a.cmp(&name_b)
    });
    json_map([("interfaces", Value::Array(entries))])
}

fn iface_to_value(i: &IfaceInfo) -> Value {
    json_map([
        ("name", Value::from(i.name.as_str())),
        ("kind", Value::from(i.kind.as_str())),
        ("operstate", Value::from(i.operstate.as_str())),
        ("carrier", Value::Bool(i.carrier)),
        ("mac", Value::from(i.mac.as_str())),
        ("mtu", Value::from(i.mtu as i64)),
        (
            "ipv4",
            Value::Array(i.ipv4.iter().map(|s| Value::from(s.as_str())).collect()),
        ),
        (
            "ipv6",
            Value::Array(i.ipv6.iter().map(|s| Value::from(s.as_str())).collect()),
        ),
        ("gateway", Value::Null),
        ("rx_bytes", Value::from(i.rx_bytes as i64)),
        ("tx_bytes", Value::from(i.tx_bytes as i64)),
    ])
}

fn extract_name(v: &Value) -> String {
    v.as_object()
        .and_then(|m| m.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
enum LinkError {
    #[error("I/O error: {0}")]
    Io(String),
    #[error("rtnetlink error: {0}")]
    Rtnetlink(String),
}

#[cfg(test)]
mod tests {
    use rtnetlink::packet_route::link::{InfoKind, LinkLayerType};

    use super::classify_link_kind;

    #[test]
    fn loopback_uses_link_layer_type() {
        assert_eq!(
            classify_link_kind(false, LinkLayerType::Loopback, None),
            "loopback"
        );
    }

    #[test]
    fn wireless_uses_capability_not_name() {
        assert_eq!(classify_link_kind(true, LinkLayerType::Ether, None), "wifi");
    }

    #[test]
    fn plain_ethernet_without_link_info_stays_ethernet() {
        assert_eq!(
            classify_link_kind(false, LinkLayerType::Ether, None),
            "ethernet"
        );
    }

    #[test]
    fn virtual_ethernet_kinds_are_not_misclassified_as_physical_ethernet() {
        assert_eq!(
            classify_link_kind(false, LinkLayerType::Ether, Some(&InfoKind::Veth)),
            "other"
        );
    }
}
