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

REQUIRED = ["location", "current", "hourly", "updated_at", "offline"]


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
        if env and assert_dict_keys(h, env.get("payload"), REQUIRED, "weather snapshot"):
            snapshot = env["payload"]
            maybe_warn_unavailable(h, "weather", snapshot)
            if isinstance(snapshot.get("hourly"), list):
                h.ok(f"weather.hourly is a list (len={len(snapshot['hourly'])})")
            else:
                h.error(f"weather.hourly invalid: {snapshot!r}")
            if isinstance(snapshot.get("offline"), bool):
                h.ok("weather.offline is a bool")
            else:
                h.error(f"weather.offline invalid: {snapshot!r}")
        client.unsub("weather")

        bad_action = client.req("weather", "no_such_action", {})
        if expect_envelope(h, bad_action, kind=ERR, topic="weather", code="E_ACTION_UNKNOWN"):
            h.ok("weather unknown action returns E_ACTION_UNKNOWN")

        refresh = client.req("weather", "refresh", {})
        expect_rep_or_warn_service_err(h, refresh, "weather", "weather refresh {}")

        permissive = client.req("weather", "refresh", {"unexpected": 1})
        if expect_rep_or_warn_service_err(h, permissive, "weather", "weather refresh {unexpected:1}"):
            h.warn("weather refresh accepts extra payload fields; implementation is permissive")

        if args.mutate:
            h.ok("weather has no additional mutate-only actions beyond refresh")
    finally:
        client.close()

    return h.finish()


if __name__ == "__main__":
    main_guard(run)
