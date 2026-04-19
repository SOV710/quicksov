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
    "fallback_source",
    "sources",
    "views",
    "transition",
    "renderer",
]
ENTRY_REQUIRED = ["path", "name", "kind"]
SOURCE_REQUIRED = ["id", "path", "name", "kind", "loop", "mute"]
VIEW_REQUIRED = ["output", "source", "fit", "crop"]
TRANSITION_REQUIRED = ["type", "duration_ms"]
RENDERER_REQUIRED = [
    "process",
    "backend",
    "status",
    "pid",
    "last_error",
    "decode_backend_order",
    "present_mode",
    "vsync",
    "video_audio",
]
AVAILABILITY_VALUES = {"ready", "empty", "unavailable"}
REASON_VALUES = {"none", "directory_missing", "permission_denied", "scan_failed"}
KIND_VALUES = {"image", "video"}
FIT_VALUES = {"cover"}
RENDERER_STATUS_VALUES = {"starting", "running", "error"}


def validate_entry(h: Harness, entry: Any, label: str) -> bool:
    if not assert_dict_keys(h, entry, ENTRY_REQUIRED, label):
        return False
    assert isinstance(entry, dict)
    if entry.get("kind") in KIND_VALUES:
        h.ok(f"{label}.kind enum is valid ({entry.get('kind')})")
    else:
        h.error(f"{label}.kind invalid: {entry!r}")
    return True


def validate_source(h: Harness, source: Any, label: str) -> bool:
    if not assert_dict_keys(h, source, SOURCE_REQUIRED, label):
        return False
    assert isinstance(source, dict)
    if source.get("kind") in KIND_VALUES:
        h.ok(f"{label}.kind enum is valid ({source.get('kind')})")
    else:
        h.error(f"{label}.kind invalid: {source!r}")
    for key in ("loop", "mute"):
        if isinstance(source.get(key), bool):
            h.ok(f"{label}.{key} is boolean ({source.get(key)})")
        else:
            h.error(f"{label}.{key} invalid: {source!r}")
    return True


def validate_view(h: Harness, view: Any, label: str) -> bool:
    if not assert_dict_keys(h, view, VIEW_REQUIRED, label):
        return False
    assert isinstance(view, dict)
    fit = view.get("fit")
    if fit in FIT_VALUES:
        h.ok(f"{label}.fit enum is valid ({fit})")
    else:
        h.error(f"{label}.fit invalid: {view!r}")

    crop = view.get("crop")
    if crop is None:
        h.ok(f"{label}.crop is null")
    elif assert_dict_keys(h, crop, ["x", "y", "width", "height"], f"{label}.crop"):
        assert isinstance(crop, dict)
        coords = [crop.get("x"), crop.get("y"), crop.get("width"), crop.get("height")]
        if all(isinstance(v, (int, float)) for v in coords):
            h.ok(f"{label}.crop has numeric normalized coordinates")
        else:
            h.error(f"{label}.crop invalid: {crop!r}")
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

    fallback_source = payload.get("fallback_source")
    if fallback_source is None:
        h.warn("wallpaper.fallback_source is null")
    elif isinstance(fallback_source, str):
        h.ok(f"wallpaper.fallback_source is a string ({fallback_source})")
    else:
        h.error(f"wallpaper.fallback_source invalid: {payload!r}")

    sources = payload.get("sources")
    if isinstance(sources, dict):
        h.ok(f"wallpaper.sources is an object (len={len(sources)})")
        if sources:
            first_key = sorted(sources.keys())[0]
            validate_source(h, sources[first_key], f"wallpaper.sources[{first_key!r}]")
    else:
        h.error(f"wallpaper.sources invalid: {payload!r}")

    views = payload.get("views")
    if isinstance(views, dict):
        h.ok(f"wallpaper.views is an object (len={len(views)})")
        if views:
            first_key = sorted(views.keys())[0]
            validate_view(h, views[first_key], f"wallpaper.views[{first_key!r}]")
    else:
        h.error(f"wallpaper.views invalid: {payload!r}")

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

    renderer = payload.get("renderer")
    if assert_dict_keys(h, renderer, RENDERER_REQUIRED, "wallpaper.renderer"):
        assert isinstance(renderer, dict)
        if renderer.get("status") in RENDERER_STATUS_VALUES:
            h.ok(f"wallpaper.renderer.status enum is valid ({renderer.get('status')})")
        else:
            h.error(f"wallpaper.renderer.status invalid: {renderer!r}")
        if isinstance(renderer.get("decode_backend_order"), list):
            h.ok("wallpaper.renderer.decode_backend_order is a list")
        else:
            h.error(f"wallpaper.renderer.decode_backend_order invalid: {renderer!r}")
        for key in ("vsync", "video_audio"):
            if isinstance(renderer.get(key), bool):
                h.ok(f"wallpaper.renderer.{key} is boolean ({renderer.get(key)})")
            else:
                h.error(f"wallpaper.renderer.{key} invalid: {renderer!r}")
        pid = renderer.get("pid")
        if pid is None or isinstance(pid, int):
            h.ok(f"wallpaper.renderer.pid shape is valid ({pid!r})")
        else:
            h.error(f"wallpaper.renderer.pid invalid: {renderer!r}")
        last_error = renderer.get("last_error")
        if last_error is None or isinstance(last_error, str):
            h.ok(f"wallpaper.renderer.last_error shape is valid ({last_error!r})")
        else:
            h.error(f"wallpaper.renderer.last_error invalid: {renderer!r}")

    if availability == "ready" and not isinstance(sources, dict):
        h.error("wallpaper.sources missing while availability=ready")
    elif availability != "ready":
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

        bad_payload = client.req("wallpaper", "set_output_path", {})
        if expect_envelope(h, bad_payload, kind=ERR, topic="wallpaper", code="E_ACTION_PAYLOAD"):
            h.ok("wallpaper set_output_path {} returns E_ACTION_PAYLOAD")

        refresh = client.req("wallpaper", "refresh", {})
        expect_rep_or_warn_service_err(h, refresh, "wallpaper", "wallpaper refresh {}")

        if args.mutate:
            if not isinstance(snapshot, dict) or snapshot.get("availability") != "ready":
                h.warn("skipping wallpaper mutate tests: service is not in ready state")
            else:
                output = "TEST-OUTPUT"
                sources = snapshot.get("sources") or {}
                entries = snapshot.get("entries") or []
                if isinstance(sources, dict) and sources:
                    first_source_id = sorted(sources.keys())[0]
                    reply = client.req(
                        "wallpaper",
                        "set_output_source",
                        {"output": output, "source": first_source_id},
                    )
                    expect_rep_or_warn_service_err(
                        h,
                        reply,
                        "wallpaper",
                        f"wallpaper set_output_source {{output:{output!r}, source:{first_source_id!r}}}",
                    )
                else:
                    h.warn("skipping set_output_source mutate test: no sources available")

                if isinstance(entries, list) and entries:
                    first_path = entries[0].get("path")
                    if isinstance(first_path, str) and first_path:
                        reply = client.req(
                            "wallpaper",
                            "set_output_path",
                            {"output": output, "path": first_path},
                        )
                        expect_rep_or_warn_service_err(
                            h,
                            reply,
                            "wallpaper",
                            f"wallpaper set_output_path {{output:{output!r}, path:{first_path!r}}}",
                        )
                    else:
                        h.warn("skipping set_output_path mutate test: first entry path missing")
                else:
                    h.warn("skipping set_output_path mutate test: no entries available")

                for action in ("next_output", "prev_output"):
                    reply = client.req("wallpaper", action, {"output": output})
                    expect_rep_or_warn_service_err(
                        h,
                        reply,
                        "wallpaper",
                        f"wallpaper {action} {{output:{output!r}}}",
                    )

                reply = client.req(
                    "wallpaper",
                    "set_output_crop",
                    {
                        "output": output,
                        "crop": {"x": 0.0, "y": 0.0, "width": 0.5, "height": 1.0},
                    },
                )
                expect_rep_or_warn_service_err(
                    h,
                    reply,
                    "wallpaper",
                    f"wallpaper set_output_crop {{output:{output!r}, crop:<half-left>}}",
                )
        else:
            h.warn("valid wallpaper mutation tests skipped; rerun with --mutate")
    finally:
        client.close()

    return h.finish()


if __name__ == "__main__":
    main_guard(run)
