#!/usr/bin/env python3
from __future__ import annotations

import argparse

from _qsov_testlib import (
    ERR,
    PUB,
    Harness,
    add_common_args,
    choose_socket,
    connect_and_hello,
    expect_envelope,
    main_guard,
)


def run() -> int:
    parser = argparse.ArgumentParser(description="Manual tests for qsov theme service")
    add_common_args(parser, mutate=False)
    args = parser.parse_args()

    h = Harness("theme", strict=args.strict)
    socket_path = choose_socket(args)
    client, _ack = connect_and_hello(h, socket_path, args.timeout, "theme")
    try:
        sub = client.sub("theme")
        env = expect_envelope(h, sub, kind=PUB, topic="theme")
        if env:
            payload = env.get("payload")
            if isinstance(payload, dict):
                h.ok("theme snapshot is a top-level map")
                for key in ["meta", "palette", "tokens"]:
                    if key in payload:
                        h.ok(f"theme snapshot contains top-level key {key!r}")
                    else:
                        h.warn(f"theme snapshot missing top-level key {key!r}; custom design-tokens.toml may differ")
            else:
                h.error(f"theme snapshot is not a map: {payload!r}")
        client.unsub("theme")

        bad_action = client.req("theme", "no_such_action", {})
        if expect_envelope(h, bad_action, kind=ERR, topic="theme", code="E_ACTION_UNKNOWN"):
            h.ok("theme rejects REQ actions with E_ACTION_UNKNOWN")
    finally:
        client.close()

    return h.finish()


if __name__ == "__main__":
    main_guard(run)
