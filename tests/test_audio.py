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

REQUIRED = ["default_sink", "default_source", "sinks", "sources", "streams"]
NODE_REQUIRED = ["id", "name", "description", "volume_pct", "muted"]
STREAM_REQUIRED = ["id", "app_name", "binary", "title", "icon", "volume_pct", "muted"]


def _read_snapshot(h: Harness, socket_path: str, timeout: float) -> dict[str, Any] | None:
    client, _ack = connect_and_hello(h, socket_path, timeout, "audio")
    try:
        sub = client.sub("audio")
        env = expect_envelope(h, sub, kind=PUB, topic="audio")
        if env is None:
            return None
        payload = env.get("payload")
        if not assert_dict_keys(h, payload, REQUIRED, "audio snapshot"):
            return None
        snapshot = payload
        maybe_warn_unavailable(h, "audio", snapshot)
        for list_name in ["sinks", "sources"]:
            nodes = snapshot.get(list_name)
            if isinstance(nodes, list):
                h.ok(f"audio.{list_name} is a list (len={len(nodes)})")
                for idx, node in enumerate(nodes):
                    if isinstance(node, dict):
                        assert_dict_keys(h, node, NODE_REQUIRED, f"audio.{list_name}[{idx}]")
                    else:
                        h.error(f"audio.{list_name}[{idx}] is not a map: {node!r}")
            else:
                h.error(f"audio.{list_name} is not a list: {snapshot!r}")

        sinks = snapshot.get("sinks")
        if isinstance(sinks, list) and sinks:
            default_sink = snapshot.get("default_sink")
            sink_names = {
                node.get("name")
                for node in sinks
                if isinstance(node, dict) and isinstance(node.get("name"), str)
            }
            h.expect(
                isinstance(default_sink, str) and default_sink in sink_names,
                f"audio.default_sink resolves to a known sink: {default_sink!r}",
                f"audio.default_sink does not resolve to a known sink: {default_sink!r}",
            )

        sources = snapshot.get("sources")
        if isinstance(sources, list) and sources:
            default_source = snapshot.get("default_source")
            source_names = {
                node.get("name")
                for node in sources
                if isinstance(node, dict) and isinstance(node.get("name"), str)
            }
            h.expect(
                isinstance(default_source, str) and default_source in source_names,
                f"audio.default_source resolves to a known source: {default_source!r}",
                f"audio.default_source does not resolve to a known source: {default_source!r}",
            )

        streams = snapshot.get("streams")
        if isinstance(streams, list):
            h.ok(f"audio.streams is a list (len={len(streams)})")
            for idx, stream in enumerate(streams):
                if isinstance(stream, dict):
                    assert_dict_keys(h, stream, STREAM_REQUIRED, f"audio.streams[{idx}]")
                else:
                    h.error(f"audio.streams[{idx}] is not a map: {stream!r}")
        else:
            h.error(f"audio.streams is not a list: {snapshot!r}")
        client.unsub("audio")
        client.drain_async()
        return snapshot
    finally:
        client.close()


def _negative_path_tests(h: Harness, socket_path: str, timeout: float) -> None:
    client, _ack = connect_and_hello(h, socket_path, timeout, "audio")
    try:
        for action in ["no_such_action", "set_volume", "set_mute", "set_default_sink", "set_stream_volume"]:
            code = "E_ACTION_UNKNOWN" if action == "no_such_action" else "E_ACTION_PAYLOAD"
            reply = client.req("audio", action, {})
            if expect_envelope(h, reply, kind=ERR, topic="audio", code=code):
                h.ok(f"audio {action} negative-path test returned {code}")
    finally:
        client.close()


def _mutate_tests(h: Harness, socket_path: str, timeout: float, snapshot: dict[str, Any], args: argparse.Namespace) -> None:
    sinks = snapshot.get("sinks")
    if not isinstance(sinks, list) or not sinks:
        h.warn("skipping audio mutate tests: no sink found")
        return

    if args.sink_id is not None:
        sink = next((s for s in sinks if isinstance(s, dict) and s.get("id") == args.sink_id), None)
    else:
        sink = next((s for s in sinks if isinstance(s, dict)), None)
    if sink is None:
        h.warn("skipping audio mutate tests: requested sink not found")
        return

    sink_id = sink["id"]
    volume = args.volume_pct if args.volume_pct is not None else sink.get("volume_pct")
    muted = (args.muted == "true") if args.muted is not None else sink.get("muted")

    client, _ack = connect_and_hello(h, socket_path, timeout, "audio")
    try:
        reply = client.req("audio", "set_volume", {"sink_id": sink_id, "volume_pct": volume})
        expect_rep_or_warn_service_err(h, reply, "audio", f"audio set_volume sink_id={sink_id} volume_pct={volume}")

        reply = client.req("audio", "set_mute", {"sink_id": sink_id, "muted": muted})
        expect_rep_or_warn_service_err(h, reply, "audio", f"audio set_mute sink_id={sink_id} muted={muted}")

        reply = client.req("audio", "set_default_sink", {"sink_id": sink_id})
        expect_rep_or_warn_service_err(h, reply, "audio", f"audio set_default_sink sink_id={sink_id}")

        streams = snapshot.get("streams")
        if not isinstance(streams, list) or not streams:
            h.warn("skipping audio stream mutate test: no active stream found")
            return

        if args.stream_id is not None:
            stream = next((s for s in streams if isinstance(s, dict) and s.get("id") == args.stream_id), None)
        else:
            stream = next((s for s in streams if isinstance(s, dict)), None)
        if stream is None:
            h.warn("skipping audio stream mutate test: requested stream not found")
            return

        stream_id = stream["id"]
        stream_volume = args.volume_pct if args.volume_pct is not None else stream.get("volume_pct")
        reply = client.req("audio", "set_stream_volume", {"stream_id": stream_id, "volume_pct": stream_volume})
        expect_rep_or_warn_service_err(
            h,
            reply,
            "audio",
            f"audio set_stream_volume stream_id={stream_id} volume_pct={stream_volume}",
        )
    finally:
        client.close()


def run() -> int:
    parser = argparse.ArgumentParser(description="Manual tests for qsov audio service")
    add_common_args(parser)
    parser.add_argument("--sink-id", type=int, default=None, help="sink id for mutate tests")
    parser.add_argument("--stream-id", type=int, default=None, help="stream id for mutate tests")
    parser.add_argument("--volume-pct", type=int, default=None, help="volume for mutate test; default=current volume")
    parser.add_argument(
        "--muted",
        choices=["true", "false"],
        default=None,
        help="mute state for mutate test; default=current sink mute state",
    )
    args = parser.parse_args()

    h = Harness("audio", strict=args.strict)
    socket_path = choose_socket(args)

    snapshot = _read_snapshot(h, socket_path, args.timeout)
    _negative_path_tests(h, socket_path, args.timeout)

    if args.mutate:
        if isinstance(snapshot, dict):
            _mutate_tests(h, socket_path, args.timeout, snapshot, args)
        else:
            h.warn("skipping audio mutate tests: snapshot unavailable")
    else:
        h.warn("mutating audio tests skipped; rerun with --mutate")

    return h.finish()


if __name__ == "__main__":
    main_guard(run)
