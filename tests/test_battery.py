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
    main_guard,
    maybe_warn_unavailable,
)


REQUIRED = [
    "availability",
    "present",
    "on_battery",
    "level",
    "state",
    "time_to_empty_sec",
    "time_to_full_sec",
    "batteries",
    "power_profile",
    "power_profile_available",
    "power_profile_backend",
    "power_profile_reason",
    "power_profile_choices",
]

ENTRY_REQUIRED = [
    "name",
    "present",
    "level",
    "state",
    "health_percent",
    "energy_rate_w",
    "energy_now_wh",
    "energy_full_wh",
    "energy_design_wh",
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
            if snapshot.get("availability") in {
                "ready",
                "no_battery",
                "backend_unavailable",
            }:
                h.ok("battery.availability enum is valid")
            else:
                h.error(f"battery.availability invalid: {snapshot!r}")
            if isinstance(snapshot.get("level"), int) and 0 <= snapshot["level"] <= 100:
                h.ok("battery.level is in [0,100]")
            else:
                h.error(f"battery.level invalid: {snapshot!r}")
            if snapshot.get("state") in {
                "charging",
                "discharging",
                "fully_charged",
                "not_charging",
                "empty",
                "unknown",
            }:
                h.ok("battery.state enum is valid")
            else:
                h.error(f"battery.state invalid: {snapshot!r}")
            if isinstance(snapshot.get("power_profile_available"), bool):
                h.ok("battery.power_profile_available is boolean")
            else:
                h.error(f"battery.power_profile_available invalid: {snapshot!r}")
            if snapshot.get("power_profile") in {
                "performance",
                "balanced",
                "power-saver",
                "custom",
                "unknown",
            }:
                h.ok("battery.power_profile enum is valid")
            else:
                h.error(f"battery.power_profile invalid: {snapshot!r}")
            if snapshot.get("power_profile_backend") in {"platform_profile", "none"}:
                h.ok("battery.power_profile_backend enum is valid")
            else:
                h.error(f"battery.power_profile_backend invalid: {snapshot!r}")
            if snapshot.get("power_profile_reason") in {
                None,
                "unsupported",
                "helper_unavailable",
                "permission_denied",
                "backend_unavailable",
                "write_failed",
            }:
                h.ok("battery.power_profile_reason enum is valid")
            else:
                h.error(f"battery.power_profile_reason invalid: {snapshot!r}")
            choices = snapshot.get("power_profile_choices")
            if isinstance(choices, list) and all(
                choice in {"performance", "balanced", "power-saver"} for choice in choices
            ):
                h.ok("battery.power_profile_choices is valid")
            else:
                h.error(f"battery.power_profile_choices invalid: {snapshot!r}")
            entries = snapshot.get("batteries")
            if isinstance(entries, list):
                h.ok("battery.batteries is an array")
                for index, entry in enumerate(entries):
                    if not assert_dict_keys(h, entry, ENTRY_REQUIRED, f"battery entry #{index}"):
                        continue
                    if entry.get("state") in {
                        "charging",
                        "discharging",
                        "fully_charged",
                        "not_charging",
                        "empty",
                        "unknown",
                    }:
                        h.ok(f"battery entry #{index} state enum is valid")
                    else:
                        h.error(f"battery entry #{index} state invalid: {entry!r}")
                    if isinstance(entry.get("level"), int) and 0 <= entry["level"] <= 100:
                        h.ok(f"battery entry #{index} level is in [0,100]")
                    else:
                        h.error(f"battery entry #{index} level invalid: {entry!r}")
            else:
                h.error(f"battery.batteries invalid: {snapshot!r}")

            for field in [
                "health_percent",
                "energy_rate_w",
                "energy_now_wh",
                "energy_full_wh",
                "energy_design_wh",
            ]:
                value = snapshot.get(field)
                if value is None or isinstance(value, (int, float)):
                    h.ok(f"battery.{field} is numeric-or-null")
                else:
                    h.error(f"battery.{field} invalid: {snapshot!r}")
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
                if not isinstance(reply, dict):
                    h.error(f"battery set_power_profile {{profile:{target!r}}}: message is not a map: {reply!r}")
                elif reply.get("kind") == REP and reply.get("topic") == "battery":
                    h.ok(f"battery set_power_profile {{profile:{target!r}}}: got REP")
                elif reply.get("kind") == ERR and reply.get("topic") == "battery":
                    payload = reply.get("payload")
                    code = payload.get("code") if isinstance(payload, dict) else None
                    if code in {"E_SERVICE_INTERNAL", "E_SERVICE_UNAVAILABLE", "E_PERMISSION"}:
                        h.warn(
                            f"battery set_power_profile {{profile:{target!r}}}: service returned {code}; helper may be unreachable, helper auth may have denied qsovd, or backend write may have failed: {reply!r}"
                        )
                        if code == "E_PERMISSION":
                            sub = client.sub("battery")
                            refreshed = expect_envelope(h, sub, kind=PUB, topic="battery")
                            payload = refreshed.get("payload") if isinstance(refreshed, dict) else None
                            if isinstance(payload, dict):
                                reason = payload.get("power_profile_reason")
                                if reason in {"permission_denied", None}:
                                    h.ok(
                                        "battery snapshot after permission error is consistent with helper auth denial semantics"
                                    )
                                else:
                                    h.warn(
                                        f"battery snapshot after permission error has unexpected power_profile_reason: {payload!r}"
                                    )
                            client.unsub("battery")
                    else:
                        h.error(
                            f"battery set_power_profile {{profile:{target!r}}}: unexpected reply: {reply!r}"
                        )
                else:
                    h.error(f"battery set_power_profile {{profile:{target!r}}}: unexpected reply: {reply!r}")
        else:
            h.warn("valid battery mutation test skipped; rerun with --mutate")
    finally:
        client.close()

    return h.finish()


if __name__ == "__main__":
    main_guard(run)
