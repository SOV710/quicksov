#!/usr/bin/env python3
from __future__ import annotations

import argparse

from _qsov_testlib import (
    ERR,
    PUB,
    REP,
    Harness,
    QsovClient,
    add_common_args,
    assert_dict_keys,
    choose_socket,
    connect_and_hello,
    expect_envelope,
    get_map_value,
    main_guard,
)


def run() -> int:
    parser = argparse.ArgumentParser(description="Manual tests for qsov meta + protocol handshake")
    add_common_args(parser, mutate=False)
    args = parser.parse_args()

    h = Harness("meta", strict=args.strict)
    socket_path = choose_socket(args)

    client, ack = connect_and_hello(h, socket_path, args.timeout, "meta")
    try:
        if isinstance(ack.get("server_version"), str):
            h.ok("HelloAck.server_version is a string")
        else:
            h.error(f"HelloAck.server_version invalid: {ack!r}")

        ping = client.req("meta", "ping", {})
        env = expect_envelope(h, ping, kind=REP, topic="meta")
        if env and assert_dict_keys(h, env.get("payload"), ["pong", "server_time"], "meta ping reply"):
            payload = env["payload"]
            if payload.get("pong") is True:
                h.ok("meta ping returned pong=true")
            else:
                h.error(f"meta ping pong field invalid: {payload!r}")
            get_map_value(payload, "server_time", int, h)

        bad_ping = client.req("meta", "ping", {"x": 1})
        if expect_envelope(h, bad_ping, kind=ERR, topic="meta", code="E_ACTION_PAYLOAD"):
            h.ok("meta ping with non-empty payload returns E_ACTION_PAYLOAD")

        shutdown = client.req("meta", "shutdown", {})
        if expect_envelope(h, shutdown, kind=ERR, topic="meta", code="E_ACTION_UNKNOWN"):
            h.ok("meta shutdown remains reserved and returns E_ACTION_UNKNOWN")

        sub = client.sub("meta")
        env = expect_envelope(h, sub, kind=PUB, topic="meta")
        if env and assert_dict_keys(
            h,
            env.get("payload"),
            ["server_version", "uptime_sec", "services", "config_needs_restart"],
            "meta snapshot",
        ):
            payload = env["payload"]
            get_map_value(payload, "server_version", str, h)
            get_map_value(payload, "uptime_sec", int, h)
            services = get_map_value(payload, "services", dict, h)
            get_map_value(payload, "config_needs_restart", bool, h)
            if isinstance(services, dict):
                meta_entry = services.get("meta")
                if not isinstance(meta_entry, dict):
                    h.error(f"meta.services.meta missing or invalid: {services!r}")
                elif meta_entry.get("status") in {"healthy", "degraded", "unavailable"}:
                    h.ok("meta.services.meta.status is valid")
                else:
                    h.error(f"invalid meta.services.meta.status: {meta_entry!r}")
        client.unsub("meta")
    finally:
        client.close()

    timeout_client = QsovClient(socket_path, timeout=max(args.timeout, 4.0))
    timeout_client.connect()
    try:
        msg = timeout_client.recv_obj()
        if isinstance(msg, dict) and msg.get("code") == "E_HANDSHAKE_TIMEOUT":
            h.ok("no-Hello handshake timeout returns raw E_HANDSHAKE_TIMEOUT body")
        else:
            h.error(f"unexpected handshake-timeout response: {msg!r}")
    finally:
        timeout_client.close()

    version_client = QsovClient(socket_path, timeout=max(args.timeout, 4.0))
    version_client.connect()
    try:
        msg = version_client.hello(proto_version="qsov/999")
        if isinstance(msg, dict) and msg.get("code") == "E_PROTO_VERSION":
            h.ok("bad proto version returns raw E_PROTO_VERSION body")
        else:
            h.error(f"unexpected bad-version response: {msg!r}")
    finally:
        version_client.close()

    return h.finish()


if __name__ == "__main__":
    main_guard(run)
