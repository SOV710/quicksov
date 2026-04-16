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
    expect_rep_or_warn_service_err,
    main_guard,
    maybe_warn_unavailable,
)

REQUIRED = ["workspaces", "focused_window"]
WORKSPACE_REQUIRED = ["idx", "name", "output", "focused", "windows"]
WINDOW_REQUIRED = ["id", "display_name", "app_id", "title"]


def run() -> int:
    parser = argparse.ArgumentParser(description="Manual tests for qsov niri service")
    add_common_args(parser)
    parser.add_argument(
        "--workspace-idx",
        type=int,
        default=None,
        help="workspace index for mutate focus test",
    )
    parser.add_argument(
        "--action-name",
        default=None,
        help="niri action name for run_action mutate test",
    )
    parser.add_argument(
        "--action-args",
        default=None,
        help="JSON-ish string payload for run_action mutate test",
    )
    args = parser.parse_args()

    h = Harness("niri", strict=args.strict)
    socket_path = choose_socket(args)
    client, _ack = connect_and_hello(h, socket_path, args.timeout, "niri")
    try:
        sub = client.sub("niri")
        env = expect_envelope(h, sub, kind=PUB, topic="niri")
        snapshot = None
        focus_idx = args.workspace_idx
        if env and assert_dict_keys(h, env.get("payload"), REQUIRED, "niri snapshot"):
            snapshot = env["payload"]
            maybe_warn_unavailable(h, "niri", snapshot)
            workspaces = snapshot.get("workspaces")
            if isinstance(workspaces, list):
                h.ok(f"niri.workspaces is a list (len={len(workspaces)})")
                for idx, ws in enumerate(workspaces):
                    if isinstance(ws, dict):
                        assert_dict_keys(
                            h, ws, WORKSPACE_REQUIRED, f"niri.workspaces[{idx}]"
                        )
                        if (
                            focus_idx is None
                            and ws.get("focused") is True
                            and isinstance(ws.get("idx"), int)
                        ):
                            focus_idx = ws["idx"]
                    else:
                        h.error(f"niri.workspaces[{idx}] is not a map: {ws!r}")
            else:
                h.error(f"niri.workspaces is not a list: {snapshot!r}")
            fw = snapshot.get("focused_window")
            if fw is None:
                h.ok("niri.focused_window is null")
            elif isinstance(fw, dict):
                assert_dict_keys(h, fw, WINDOW_REQUIRED, "niri.focused_window")
            else:
                h.error(f"niri.focused_window invalid: {fw!r}")
        client.unsub("niri")

        bad_action = client.req("niri", "no_such_action", {})
        if expect_envelope(
            h, bad_action, kind=ERR, topic="niri", code="E_ACTION_UNKNOWN"
        ):
            h.ok("niri unknown action returns E_ACTION_UNKNOWN")

        bad_focus = client.req("niri", "focus_workspace", {})
        if expect_envelope(
            h, bad_focus, kind=ERR, topic="niri", code="E_ACTION_PAYLOAD"
        ):
            h.ok("niri focus_workspace {} returns E_ACTION_PAYLOAD")

        bad_run = client.req("niri", "run_action", {})
        if expect_envelope(h, bad_run, kind=ERR, topic="niri", code="E_ACTION_PAYLOAD"):
            h.ok("niri run_action {} returns E_ACTION_PAYLOAD")

        if args.mutate:
            if focus_idx is not None:
                reply = client.req("niri", "focus_workspace", {"idx": focus_idx})
                expect_rep_or_warn_service_err(
                    h, reply, "niri", f"niri focus_workspace {{idx:{focus_idx}}}"
                )
            else:
                h.warn(
                    "skipping niri focus_workspace mutate test: no workspace idx available"
                )

            if args.action_name:
                payload = {"action": args.action_name}
                if args.action_args is not None:
                    payload["args"] = args.action_args
                reply = client.req("niri", "run_action", payload)
                expect_rep_or_warn_service_err(
                    h, reply, "niri", f"niri run_action {payload!r}"
                )
            else:
                h.warn(
                    "skipping niri run_action test: provide --action-name with --mutate"
                )
        else:
            h.warn("mutating niri tests skipped; rerun with --mutate")
    finally:
        client.close()

    return h.finish()


if __name__ == "__main__":
    main_guard(run)
