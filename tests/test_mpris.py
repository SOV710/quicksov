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

REQUIRED = ["active_player", "players"]
PLAYER_REQUIRED = [
    "bus_name",
    "identity",
    "playback_status",
    "title",
    "artist",
    "album",
    "art_url",
    "length_us",
    "position_us",
]


def run() -> int:
    parser = argparse.ArgumentParser(description="Manual tests for qsov mpris service")
    add_common_args(parser)
    parser.add_argument("--bus-name", default=None, help="player bus name for mutate/control tests")
    parser.add_argument(
        "--control",
        choices=["play_pause", "next", "prev", "stop"],
        default=None,
        help="optional player control to run during --mutate",
    )
    parser.add_argument("--seek-offset-us", type=int, default=None, help="run seek during --mutate")
    parser.add_argument("--position-us", type=int, default=None, help="run set_position during --mutate")
    args = parser.parse_args()

    h = Harness("mpris", strict=args.strict)
    socket_path = choose_socket(args)
    client, _ack = connect_and_hello(h, socket_path, args.timeout, "mpris")
    try:
        sub = client.sub("mpris")
        env = expect_envelope(h, sub, kind=PUB, topic="mpris")
        snapshot = None
        selected_bus = None
        if env and assert_dict_keys(h, env.get("payload"), REQUIRED, "mpris snapshot"):
            snapshot = env["payload"]
            maybe_warn_unavailable(h, "mpris", snapshot)
            players = snapshot.get("players")
            if isinstance(players, list):
                h.ok(f"mpris.players is a list (len={len(players)})")
                for idx, player in enumerate(players):
                    if isinstance(player, dict):
                        assert_dict_keys(h, player, PLAYER_REQUIRED, f"mpris.players[{idx}]")
                    else:
                        h.error(f"mpris.players[{idx}] is not a map: {player!r}")
                if args.bus_name is not None:
                    selected_bus = args.bus_name
                elif isinstance(snapshot.get("active_player"), str):
                    selected_bus = snapshot["active_player"]
                elif players and isinstance(players[0], dict) and isinstance(players[0].get("bus_name"), str):
                    selected_bus = players[0]["bus_name"]
            else:
                h.error(f"mpris.players is not a list: {snapshot!r}")
        client.unsub("mpris")

        bad_action = client.req("mpris", "no_such_action", {})
        if expect_envelope(h, bad_action, kind=ERR, topic="mpris", code="E_ACTION_UNKNOWN"):
            h.ok("mpris unknown action returns E_ACTION_UNKNOWN")

        bad_select = client.req("mpris", "select_active", {})
        if expect_envelope(h, bad_select, kind=ERR, topic="mpris", code="E_ACTION_PAYLOAD"):
            h.ok("mpris select_active {} returns E_ACTION_PAYLOAD")

        if selected_bus:
            reply = client.req("mpris", "select_active", {"bus_name": selected_bus})
            expect_rep_or_warn_service_err(h, reply, "mpris", f"mpris select_active {{bus_name:{selected_bus!r}}}")
        else:
            h.warn("skipping valid select_active test: no bus_name available")

        if args.mutate:
            if args.control and selected_bus:
                reply = client.req("mpris", args.control, {"bus_name": selected_bus})
                expect_rep_or_warn_service_err(h, reply, "mpris", f"mpris {args.control} {{bus_name:{selected_bus!r}}}")
            elif args.control and not selected_bus:
                h.warn("skipping mpris control test: no target bus name available")

            if args.seek_offset_us is not None:
                if selected_bus:
                    reply = client.req(
                        "mpris",
                        "seek",
                        {"bus_name": selected_bus, "offset_us": args.seek_offset_us},
                    )
                    expect_rep_or_warn_service_err(h, reply, "mpris", f"mpris seek bus_name={selected_bus!r} offset_us={args.seek_offset_us}")
                else:
                    h.warn("skipping mpris seek: no target bus name available")

            if args.position_us is not None:
                if selected_bus:
                    reply = client.req(
                        "mpris",
                        "set_position",
                        {"bus_name": selected_bus, "position_us": args.position_us},
                    )
                    expect_rep_or_warn_service_err(h, reply, "mpris", f"mpris set_position bus_name={selected_bus!r} position_us={args.position_us}")
                else:
                    h.warn("skipping mpris set_position: no target bus name available")
        else:
            h.warn("media-control mutate tests skipped; rerun with --mutate")
    finally:
        client.close()

    return h.finish()


if __name__ == "__main__":
    main_guard(run)
