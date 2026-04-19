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
)

REQUIRED = [
    "directory",
    "availability",
    "availability_reason",
    "entries",
    "current",
    "transition",
    "render",
]
ENTRY_REQUIRED = ["path", "name", "kind"]
TRANSITION_REQUIRED = ["type", "duration_ms"]
RENDER_REQUIRED = ["backend", "video_enabled", "video_audio"]
AVAILABILITY_VALUES = {"ready", "empty", "unavailable"}
REASON_VALUES = {"none", "directory_missing", "permission_denied", "scan_failed"}
KIND_VALUES = {"image", "video"}


def validate_entry(h: Harness, entry: Any, label: str) -> bool:
    if not assert_dict_keys(h, entry, ENTRY_REQUIRED, label):
        return False
    assert isinstance(entry, dict)
    if entry.get("kind") in KIND_VALUES:
        h.ok(f"{label}.kind enum is valid ({entry.get('kind')})")
    else:
        h.error(f"{label}.kind invalid: {entry!r}")
    return True


def validate_snapshot(h: Harness, payload: Any) -> dict[str, Any] | None:
    if not assert_dict_keys(h, payload, REQUIRED, "wallpaper snapshot"):
        return None
    assert isinstance(payload, dict)

    directory = payload.get("directory")
    if isinstance(directory, str) and directory:
        h.ok(f"wallpaper.directory is a non-empty string ({directory})")
    else:
        h.error(f"wallpaper.directory invalid: {payload!r}")

    availability = payload.get("availability")
    if availability in AVAILABILITY_VALUES:
        h.ok(f"wallpaper.availability enum is valid ({availability})")
    else:
        h.error(f"wallpaper.availability invalid: {payload!r}")

    reason = payload.get("availability_reason")
    if reason in REASON_VALUES:
        h.ok(f"wallpaper.availability_reason enum is valid ({reason})")
    else:
        h.error(f"wallpaper.availability_reason invalid: {payload!r}")

    entries = payload.get("entries")
    if isinstance(entries, list):
        h.ok(f"wallpaper.entries is a list (len={len(entries)})")
        if entries:
            validate_entry(h, entries[0], "wallpaper.entries[0]")
    else:
        h.error(f"wallpaper.entries invalid: {payload!r}")

    current = payload.get("current")
    if current is None:
        h.warn("wallpaper.current is null")
    else:
        validate_entry(h, current, "wallpaper.current")

    transition = payload.get("transition")
    if assert_dict_keys(h, transition, TRANSITION_REQUIRED, "wallpaper.transition"):
        assert isinstance(transition, dict)
        if transition.get("type") == "fade":
            h.ok("wallpaper.transition.type is fade")
        else:
            h.error(f"wallpaper.transition.type invalid: {transition!r}")
        duration = transition.get("duration_ms")
        if isinstance(duration, int) and duration >= 0:
            h.ok(f"wallpaper.transition.duration_ms is non-negative ({duration})")
        else:
            h.error(f"wallpaper.transition.duration_ms invalid: {transition!r}")

    render = payload.get("render")
    if assert_dict_keys(h, render, RENDER_REQUIRED, "wallpaper.render"):
        assert isinstance(render, dict)
        if render.get("backend") == "mpv":
            h.ok("wallpaper.render.backend is mpv")
        else:
            h.error(f"wallpaper.render.backend invalid: {render!r}")
        for key in ("video_enabled", "video_audio"):
            if isinstance(render.get(key), bool):
                h.ok(f"wallpaper.render.{key} is boolean ({render.get(key)})")
            else:
                h.error(f"wallpaper.render.{key} invalid: {render!r}")

    if availability == "ready" and current is None:
        h.error("wallpaper.current is null while availability=ready")
    elif availability in {"empty", "unavailable"} and current is not None:
        h.warn(f"wallpaper.current is set while availability={availability}")

    if availability != "ready":
        h.warn(f"wallpaper availability={availability}; reason={reason}")

    return payload


def run() -> int:
    parser = argparse.ArgumentParser(description="Manual tests for qsov wallpaper service")
    add_common_args(parser)
    args = parser.parse_args()

    h = Harness("wallpaper", strict=args.strict)
    socket_path = choose_socket(args)
    client, _ack = connect_and_hello(h, socket_path, args.timeout, "wallpaper")
    try:
        sub = client.sub("wallpaper")
        env = expect_envelope(h, sub, kind=PUB, topic="wallpaper")
        snapshot = None
        if env:
            snapshot = validate_snapshot(h, env.get("payload"))
        client.unsub("wallpaper")

        bad_action = client.req("wallpaper", "no_such_action", {})
        if expect_envelope(h, bad_action, kind=ERR, topic="wallpaper", code="E_ACTION_UNKNOWN"):
            h.ok("wallpaper unknown action returns E_ACTION_UNKNOWN")

        bad_payload = client.req("wallpaper", "set_path", {})
        if expect_envelope(h, bad_payload, kind=ERR, topic="wallpaper", code="E_ACTION_PAYLOAD"):
            h.ok("wallpaper set_path {} returns E_ACTION_PAYLOAD")

        refresh = client.req("wallpaper", "refresh", {})
        expect_rep_or_warn_service_err(h, refresh, "wallpaper", "wallpaper refresh {}")

        if args.mutate:
            if not isinstance(snapshot, dict) or snapshot.get("availability") != "ready":
                h.warn("skipping wallpaper mutate tests: service is not in ready state")
            else:
                current = snapshot.get("current") or {}
                current_path = current.get("path") if isinstance(current, dict) else None
                reply = client.req("wallpaper", "next", {})
                expect_rep_or_warn_service_err(h, reply, "wallpaper", "wallpaper next {}")
                reply = client.req("wallpaper", "prev", {})
                expect_rep_or_warn_service_err(h, reply, "wallpaper", "wallpaper prev {}")
                if isinstance(current_path, str) and current_path:
                    reply = client.req("wallpaper", "set_path", {"path": current_path})
                    expect_rep_or_warn_service_err(
                        h,
                        reply,
                        "wallpaper",
                        f"wallpaper set_path {{path:{current_path!r}}}",
                    )
                else:
                    h.warn("skipping wallpaper set_path mutate test: current path missing")
        else:
            h.warn("valid wallpaper mutation tests skipped; rerun with --mutate")
    finally:
        client.close()

    return h.finish()


if __name__ == "__main__":
    main_guard(run)
