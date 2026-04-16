#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import socket
import sys
from typing import Any


def default_niri_socket(explicit: str | None = None) -> str:
    if explicit:
        return explicit
    env_socket = os.getenv("NIRI_SOCKET")
    if env_socket:
        return env_socket
    return f"/run/user/{os.getuid()}/niri/socket"


def request_niri(socket_path: str, request: str, timeout: float) -> Any:
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
        sock.settimeout(timeout)
        sock.connect(socket_path)
        sock.sendall((request + "\n").encode("utf-8"))

        data = bytearray()
        while True:
            chunk = sock.recv(4096)
            if not chunk:
                break
            data.extend(chunk)
            if b"\n" in chunk:
                break

    if not data:
        raise RuntimeError("empty response from niri socket")

    line = data.split(b"\n", 1)[0].decode("utf-8", errors="strict")
    return json.loads(line)


def parse_focused_window(payload: Any) -> dict[str, Any] | None:
    if not isinstance(payload, dict):
        raise RuntimeError(f"niri reply is not a JSON object: {payload!r}")

    ok = payload.get("Ok")
    if not isinstance(ok, dict):
        err = payload.get("Err")
        raise RuntimeError(f"niri returned non-Ok reply: {err!r}")

    window = ok.get("FocusedWindow")
    if window is None:
        return None
    if not isinstance(window, dict):
        raise RuntimeError(f"FocusedWindow is not a JSON object: {window!r}")
    return window


def validate_window(window: dict[str, Any]) -> list[str]:
    errors: list[str] = []

    if not isinstance(window.get("id"), int):
        errors.append(f"field 'id' invalid: {window.get('id')!r}")
    if not isinstance(window.get("app_id"), str) or not window.get("app_id"):
        errors.append(f"field 'app_id' invalid: {window.get('app_id')!r}")
    if not isinstance(window.get("title"), str):
        errors.append(f"field 'title' invalid: {window.get('title')!r}")

    return errors


def run() -> int:
    parser = argparse.ArgumentParser(
        description="Check whether a running niri session returns the current FocusedWindow"
    )
    parser.add_argument(
        "--socket",
        default=None,
        help="override niri UDS path; default is $NIRI_SOCKET or /run/user/$UID/niri/socket",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=3.0,
        help="socket timeout in seconds",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="print the full FocusedWindow object as JSON",
    )
    args = parser.parse_args()

    socket_path = default_niri_socket(args.socket)
    print(f"INFO  [niri-focus] socket={socket_path}")

    try:
        payload = request_niri(socket_path, '"FocusedWindow"', args.timeout)
    except (OSError, json.JSONDecodeError, RuntimeError) as exc:
        print(f"ERROR [niri-focus] request failed: {exc}", file=sys.stderr)
        return 1

    try:
        window = parse_focused_window(payload)
    except RuntimeError as exc:
        print(f"ERROR [niri-focus] invalid reply: {exc}", file=sys.stderr)
        return 1

    if window is None:
        print("WARN  [niri-focus] FocusedWindow is null; no currently focused window")
        return 2

    errors = validate_window(window)
    if errors:
        for err in errors:
            print(f"ERROR [niri-focus] {err}", file=sys.stderr)
        return 1

    print(
        "PASS  [niri-focus] "
        f"id={window['id']} app_id={window['app_id']!r} title={window['title']!r}"
    )

    if args.json:
        print(json.dumps(window, ensure_ascii=False, indent=2, sort_keys=True))

    return 0


if __name__ == "__main__":
    raise SystemExit(run())
