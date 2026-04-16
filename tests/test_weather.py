#!/usr/bin/env python3
from __future__ import annotations

import argparse
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

REQUIRED = [
    "provider",
    "status",
    "ttl_sec",
    "location",
    "current",
    "hourly",
    "last_success_at",
    "error",
]
LOCATION_REQUIRED = ["name", "latitude", "longitude"]
CURRENT_REQUIRED = [
    "temperature_c",
    "apparent_c",
    "humidity_pct",
    "wind_kmh",
    "wmo_code",
    "icon",
    "description",
]
HOURLY_REQUIRED = ["time", "temperature_c", "wmo_code"]
ERROR_REQUIRED = ["kind", "message", "at"]
STATUS_VALUES = {"loading", "ready", "refreshing", "init_failed", "refresh_failed"}


def validate_snapshot(h: Harness, payload: Any) -> dict[str, Any] | None:
    if not assert_dict_keys(h, payload, REQUIRED, "weather snapshot"):
        return None
    assert isinstance(payload, dict)

    provider = payload.get("provider")
    if isinstance(provider, str) and provider:
        h.ok(f"weather.provider is a non-empty string ({provider})")
    else:
        h.error(f"weather.provider invalid: {payload!r}")

    status = payload.get("status")
    if status in STATUS_VALUES:
        h.ok(f"weather.status enum is valid ({status})")
    else:
        h.error(f"weather.status invalid: {payload!r}")

    ttl_sec = payload.get("ttl_sec")
    if isinstance(ttl_sec, int) and ttl_sec > 0:
        h.ok(f"weather.ttl_sec is positive ({ttl_sec})")
    else:
        h.error(f"weather.ttl_sec invalid: {payload!r}")

    location = payload.get("location")
    if location is None:
        h.warn("weather.location is null")
    elif isinstance(location, dict):
        assert_dict_keys(h, location, LOCATION_REQUIRED, "weather.location")
    else:
        h.error(f"weather.location invalid: {payload!r}")

    current = payload.get("current")
    if current is None:
        h.warn("weather.current is null")
    elif isinstance(current, dict):
        assert_dict_keys(h, current, CURRENT_REQUIRED, "weather.current")
    else:
        h.error(f"weather.current invalid: {payload!r}")

    hourly = payload.get("hourly")
    if isinstance(hourly, list):
        h.ok(f"weather.hourly is a list (len={len(hourly)})")
        if hourly:
            first = hourly[0]
            if isinstance(first, dict):
                assert_dict_keys(h, first, HOURLY_REQUIRED, "weather.hourly[0]")
            else:
                h.error(f"weather.hourly[0] invalid: {first!r}")
    else:
        h.error(f"weather.hourly invalid: {payload!r}")

    last_success_at = payload.get("last_success_at")
    if last_success_at is None:
        h.warn("weather.last_success_at is null")
    elif isinstance(last_success_at, int):
        h.ok("weather.last_success_at is an integer")
    else:
        h.error(f"weather.last_success_at invalid: {payload!r}")

    error = payload.get("error")
    if error is None:
        h.ok("weather.error is null")
    elif isinstance(error, dict):
        assert_dict_keys(h, error, ERROR_REQUIRED, "weather.error")
    else:
        h.error(f"weather.error invalid: {payload!r}")

    if current is None and last_success_at is not None:
        h.error("weather.current is null while weather.last_success_at is set")
    elif current is not None and last_success_at is None:
        h.warn("weather has current data but no last_success_at")

    maybe_warn_unavailable(h, "weather", payload)
    return payload


def run() -> int:
    parser = argparse.ArgumentParser(description="Manual tests for qsov weather service")
    add_common_args(parser)
    args = parser.parse_args()

    h = Harness("weather", strict=args.strict)
    socket_path = choose_socket(args)
    client, _ack = connect_and_hello(h, socket_path, args.timeout, "weather")
    try:
        sub = client.sub("weather")
        env = expect_envelope(h, sub, kind=PUB, topic="weather")
        if env:
            validate_snapshot(h, env.get("payload"))
        client.unsub("weather")

        bad_action = client.req("weather", "no_such_action", {})
        if expect_envelope(h, bad_action, kind=ERR, topic="weather", code="E_ACTION_UNKNOWN"):
            h.ok("weather unknown action returns E_ACTION_UNKNOWN")

        refresh = client.req("weather", "refresh", {})
        expect_rep_or_warn_service_err(h, refresh, "weather", "weather refresh {}")

        bad_payload = client.req("weather", "refresh", {"unexpected": 1})
        if expect_envelope(h, bad_payload, kind=ERR, topic="weather", code="E_ACTION_PAYLOAD"):
            h.ok("weather refresh rejects non-empty payloads")

        if args.mutate:
            h.ok("weather has no additional mutate-only actions beyond refresh")
    finally:
        client.close()

    return h.finish()


if __name__ == "__main__":
    main_guard(run)
