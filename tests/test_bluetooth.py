#!/usr/bin/env python3
from __future__ import annotations

import argparse
import time
from typing import Any

from _qsov_testlib import (
    ERR,
    PUB,
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

REQUIRED = ["powered", "discovering", "devices"]
DEVICE_REQUIRED = [
    "address",
    "name",
    "icon",
    "paired",
    "connected",
    "trusted",
    "battery",
]


def snapshot_addresses(snapshot: dict[str, Any] | None) -> set[str]:
    if not isinstance(snapshot, dict):
        return set()
    devices = snapshot.get("devices")
    if not isinstance(devices, list):
        return set()
    out: set[str] = set()
    for dev in devices:
        if isinstance(dev, dict):
            addr = dev.get("address")
            if isinstance(addr, str):
                out.add(addr)
    return out


def validate_snapshot(
    h: Harness, payload: Any, *, detailed_devices: bool
) -> dict[str, Any] | None:
    if not assert_dict_keys(h, payload, REQUIRED, "bluetooth snapshot"):
        return None
    assert isinstance(payload, dict)
    maybe_warn_unavailable(h, "bluetooth", payload)
    devices = payload.get("devices")
    if isinstance(devices, list):
        h.ok(f"bluetooth.devices is a list (len={len(devices)})")
        if detailed_devices:
            for idx, dev in enumerate(devices):
                if isinstance(dev, dict):
                    assert_dict_keys(
                        h, dev, DEVICE_REQUIRED, f"bluetooth.devices[{idx}]"
                    )
                else:
                    h.error(f"bluetooth.devices[{idx}] is not a map: {dev!r}")
    else:
        h.error(f"bluetooth.devices is not a list: {payload!r}")
    return payload


def recv_pub_snapshot(
    client,
    h: Harness,
    timeout: float,
    *,
    detailed_devices: bool,
    label: str,
) -> dict[str, Any] | None:
    sock = client._ensure_sock()
    prev = sock.gettimeout()
    sock.settimeout(timeout)
    try:
        msg = client.recv_obj()
    finally:
        sock.settimeout(prev)
    env = expect_envelope(h, msg, kind=PUB, topic="bluetooth")
    if env is None:
        return None
    snapshot = validate_snapshot(
        h, env.get("payload"), detailed_devices=detailed_devices
    )
    if snapshot is not None:
        powered = snapshot.get("powered")
        discovering = snapshot.get("discovering")
        devices = snapshot.get("devices")
        n = len(devices) if isinstance(devices, list) else "?"
        h.ok(
            f"{label}: snapshot powered={powered!r} discovering={discovering!r} devices={n}"
        )
    return snapshot


def collect_scan_results(
    client,
    h: Harness,
    *,
    scan_seconds: float,
    idle_timeout: float,
) -> tuple[dict[str, Any] | None, int]:
    deadline = time.monotonic() + scan_seconds
    latest: dict[str, Any] | None = None
    updates = 0
    while True:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            break
        timeout = idle_timeout if remaining > idle_timeout else remaining
        try:
            snap = recv_pub_snapshot(
                client,
                h,
                timeout,
                detailed_devices=False,
                label="bluetooth scan result",
            )
        except TimeoutError:
            continue
        if snap is not None:
            latest = snap
            updates += 1
    return latest, updates


def run() -> int:
    parser = argparse.ArgumentParser(
        description="Manual tests for qsov bluetooth service"
    )
    add_common_args(parser)
    parser.add_argument(
        "--address",
        default=None,
        help="device address for connect/disconnect/pair/forget mutate tests",
    )
    parser.add_argument(
        "--scan-seconds",
        type=float,
        default=8.0,
        help="how long to keep bluetooth scan on and collect PUB snapshots",
    )
    parser.add_argument(
        "--skip-device-actions",
        action="store_true",
        help="skip connect/disconnect/pair/forget even when --address is provided",
    )
    args = parser.parse_args()

    h = Harness("bluetooth", strict=args.strict)
    socket_path = choose_socket(args)
    client, _ack = connect_and_hello(h, socket_path, args.timeout, "bluetooth")
    try:
        initial = client.sub("bluetooth")
        env = expect_envelope(h, initial, kind=PUB, topic="bluetooth")
        snapshot: dict[str, Any] | None = None
        if env is not None:
            snapshot = validate_snapshot(h, env.get("payload"), detailed_devices=True)

        bad_action = client.req("bluetooth", "no_such_action", {})
        if expect_envelope(
            h, bad_action, kind=ERR, topic="bluetooth", code="E_ACTION_UNKNOWN"
        ):
            h.ok("bluetooth unknown action returns E_ACTION_UNKNOWN")

        bad_power = client.req("bluetooth", "power", {})
        if expect_envelope(
            h, bad_power, kind=ERR, topic="bluetooth", code="E_ACTION_PAYLOAD"
        ):
            h.ok("bluetooth power {} returns E_ACTION_PAYLOAD")

        bad_connect = client.req("bluetooth", "connect", {})
        if expect_envelope(
            h, bad_connect, kind=ERR, topic="bluetooth", code="E_ACTION_PAYLOAD"
        ):
            h.ok("bluetooth connect {} returns E_ACTION_PAYLOAD")

        if args.mutate:
            reply = client.req("bluetooth", "power", {"on": True})
            expect_rep_or_warn_service_err(
                h, reply, "bluetooth", "bluetooth power {on:True}"
            )
            latest = recv_pub_snapshot(
                client,
                h,
                min(args.timeout, 5.0),
                detailed_devices=False,
                label="after power on",
            )
            if latest is not None:
                snapshot = latest
                if latest.get("powered") is True:
                    h.ok("bluetooth post-power-on snapshot shows powered=true")
                else:
                    h.warn(
                        f"bluetooth post-power-on snapshot still shows powered={latest.get('powered')!r}"
                    )

            reply = client.req("bluetooth", "scan_start", {})
            expect_rep_or_warn_service_err(
                h, reply, "bluetooth", "bluetooth scan_start {}"
            )
            before_addrs = snapshot_addresses(snapshot)
            latest, updates = collect_scan_results(
                client,
                h,
                scan_seconds=args.scan_seconds,
                idle_timeout=min(1.0, args.timeout),
            )
            if latest is not None:
                snapshot = latest
            if updates > 0:
                h.ok(f"bluetooth scan workflow produced {updates} PUB update(s)")
            else:
                h.warn(
                    "bluetooth scan workflow produced no PUB updates during scan window"
                )
            after_addrs = snapshot_addresses(snapshot)
            gained = sorted(after_addrs - before_addrs)
            if gained:
                h.ok(
                    f"bluetooth scan discovered {len(gained)} new address(es): {', '.join(gained)}"
                )
            else:
                h.warn(
                    "bluetooth scan did not discover any new addresses during this run"
                )

            if args.address and not args.skip_device_actions:
                if args.address not in after_addrs:
                    h.warn(
                        f"skipping bluetooth device-action tests: address {args.address!r} not present in current scan results"
                    )
                else:
                    for action in ["connect", "disconnect", "pair", "forget"]:
                        reply = client.req(
                            "bluetooth", action, {"address": args.address}
                        )
                        expect_rep_or_warn_service_err(
                            h,
                            reply,
                            "bluetooth",
                            f"bluetooth {action} {{address:{args.address!r}}}",
                        )
            elif args.address and args.skip_device_actions:
                h.warn("bluetooth device-action tests skipped by --skip-device-actions")
            else:
                h.warn("skipping bluetooth device-action tests: provide --address")

            reply = client.req("bluetooth", "scan_stop", {})
            expect_rep_or_warn_service_err(
                h, reply, "bluetooth", "bluetooth scan_stop {}"
            )
            latest = recv_pub_snapshot(
                client,
                h,
                min(args.timeout, 5.0),
                detailed_devices=False,
                label="after scan off",
            )
            if latest is not None:
                snapshot = latest
                if latest.get("discovering") is False:
                    h.ok("bluetooth post-scan-stop snapshot shows discovering=false")
                else:
                    h.warn(
                        f"bluetooth post-scan-stop snapshot still shows discovering={latest.get('discovering')!r}"
                    )

            reply = client.req("bluetooth", "power", {"on": False})
            expect_rep_or_warn_service_err(
                h, reply, "bluetooth", "bluetooth power {on:False}"
            )
            latest = recv_pub_snapshot(
                client,
                h,
                min(args.timeout, 5.0),
                detailed_devices=False,
                label="after power off",
            )
            if latest is not None:
                snapshot = latest
                if latest.get("powered") is False:
                    h.ok("bluetooth post-power-off snapshot shows powered=false")
                else:
                    h.warn(
                        f"bluetooth post-power-off snapshot still shows powered={latest.get('powered')!r}"
                    )
        else:
            h.warn("mutating bluetooth tests skipped; rerun with --mutate")

        client.unsub("bluetooth")
        h.ok("bluetooth unsubscribe sent")
    finally:
        client.close()

    return h.finish()


if __name__ == "__main__":
    main_guard(run)
