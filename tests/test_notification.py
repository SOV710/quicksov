#!/usr/bin/env python3
from __future__ import annotations

import argparse

from _qsov_testlib import (
    REQ,
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

REQUIRED = ["do_not_disturb", "unread_count", "history"]
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
        client.sub_events("notification")
        immediate_events = client.drain_async(timeout=0.1, limit=1)
        if not immediate_events:
            h.ok("notification SUB_EVENTS does not emit an initial snapshot")
        else:
            h.error(f"notification SUB_EVENTS unexpectedly produced immediate messages: {immediate_events!r}")
        snapshot = None
        selected_id = args.id
        selected_action_id = args.action_id
        initial_dnd = False
        if env and assert_dict_keys(h, env.get("payload"), REQUIRED, "notification snapshot"):
            snapshot = env["payload"]
            history = snapshot.get("history")
            if isinstance(snapshot.get("do_not_disturb"), bool):
                initial_dnd = snapshot["do_not_disturb"]
                h.ok("notification.do_not_disturb is a bool")
            else:
                h.error(f"notification.do_not_disturb invalid: {snapshot!r}")
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

        for action in ["dismiss", "invoke_action", "invoke_action_and_dismiss", "set_do_not_disturb"]:
            reply = client.req("notification", action, {})
            if expect_envelope(h, reply, kind=ERR, topic="notification", code="E_ACTION_PAYLOAD"):
                h.ok(f"notification {action} {{}} returns E_ACTION_PAYLOAD")

        if args.mutate:
            for enabled in [True, False]:
                reply = client.req("notification", "set_do_not_disturb", {"enabled": enabled})
                env = expect_envelope(h, reply, kind=REP, topic="notification")
                if env:
                    h.ok(f"notification set_do_not_disturb {{enabled:{enabled}}} returned REP")

            if initial_dnd:
                reply = client.req("notification", "set_do_not_disturb", {"enabled": True})
                env = expect_envelope(h, reply, kind=REP, topic="notification")
                if env:
                    h.ok("notification set_do_not_disturb restored initial enabled state")

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

                    if not args.dismiss_first:
                        req_id = client.send_envelope(
                            REQ,
                            "notification",
                            "invoke_action_and_dismiss",
                            {"id": selected_id, "action_id": selected_action_id},
                        )
                        reply = None
                        events = []
                        for _ in range(6):
                            msg = client.recv_obj()
                            if not isinstance(msg, dict):
                                h.error(f"invoke_action_and_dismiss yielded non-map message: {msg!r}")
                                break
                            if msg.get("id") == req_id:
                                reply = msg
                            elif msg.get("kind") == PUB and msg.get("topic") == "notification":
                                events.append(msg)
                            if reply is not None and len(events) >= 2:
                                break

                        env = expect_envelope(h, reply, kind=REP, topic="notification") if reply is not None else None
                        if env:
                            h.ok(
                                "notification invoke_action_and_dismiss returned REP"
                            )
                        event_names = [event.get("action") for event in events]
                        if event_names[:2] == ["action_invoked", "closed"]:
                            h.ok("notification invoke_action_and_dismiss published action_invoked then closed")
                        else:
                            h.error(
                                "notification invoke_action_and_dismiss event order mismatch: "
                                f"{event_names!r}"
                            )
                        if len(events) >= 2 and isinstance(events[1].get("payload"), dict):
                            closed_payload = events[1]["payload"]
                            if closed_payload.get("id") == selected_id and closed_payload.get("reason") == "dismissed":
                                h.ok("notification closed event uses symbolic dismissed reason")
                            else:
                                h.error(
                                    "notification closed event payload unexpected: "
                                    f"{closed_payload!r}"
                                )
                        selected_id = None
                else:
                    h.warn("skipping invoke_action test: no action id available")

                if args.dismiss_first and selected_id is not None:
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
