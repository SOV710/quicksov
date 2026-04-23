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
    get_map_value,
    main_guard,
)

SNAPSHOT_REQUIRED = ["availability", "desktop_entries", "icon_entries"]
RESOLVE_REQUIRED = ["display_name", "icon", "icon_name", "desktop_entry", "match_source"]


def run() -> int:
    parser = argparse.ArgumentParser(description="Manual tests for qsov icon service")
    add_common_args(parser, mutate=False)
    parser.add_argument("--app-id", default=None, help="lookup app_id")
    parser.add_argument("--desktop-entry", default=None, help="lookup desktop entry id")
    parser.add_argument("--wm-class", default=None, help="lookup WM class")
    parser.add_argument("--app-name", default=None, help="lookup app name")
    parser.add_argument("--binary", default=None, help="lookup binary name")
    parser.add_argument("--icon-hint", default=None, help="lookup explicit icon hint")
    parser.add_argument("--process-id", type=int, default=None, help="lookup process id")
    args = parser.parse_args()

    h = Harness("icon", strict=args.strict)
    socket_path = choose_socket(args)
    client, _ack = connect_and_hello(h, socket_path, args.timeout, "icon")
    try:
        sub = client.sub("icon")
        env = expect_envelope(h, sub, kind=PUB, topic="icon")
        if env and assert_dict_keys(h, env.get("payload"), SNAPSHOT_REQUIRED, "icon snapshot"):
            payload = env["payload"]
            if payload.get("availability") == "ready":
                h.ok("icon.availability is ready")
            else:
                h.error(f"icon.availability invalid: {payload!r}")
            get_map_value(payload, "desktop_entries", int, h)
            get_map_value(payload, "icon_entries", int, h)
        client.unsub("icon")

        bad_action = client.req("icon", "no_such_action", {})
        if expect_envelope(h, bad_action, kind=ERR, topic="icon", code="E_ACTION_UNKNOWN"):
            h.ok("icon unknown action returns E_ACTION_UNKNOWN")

        bad_payload = client.req("icon", "resolve", {})
        if expect_envelope(h, bad_payload, kind=ERR, topic="icon", code="E_ACTION_PAYLOAD"):
            h.ok("icon resolve {} returns E_ACTION_PAYLOAD")

        payload = {
            "app_id": args.app_id,
            "desktop_entry": args.desktop_entry,
            "wm_class": args.wm_class,
            "app_name": args.app_name,
            "binary": args.binary,
            "icon_hint": args.icon_hint,
            "process_id": args.process_id,
        }
        payload = {key: value for key, value in payload.items() if value is not None}

        if payload:
            reply = client.req("icon", "resolve", payload)
            env = expect_envelope(h, reply, kind=REP, topic="icon")
            if env and assert_dict_keys(h, env.get("payload"), RESOLVE_REQUIRED, "icon resolve reply"):
                result = env["payload"]
                for key in RESOLVE_REQUIRED:
                    if isinstance(result.get(key), str):
                        h.ok(f"icon resolve payload field {key!r} is a string")
                    else:
                        h.error(f"icon resolve payload field {key!r} invalid: {result!r}")
        else:
            h.warn("resolve test skipped; pass one or more lookup args to run it")
    finally:
        client.close()

    return h.finish()


if __name__ == "__main__":
    main_guard(run)
