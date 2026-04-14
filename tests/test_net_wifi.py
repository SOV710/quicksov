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
    "interface",
    "state",
    "ssid",
    "bssid",
    "rssi_dbm",
    "signal_pct",
    "frequency",
    "saved_networks",
    "scan_results",
]


def run() -> int:
    parser = argparse.ArgumentParser(description="Manual tests for qsov net.wifi service")
    add_common_args(parser)
    parser.add_argument("--ssid", default=None, help="SSID for --mutate connect test")
    parser.add_argument("--psk", default=None, help="PSK for --mutate connect test")
    parser.add_argument("--save", action="store_true", help="use save=true for --mutate connect")
    parser.add_argument("--forget-ssid", default=None, help="SSID for --mutate forget test")
    args = parser.parse_args()

    h = Harness("net.wifi", strict=args.strict)
    socket_path = choose_socket(args)
    client, _ack = connect_and_hello(h, socket_path, args.timeout, "net.wifi")
    try:
        sub = client.sub("net.wifi")
        env = expect_envelope(h, sub, kind=PUB, topic="net.wifi")
        snapshot = None
        if env and assert_dict_keys(h, env.get("payload"), REQUIRED, "net.wifi snapshot"):
            snapshot = env["payload"]
            maybe_warn_unavailable(h, "net.wifi", snapshot)
            if snapshot.get("state") in {
                "disconnected",
                "scanning",
                "associating",
                "connected",
                "unknown",
            }:
                h.ok("net.wifi.state enum is valid")
            else:
                h.error(f"net.wifi.state invalid: {snapshot!r}")
        client.unsub("net.wifi")

        bad_action = client.req("net.wifi", "no_such_action", {})
        if expect_envelope(h, bad_action, kind=ERR, topic="net.wifi", code="E_ACTION_UNKNOWN"):
            h.ok("net.wifi unknown action returns E_ACTION_UNKNOWN")

        bad_connect = client.req("net.wifi", "connect", {})
        if expect_envelope(h, bad_connect, kind=ERR, topic="net.wifi", code="E_ACTION_PAYLOAD"):
            h.ok("net.wifi connect {} returns E_ACTION_PAYLOAD")

        bad_forget = client.req("net.wifi", "forget", {})
        if expect_envelope(h, bad_forget, kind=ERR, topic="net.wifi", code="E_ACTION_PAYLOAD"):
            h.ok("net.wifi forget {} returns E_ACTION_PAYLOAD")

        scan = client.req("net.wifi", "scan", {})
        expect_rep_or_warn_service_err(h, scan, "net.wifi", "net.wifi scan {}")

        if args.mutate:
            if args.ssid:
                payload = {"ssid": args.ssid, "save": bool(args.save)}
                if args.psk is not None:
                    payload["psk"] = args.psk
                reply = client.req("net.wifi", "connect", payload)
                expect_rep_or_warn_service_err(h, reply, "net.wifi", f"net.wifi connect {payload!r}")
            else:
                h.warn("skipping net.wifi connect test: provide --ssid (and optionally --psk)")

            disconnect = client.req("net.wifi", "disconnect", {})
            expect_rep_or_warn_service_err(h, disconnect, "net.wifi", "net.wifi disconnect {}")

            target_forget = args.forget_ssid or args.ssid
            if target_forget:
                forget = client.req("net.wifi", "forget", {"ssid": target_forget})
                expect_rep_or_warn_service_err(h, forget, "net.wifi", f"net.wifi forget {{ssid:{target_forget!r}}}")
            else:
                h.warn("skipping net.wifi forget test: provide --forget-ssid or --ssid")
        else:
            h.warn("mutating Wi-Fi tests skipped; rerun with --mutate")
    finally:
        client.close()

    return h.finish()


if __name__ == "__main__":
    main_guard(run)
