#!/usr/bin/env python3
from __future__ import annotations

import argparse
import socket
import time

from _qsov_testlib import (
    ERR,
    PUB,
    REP,
    Harness,
    add_common_args,
    assert_dict_keys,
    choose_socket,
    connect_and_hello,
    expect_envelope,
    expect_rep_or_warn_service_err,
    main_guard,
    maybe_warn_unavailable,
)


REQUIRED = [
    "interface",
    "state",
    "connection_state",
    "scan_state",
    "scan_started_at",
    "scan_finished_at",
    "scan_last_error",
    "manual_connect_state",
    "manual_connect_ssid",
    "manual_connect_reason",
    "manual_connect_started_at",
    "present",
    "enabled",
    "availability",
    "availability_reason",
    "interface_operstate",
    "rfkill_available",
    "rfkill_soft_blocked",
    "rfkill_hard_blocked",
    "airplane_mode",
    "network_id",
    "ssid",
    "bssid",
    "rssi_dbm",
    "signal_pct",
    "frequency",
    "saved_networks",
    "scan_results",
]

CONNECTION_STATES = {"disconnected", "associating", "connected", "unknown"}
SCAN_STATES = {"idle", "starting", "running"}
MANUAL_CONNECT_STATES = {"idle", "connecting", "failed"}
MANUAL_CONNECT_REASONS = {"none", "auth_failed", "timeout", "backend_error"}
LEGACY_STATES = {"disconnected", "scanning", "associating", "connected", "unknown"}


def _is_optional_int(value: object) -> bool:
    return value is None or type(value) is int


def _is_optional_str(value: object) -> bool:
    return value is None or isinstance(value, str)


def _recv_until_reply(client, msg_id: int, timeout: float) -> tuple[dict | None, list[dict]]:
    sock = client.sock
    if sock is None:
        raise RuntimeError("socket is not connected")

    original_timeout = sock.gettimeout()
    reply = None
    pubs: list[dict] = []
    deadline = time.monotonic() + timeout
    try:
        while reply is None and time.monotonic() < deadline:
            remaining = max(deadline - time.monotonic(), 0.05)
            sock.settimeout(remaining)
            try:
                msg = client.recv_obj()
            except (socket.timeout, TimeoutError):
                break
            if not isinstance(msg, dict):
                continue
            if msg.get("id") == msg_id:
                reply = msg
            elif msg.get("kind") == PUB and msg.get("topic") == "net.wifi":
                pubs.append(msg)
    finally:
        sock.settimeout(original_timeout)

    return reply, pubs


def _collect_wifi_pubs(client, timeout: float, *, stop_on_idle: bool = False) -> list[dict]:
    sock = client.sock
    if sock is None:
        raise RuntimeError("socket is not connected")

    original_timeout = sock.gettimeout()
    pubs: list[dict] = []
    deadline = time.monotonic() + timeout
    try:
        while time.monotonic() < deadline:
            remaining = max(deadline - time.monotonic(), 0.05)
            sock.settimeout(remaining)
            try:
                msg = client.recv_obj()
            except (socket.timeout, TimeoutError):
                break
            if not isinstance(msg, dict):
                continue
            if msg.get("kind") != PUB or msg.get("topic") != "net.wifi":
                continue
            pubs.append(msg)
            payload = msg.get("payload")
            if stop_on_idle and isinstance(payload, dict) and payload.get("scan_state") == "idle":
                break
    finally:
        sock.settimeout(original_timeout)

    return pubs


def _pub_payloads(messages: list[dict]) -> list[dict]:
    payloads: list[dict] = []
    for msg in messages:
        payload = msg.get("payload")
        if isinstance(payload, dict):
            payloads.append(payload)
    return payloads


def run() -> int:
    parser = argparse.ArgumentParser(description="Manual tests for qsov net.wifi service")
    add_common_args(parser)
    parser.add_argument("--ssid", default=None, help="SSID for --mutate connect test")
    parser.add_argument("--psk", default=None, help="PSK for --mutate connect test")
    parser.add_argument("--save", action="store_true", help="use save=true for --mutate connect")
    parser.add_argument("--forget-ssid", default=None, help="SSID for --mutate forget test")
    parser.add_argument(
        "--mutate-rfkill",
        action="store_true",
        help="run rfkill set_enabled / airplane_mode actions (dangerous: changes local radio state)",
    )
    args = parser.parse_args()

    h = Harness("net.wifi", strict=args.strict)
    socket_path = choose_socket(args)
    client, _ack = connect_and_hello(h, socket_path, args.timeout, "net.wifi")
    try:
        sub = client.sub("net.wifi")
        env = expect_envelope(h, sub, kind=PUB, topic="net.wifi")
        snapshot = None
        if env and assert_dict_keys(h, env.get("payload"), REQUIRED, "net.wifi snapshot"):
            snapshot = env["payload"]
            maybe_warn_unavailable(h, "net.wifi", snapshot)
            if snapshot.get("state") in LEGACY_STATES:
                h.ok("net.wifi.state enum is valid")
            else:
                h.error(f"net.wifi.state invalid: {snapshot!r}")
            if snapshot.get("connection_state") in CONNECTION_STATES:
                h.ok("net.wifi.connection_state enum is valid")
            else:
                h.error(f"net.wifi.connection_state invalid: {snapshot!r}")
            if snapshot.get("scan_state") in SCAN_STATES:
                h.ok("net.wifi.scan_state enum is valid")
            else:
                h.error(f"net.wifi.scan_state invalid: {snapshot!r}")
            if snapshot.get("availability") in {"ready", "disabled", "unavailable"}:
                h.ok("net.wifi.availability enum is valid")
            else:
                h.error(f"net.wifi.availability invalid: {snapshot!r}")
            if _is_optional_int(snapshot.get("scan_started_at")):
                h.ok("net.wifi.scan_started_at type is valid")
            else:
                h.error(f"net.wifi.scan_started_at invalid: {snapshot!r}")
            if _is_optional_int(snapshot.get("scan_finished_at")):
                h.ok("net.wifi.scan_finished_at type is valid")
            else:
                h.error(f"net.wifi.scan_finished_at invalid: {snapshot!r}")
            if _is_optional_str(snapshot.get("scan_last_error")):
                h.ok("net.wifi.scan_last_error type is valid")
            else:
                h.error(f"net.wifi.scan_last_error invalid: {snapshot!r}")
            if snapshot.get("manual_connect_state") in MANUAL_CONNECT_STATES:
                h.ok("net.wifi.manual_connect_state enum is valid")
            else:
                h.error(f"net.wifi.manual_connect_state invalid: {snapshot!r}")
            if _is_optional_str(snapshot.get("manual_connect_ssid")):
                h.ok("net.wifi.manual_connect_ssid type is valid")
            else:
                h.error(f"net.wifi.manual_connect_ssid invalid: {snapshot!r}")
            if snapshot.get("manual_connect_reason") in MANUAL_CONNECT_REASONS:
                h.ok("net.wifi.manual_connect_reason enum is valid")
            else:
                h.error(f"net.wifi.manual_connect_reason invalid: {snapshot!r}")
            if _is_optional_int(snapshot.get("manual_connect_started_at")):
                h.ok("net.wifi.manual_connect_started_at type is valid")
            else:
                h.error(f"net.wifi.manual_connect_started_at invalid: {snapshot!r}")
            if _is_optional_str(snapshot.get("network_id")):
                h.ok("net.wifi.network_id type is valid")
            else:
                h.error(f"net.wifi.network_id invalid: {snapshot!r}")

        bad_action = client.req("net.wifi", "no_such_action", {})
        if expect_envelope(h, bad_action, kind=ERR, topic="net.wifi", code="E_ACTION_UNKNOWN"):
            h.ok("net.wifi unknown action returns E_ACTION_UNKNOWN")

        bad_connect = client.req("net.wifi", "connect", {})
        if expect_envelope(h, bad_connect, kind=ERR, topic="net.wifi", code="E_ACTION_PAYLOAD"):
            h.ok("net.wifi connect {} returns E_ACTION_PAYLOAD")

        bad_forget = client.req("net.wifi", "forget", {})
        if expect_envelope(h, bad_forget, kind=ERR, topic="net.wifi", code="E_ACTION_PAYLOAD"):
            h.ok("net.wifi forget {} returns E_ACTION_PAYLOAD")

        bad_set_enabled = client.req("net.wifi", "set_enabled", {})
        if expect_envelope(h, bad_set_enabled, kind=ERR, topic="net.wifi", code="E_ACTION_PAYLOAD"):
            h.ok("net.wifi set_enabled {} returns E_ACTION_PAYLOAD")

        bad_set_airplane = client.req("net.wifi", "set_airplane_mode", {})
        if expect_envelope(h, bad_set_airplane, kind=ERR, topic="net.wifi", code="E_ACTION_PAYLOAD"):
            h.ok("net.wifi set_airplane_mode {} returns E_ACTION_PAYLOAD")

        scan_req_id = client.send_envelope(0, "net.wifi", "scan_start", {})
        scan, scan_pubs = _recv_until_reply(client, scan_req_id, max(args.timeout, 6.0))
        first_scan_ok = False
        if scan is None:
            h.error("net.wifi scan_start {} reply timed out")
        else:
            first_scan_ok = expect_rep_or_warn_service_err(
                h, scan, "net.wifi", "net.wifi scan_start {}"
            )

        second_scan_ok = False
        second_scan_pubs: list[dict] = []
        if first_scan_ok:
            second_req_id = client.send_envelope(0, "net.wifi", "scan_start", {})
            second_scan, second_scan_pubs = _recv_until_reply(client, second_req_id, max(args.timeout, 6.0))
            if second_scan is None:
                h.error("second net.wifi scan_start {} reply timed out")
            else:
                second_scan_ok = expect_rep_or_warn_service_err(
                    h,
                    second_scan,
                    "net.wifi",
                    "second net.wifi scan_start {}",
                )

        alias_scan_ok = False
        alias_scan_pubs: list[dict] = []
        if first_scan_ok:
            alias_req_id = client.send_envelope(0, "net.wifi", "scan", {})
            alias_scan, alias_scan_pubs = _recv_until_reply(client, alias_req_id, max(args.timeout, 6.0))
            if alias_scan is None:
                h.error("legacy net.wifi scan {} alias reply timed out")
            else:
                alias_scan_ok = expect_rep_or_warn_service_err(
                    h,
                    alias_scan,
                    "net.wifi",
                    "legacy net.wifi scan {} alias",
                )

        stop_active_ok = False
        stop_active_pubs: list[dict] = []
        if first_scan_ok:
            stop_active_req_id = client.send_envelope(0, "net.wifi", "scan_stop", {})
            stop_active, stop_active_pubs = _recv_until_reply(
                client, stop_active_req_id, max(args.timeout, 6.0)
            )
            if stop_active is None:
                h.error("net.wifi scan_stop {} during active scan reply timed out")
            else:
                stop_active_ok = expect_rep_or_warn_service_err(
                    h,
                    stop_active,
                    "net.wifi",
                    "net.wifi scan_stop {} during active scan",
                )

        if first_scan_ok:
            transition_payloads = [snapshot] if isinstance(snapshot, dict) else []
            transition_payloads.extend(_pub_payloads(scan_pubs))
            transition_payloads.extend(_pub_payloads(second_scan_pubs))
            transition_payloads.extend(_pub_payloads(alias_scan_pubs))
            transition_payloads.extend(_pub_payloads(stop_active_pubs))

            if any(payload.get("scan_state") in {"starting", "running"} for payload in transition_payloads):
                h.ok("net.wifi scan_start pushes snapshot into starting/running state")
            else:
                h.error(f"net.wifi scan_start never reported starting/running: {transition_payloads!r}")

        if first_scan_ok and second_scan_ok and alias_scan_ok and stop_active_ok:
            idle_pubs = _collect_wifi_pubs(client, max(args.timeout, 10.0), stop_on_idle=True)
            idle_payloads = _pub_payloads(idle_pubs)
            transition_payloads = [snapshot] if isinstance(snapshot, dict) else []
            transition_payloads.extend(_pub_payloads(scan_pubs))
            transition_payloads.extend(_pub_payloads(second_scan_pubs))
            transition_payloads.extend(_pub_payloads(alias_scan_pubs))
            transition_payloads.extend(_pub_payloads(stop_active_pubs))
            transition_payloads.extend(idle_payloads)
            if any(payload.get("scan_state") == "idle" for payload in transition_payloads):
                h.ok("net.wifi scan_stop returns snapshot to idle")
            else:
                h.error(f"net.wifi scan_stop did not return to idle: {transition_payloads!r}")

            stop_idle = client.req("net.wifi", "scan_stop", {})
            expect_rep_or_warn_service_err(
                h, stop_idle, "net.wifi", "net.wifi scan_stop {} while idle"
            )

        client.unsub("net.wifi")

        if args.mutate:
            if args.ssid:
                payload = {"ssid": args.ssid, "save": bool(args.save)}
                if args.psk is not None:
                    payload["psk"] = args.psk
                reply = client.req("net.wifi", "connect", payload)
                expect_rep_or_warn_service_err(h, reply, "net.wifi", f"net.wifi connect {payload!r}")
            else:
                h.warn("skipping net.wifi connect test: provide --ssid (and optionally --psk)")

            disconnect = client.req("net.wifi", "disconnect", {})
            expect_rep_or_warn_service_err(h, disconnect, "net.wifi", "net.wifi disconnect {}")

            target_forget = args.forget_ssid or args.ssid
            if target_forget:
                forget = client.req("net.wifi", "forget", {"ssid": target_forget})
                expect_rep_or_warn_service_err(h, forget, "net.wifi", f"net.wifi forget {{ssid:{target_forget!r}}}")
            else:
                h.warn("skipping net.wifi forget test: provide --forget-ssid or --ssid")
        else:
            h.warn("mutating Wi-Fi tests skipped; rerun with --mutate")

        if args.mutate_rfkill:
            reply = client.req("net.wifi", "set_enabled", {"enabled": True})
            expect_rep_or_warn_service_err(h, reply, "net.wifi", "net.wifi set_enabled {enabled:true}")

            reply = client.req("net.wifi", "set_airplane_mode", {"enabled": False})
            expect_rep_or_warn_service_err(h, reply, "net.wifi", "net.wifi set_airplane_mode {enabled:false}")
        else:
            h.warn("rfkill mutate tests skipped; rerun with --mutate-rfkill only on a safe local session")
    finally:
        client.close()

    return h.finish()


if __name__ == "__main__":
    main_guard(run)
