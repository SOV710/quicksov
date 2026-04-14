#!/usr/bin/env python3
from __future__ import annotations

import argparse

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
    "present",
    "on_battery",
    "level",
    "state",
    "time_to_empty_sec",
    "time_to_full_sec",
    "power_profile",
]


def run() -> int:
    parser = argparse.ArgumentParser(description="Manual tests for qsov battery service")
    add_common_args(parser)
    parser.add_argument(
        "--profile",
        default=None,
        choices=["performance", "balanced", "power-saver"],
        help="profile to apply during --mutate; default = current snapshot profile",
    )
    args = parser.parse_args()

    h = Harness("battery", strict=args.strict)
    socket_path = choose_socket(args)
    client, _ack = connect_and_hello(h, socket_path, args.timeout, "battery")
    try:
        sub = client.sub("battery")
        env = expect_envelope(h, sub, kind=PUB, topic="battery")
        snapshot = None
        if env and assert_dict_keys(h, env.get("payload"), REQUIRED, "battery snapshot"):
            snapshot = env["payload"]
            maybe_warn_unavailable(h, "battery", snapshot)
            if isinstance(snapshot.get("level"), int) and 0 <= snapshot["level"] <= 100:
                h.ok("battery.level is in [0,100]")
            else:
                h.error(f"battery.level invalid: {snapshot!r}")
            if snapshot.get("state") in {
                "charging",
                "discharging",
                "empty",
                "fully_charged",
                "pending_charge",
                "pending_discharge",
                "unknown",
            }:
                h.ok("battery.state enum is valid")
            else:
                h.error(f"battery.state invalid: {snapshot!r}")
        client.unsub("battery")

        bad_action = client.req("battery", "no_such_action", {})
        if expect_envelope(h, bad_action, kind=ERR, topic="battery", code="E_ACTION_UNKNOWN"):
            h.ok("battery unknown action returns E_ACTION_UNKNOWN")

        bad_payload = client.req("battery", "set_power_profile", {})
        if expect_envelope(h, bad_payload, kind=ERR, topic="battery", code="E_ACTION_PAYLOAD"):
            h.ok("battery set_power_profile {} returns E_ACTION_PAYLOAD")

        if args.mutate:
            target = args.profile
            if target is None and isinstance(snapshot, dict):
                current = snapshot.get("power_profile")
                if current in {"performance", "balanced", "power-saver"}:
                    target = current
            if target is None:
                h.warn("skipping valid set_power_profile test: no usable profile in snapshot")
            else:
                reply = client.req("battery", "set_power_profile", {"profile": target})
                expect_rep_or_warn_service_err(h, reply, "battery", f"battery set_power_profile {{profile:{target!r}}}")
        else:
            h.warn("valid battery mutation test skipped; rerun with --mutate")
    finally:
        client.close()

    return h.finish()


if __name__ == "__main__":
    main_guard(run)
