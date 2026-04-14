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
    main_guard,
)

REQUIRED = ["unread_count", "history"]
HISTORY_REQUIRED = ["id", "app_name", "summary", "body", "icon", "urgency", "timestamp", "actions"]


def run() -> int:
    parser = argparse.ArgumentParser(description="Manual tests for qsov notification service")
    add_common_args(parser)
    parser.add_argument("--id", type=int, default=None, help="notification id for mutate tests")
    parser.add_argument("--action-id", default=None, help="notification action id for invoke_action mutate test")
    parser.add_argument("--dismiss-first", action="store_true", help="dismiss the selected notification during --mutate")
    parser.add_argument("--dismiss-all", action="store_true", help="dismiss all notifications during --mutate")
    parser.add_argument("--mark-read-all", action="store_true", help="mark all notifications read during --mutate")
    args = parser.parse_args()

    h = Harness("notification", strict=args.strict)
    socket_path = choose_socket(args)
    client, _ack = connect_and_hello(h, socket_path, args.timeout, "notification")
    try:
        sub = client.sub("notification")
        env = expect_envelope(h, sub, kind=PUB, topic="notification")
        snapshot = None
        selected_id = args.id
        selected_action_id = args.action_id
        if env and assert_dict_keys(h, env.get("payload"), REQUIRED, "notification snapshot"):
            snapshot = env["payload"]
            history = snapshot.get("history")
            if isinstance(snapshot.get("unread_count"), int):
                h.ok("notification.unread_count is an int")
            else:
                h.error(f"notification.unread_count invalid: {snapshot!r}")
            if isinstance(history, list):
                h.ok(f"notification.history is a list (len={len(history)})")
                for idx, item in enumerate(history):
                    if isinstance(item, dict):
                        assert_dict_keys(h, item, HISTORY_REQUIRED, f"notification.history[{idx}]")
                    else:
                        h.error(f"notification.history[{idx}] is not a map: {item!r}")
                if selected_id is None and history and isinstance(history[0], dict) and isinstance(history[0].get("id"), int):
                    selected_id = history[0]["id"]
                    actions = history[0].get("actions")
                    if selected_action_id is None and isinstance(actions, list) and actions and isinstance(actions[0], dict):
                        action_id = actions[0].get("id")
                        if isinstance(action_id, str):
                            selected_action_id = action_id
            else:
                h.error(f"notification.history is not a list: {snapshot!r}")
        client.unsub("notification")

        bad_action = client.req("notification", "no_such_action", {})
        if expect_envelope(h, bad_action, kind=ERR, topic="notification", code="E_ACTION_UNKNOWN"):
            h.ok("notification unknown action returns E_ACTION_UNKNOWN")

        for action in ["dismiss", "invoke_action"]:
            reply = client.req("notification", action, {})
            if expect_envelope(h, reply, kind=ERR, topic="notification", code="E_ACTION_PAYLOAD"):
                h.ok(f"notification {action} {{}} returns E_ACTION_PAYLOAD")

        if args.mutate:
            if args.mark_read_all:
                reply = client.req("notification", "mark_read", {})
                env = expect_envelope(h, reply, kind=REP, topic="notification")
                if env:
                    h.ok("notification mark_read {} returned REP")
            else:
                h.warn("mark_read-all test skipped; pass --mark-read-all with --mutate to run it")

            if args.dismiss_all:
                reply = client.req("notification", "dismiss_all", {})
                env = expect_envelope(h, reply, kind=REP, topic="notification")
                if env:
                    h.ok("notification dismiss_all {} returned REP")
            else:
                h.warn("dismiss_all test skipped; pass --dismiss-all with --mutate to run it")

            if selected_id is not None:
                reply = client.req("notification", "mark_read", {"id": selected_id})
                env = expect_envelope(h, reply, kind=REP, topic="notification")
                if env:
                    h.ok(f"notification mark_read {{id:{selected_id}}} returned REP")

                if selected_action_id is not None:
                    reply = client.req(
                        "notification",
                        "invoke_action",
                        {"id": selected_id, "action_id": selected_action_id},
                    )
                    env = expect_envelope(h, reply, kind=REP, topic="notification")
                    if env:
                        h.ok(
                            f"notification invoke_action {{id:{selected_id}, action_id:{selected_action_id!r}}} returned REP"
                        )
                else:
                    h.warn("skipping invoke_action test: no action id available")

                if args.dismiss_first:
                    reply = client.req("notification", "dismiss", {"id": selected_id})
                    env = expect_envelope(h, reply, kind=REP, topic="notification")
                    if env:
                        h.ok(f"notification dismiss {{id:{selected_id}}} returned REP")
                else:
                    h.warn("dismiss-first test skipped; pass --dismiss-first with --mutate to run it")
            else:
                h.warn("skipping id-based notification mutate tests: no notification id available")
        else:
            h.warn("mutating notification tests skipped; rerun with --mutate")
    finally:
        client.close()

    return h.finish()


if __name__ == "__main__":
    main_guard(run)
