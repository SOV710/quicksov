#!/usr/bin/env python3
from __future__ import annotations

import argparse

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
        "--power-on",
        choices=["true", "false"],
        default=None,
        help="explicit powered state for --mutate power test; default=current snapshot value",
    )
    args = parser.parse_args()

    h = Harness("bluetooth", strict=args.strict)
    socket_path = choose_socket(args)
    client, _ack = connect_and_hello(h, socket_path, args.timeout, "bluetooth")
    try:
        sub = client.sub("bluetooth")
        env = expect_envelope(h, sub, kind=PUB, topic="bluetooth")
        snapshot = None
        if env and assert_dict_keys(
            h, env.get("payload"), REQUIRED, "bluetooth snapshot"
        ):
            snapshot = env["payload"]
            maybe_warn_unavailable(h, "bluetooth", snapshot)
            devices = snapshot.get("devices")
            if isinstance(devices, list):
                h.ok(f"bluetooth.devices is a list (len={len(devices)})")
                for idx, dev in enumerate(devices):
                    if isinstance(dev, dict):
                        assert_dict_keys(
                            h, dev, DEVICE_REQUIRED, f"bluetooth.devices[{idx}]"
                        )
                    else:
                        h.error(f"bluetooth.devices[{idx}] is not a map: {dev!r}")
            else:
                h.error(f"bluetooth.devices is not a list: {snapshot!r}")
        client.unsub("bluetooth")

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
            target_power = None
            if args.power_on is not None:
                target_power = args.power_on == "true"
            elif isinstance(snapshot, dict):
                current = snapshot.get("powered")
                if isinstance(current, bool):
                    target_power = current
            if target_power is None:
                h.warn(
                    "skipping bluetooth power test: no current powered state available"
                )
            else:
                reply = client.req("bluetooth", "power", {"on": target_power})
                expect_rep_or_warn_service_err(
                    h, reply, "bluetooth", f"bluetooth power {{on:{target_power}}}"
                )

            for action in ["scan_start", "scan_stop"]:
                reply = client.req("bluetooth", action, {})
                expect_rep_or_warn_service_err(
                    h, reply, "bluetooth", f"bluetooth {action} {{}}"
                )

            if args.address:
                for action in ["connect", "disconnect", "pair", "forget"]:
                    reply = client.req("bluetooth", action, {"address": args.address})
                    expect_rep_or_warn_service_err(
                        h,
                        reply,
                        "bluetooth",
                        f"bluetooth {action} {{address:{args.address!r}}}",
                    )
            else:
                h.warn("skipping bluetooth device-action tests: provide --address")
        else:
            h.warn("mutating bluetooth tests skipped; rerun with --mutate")
    finally:
        client.close()

    return h.finish()


if __name__ == "__main__":
    main_guard(run)
