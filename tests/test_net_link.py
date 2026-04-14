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
    main_guard,
)

REQUIRED = ["interfaces"]
IFACE_REQUIRED = [
    "name",
    "kind",
    "operstate",
    "carrier",
    "mac",
    "mtu",
    "ipv4",
    "ipv6",
    "rx_bytes",
    "tx_bytes",
]


def run() -> int:
    parser = argparse.ArgumentParser(
        description="Manual tests for qsov net.link service"
    )
    add_common_args(parser, mutate=False)
    args = parser.parse_args()

    h = Harness("net.link", strict=args.strict)
    socket_path = choose_socket(args)
    client, _ack = connect_and_hello(h, socket_path, args.timeout, "net.link")
    try:
        sub = client.sub("net.link")
        env = expect_envelope(h, sub, kind=PUB, topic="net.link")
        if env and assert_dict_keys(
            h, env.get("payload"), REQUIRED, "net.link snapshot"
        ):
            payload = env["payload"]
            interfaces = payload.get("interfaces")
            if isinstance(interfaces, list):
                h.ok(f"net.link.interfaces is a list (len={len(interfaces)})")
                if not interfaces:
                    h.warn("net.link.interfaces is empty")
                seen = set()
                for idx, iface in enumerate(interfaces):
                    if not isinstance(iface, dict):
                        h.error(f"interface #{idx} is not a map: {iface!r}")
                        continue
                    assert_dict_keys(
                        h, iface, IFACE_REQUIRED, f"net.link.interfaces[{idx}]"
                    )
                    name = iface.get("name")
                    if isinstance(name, str):
                        if name in seen:
                            h.warn(f"duplicate interface name observed: {name}")
                        seen.add(name)
                    if iface.get("kind") not in {
                        "wifi",
                        "ethernet",
                        "loopback",
                        "other",
                    }:
                        h.error(f"invalid net.link kind: {iface!r}")
                    if iface.get("operstate") not in {
                        "up",
                        "down",
                        "unknown",
                        "dormant",
                        "lowerlayerdown",
                        "lower_layer_down",
                        "notpresent",
                        "not_present",
                        "testing",
                    }:
                        h.error(f"invalid net.link operstate: {iface!r}")
            else:
                h.error(f"net.link.interfaces is not a list: {payload!r}")
        client.unsub("net.link")

        bad_action = client.req("net.link", "anything", {})
        if expect_envelope(
            h, bad_action, kind=ERR, topic="net.link", code="E_ACTION_UNKNOWN"
        ):
            h.ok("net.link rejects all REQ actions with E_ACTION_UNKNOWN")
    finally:
        client.close()

    return h.finish()


if __name__ == "__main__":
    main_guard(run)
