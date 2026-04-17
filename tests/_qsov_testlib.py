#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import socket
import sys
from pathlib import Path
from typing import Any, Iterable

REQ = 0
REP = 1
ERR = 2
PUB = 3
ONESHOT = 4
SUB = 5
UNSUB = 6
PROTO_VERSION = "qsov/1"


class QsovError(RuntimeError):
    pass


class Harness:
    def __init__(self, name: str, strict: bool = False) -> None:
        self.name = name
        self.strict = strict
        self.passes = 0
        self.warnings = 0
        self.errors = 0

    def ok(self, msg: str) -> None:
        self.passes += 1
        print(f"PASS  [{self.name}] {msg}")

    def warn(self, msg: str) -> None:
        self.warnings += 1
        print(f"WARN  [{self.name}] {msg}")

    def error(self, msg: str) -> None:
        self.errors += 1
        print(f"ERROR [{self.name}] {msg}")

    def expect(self, cond: bool, ok_msg: str, err_msg: str) -> bool:
        if cond:
            self.ok(ok_msg)
            return True
        self.error(err_msg)
        return False

    def finish(self) -> int:
        print(f"SUMMARY [{self.name}] pass={self.passes} warn={self.warnings} error={self.errors}")
        if self.errors:
            return 1
        if self.strict and self.warnings:
            return 2
        return 0


class QsovClient:
    def __init__(self, socket_path: str, timeout: float = 3.0) -> None:
        self.socket_path = socket_path
        self.timeout = timeout
        self.sock: socket.socket | None = None
        self._next_id = 1
        self._recv_buffer = bytearray()

    def connect(self) -> None:
        self.close()
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(self.timeout)
        self.sock.connect(self.socket_path)
        self._recv_buffer.clear()

    def close(self) -> None:
        if self.sock is not None:
            try:
                self.sock.close()
            except OSError:
                pass
            self.sock = None

    def __enter__(self) -> "QsovClient":
        self.connect()
        return self

    def __exit__(self, exc_type, exc, tb) -> None:
        self.close()

    def _ensure_sock(self) -> socket.socket:
        if self.sock is None:
            raise QsovError("socket is not connected")
        return self.sock

    def send_obj(self, obj: Any) -> None:
        raw = json.dumps(obj, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        self._ensure_sock().sendall(raw + b"\n")

    def recv_obj(self) -> Any:
        sock = self._ensure_sock()
        while b"\n" not in self._recv_buffer:
            chunk = sock.recv(4096)
            if not chunk:
                if self._recv_buffer:
                    break
                raise QsovError("socket closed while receiving line")
            self._recv_buffer.extend(chunk)

        line, sep, rest = bytes(self._recv_buffer).partition(b"\n")
        if sep:
            self._recv_buffer = bytearray(rest)
        else:
            self._recv_buffer.clear()
        if not line:
            raise QsovError("received empty line")

        try:
            return json.loads(line.decode("utf-8"))
        except json.JSONDecodeError as exc:
            raise QsovError(f"failed to decode JSON line: {exc}") from exc

    def hello(
        self,
        client_name: str = "manual-tests",
        client_version: str = "0.1",
        proto_version: str = PROTO_VERSION,
    ) -> Any:
        self.send_obj(
            {
                "proto_version": proto_version,
                "client_name": client_name,
                "client_version": client_version,
            }
        )
        return self.recv_obj()

    def send_envelope(
        self,
        kind: int,
        topic: str,
        action: str = "",
        payload: Any = None,
        msg_id: int | None = None,
    ) -> int:
        if msg_id is None:
            msg_id = self._next_id
            self._next_id += 1
        self.send_obj(
            {
                "id": msg_id,
                "kind": kind,
                "topic": topic,
                "action": action,
                "payload": payload,
            }
        )
        return msg_id

    def recv_reply(self, msg_id: int, *, skip_async: bool = True, max_skips: int = 64) -> Any:
        skipped = 0
        while True:
            msg = self.recv_obj()
            if not isinstance(msg, dict):
                raise QsovError(f"reply is not a map: {msg!r}")
            incoming_id = msg.get("id")
            kind = msg.get("kind")
            if incoming_id == msg_id:
                return msg
            if skip_async and incoming_id == 0 and kind in {PUB, ONESHOT}:
                skipped += 1
                if skipped > max_skips:
                    raise QsovError(
                        f"too many async messages while waiting for reply id={msg_id}; last={msg!r}"
                    )
                continue
            raise QsovError(f"reply id mismatch: expected {msg_id}, got {incoming_id}; msg={msg!r}")

    def req(self, topic: str, action: str, payload: Any = None, msg_id: int | None = None) -> Any:
        msg_id = self.send_envelope(REQ, topic, action, payload, msg_id)
        return self.recv_reply(msg_id)

    def oneshot(self, topic: str, action: str, payload: Any = None) -> None:
        self.send_envelope(ONESHOT, topic, action, payload)

    def sub(self, topic: str, msg_id: int = 0) -> Any:
        self.send_envelope(SUB, topic, payload=None, msg_id=msg_id)
        return self.recv_obj()

    def unsub(self, topic: str, msg_id: int = 0) -> None:
        self.send_envelope(UNSUB, topic, payload=None, msg_id=msg_id)

    def drain_async(self, *, timeout: float = 0.05, limit: int = 64) -> list[Any]:
        sock = self._ensure_sock()
        original_timeout = sock.gettimeout()
        drained: list[Any] = []
        try:
            sock.settimeout(timeout)
            for _ in range(limit):
                try:
                    msg = self.recv_obj()
                except TimeoutError:
                    break
                except socket.timeout:
                    break
                drained.append(msg)
        finally:
            sock.settimeout(original_timeout)
        return drained


def default_socket_path(explicit: str | None = None) -> str:
    if explicit:
        return explicit
    if os.getenv("QSOV_SOCKET"):
        return os.environ["QSOV_SOCKET"]
    xdg_runtime = os.getenv("XDG_RUNTIME_DIR")
    if xdg_runtime:
        return str(Path(xdg_runtime) / "quicksov" / "daemon.sock")
    return f"/run/user/{os.getuid()}/quicksov/daemon.sock"


def add_common_args(parser: argparse.ArgumentParser, *, mutate: bool = True) -> None:
    parser.add_argument("--socket", default=None, help="override daemon UDS path")
    parser.add_argument("--timeout", type=float, default=3.0, help="socket timeout in seconds")
    parser.add_argument(
        "--strict",
        action="store_true",
        help="treat WARN as non-zero exit status (exit 2)",
    )
    if mutate:
        parser.add_argument(
            "--mutate",
            action="store_true",
            help="run valid state-changing actions in addition to read-only and ERR-path checks",
        )


def connect_and_hello(
    harness: Harness,
    socket_path: str,
    timeout: float,
    capability: str,
) -> tuple[QsovClient, dict[str, Any]]:
    client = QsovClient(socket_path, timeout)
    client.connect()
    ack = client.hello()
    if not isinstance(ack, dict):
        client.close()
        raise QsovError(f"hello ack is not a map: {ack!r}")
    capabilities = ack.get("capabilities")
    if isinstance(capabilities, list) and capability in capabilities:
        harness.ok(f"HelloAck contains capability {capability!r}")
    else:
        harness.error(f"HelloAck missing capability {capability!r}: {ack!r}")
    return client, ack


def expect_envelope(
    harness: Harness,
    msg: Any,
    *,
    kind: int,
    topic: str,
    code: str | None = None,
) -> dict[str, Any] | None:
    if not isinstance(msg, dict):
        harness.error(f"message is not a map: {msg!r}")
        return None
    if msg.get("kind") != kind:
        harness.error(f"unexpected kind: expected {kind}, got {msg.get('kind')}; msg={msg!r}")
        return None
    if msg.get("topic") != topic:
        harness.error(f"unexpected topic: expected {topic!r}, got {msg.get('topic')!r}")
        return None
    if code is not None:
        payload = msg.get("payload")
        if not isinstance(payload, dict):
            harness.error(f"ERR payload is not a map: {payload!r}")
            return None
        actual = payload.get("code")
        if actual != code:
            harness.error(f"unexpected error code: expected {code}, got {actual}; msg={msg!r}")
            return None
    return msg


def assert_dict_keys(harness: Harness, payload: Any, keys: Iterable[str], label: str) -> bool:
    if not isinstance(payload, dict):
        harness.error(f"{label} is not a map: {payload!r}")
        return False
    missing = [key for key in keys if key not in payload]
    if missing:
        harness.error(f"{label} missing keys: {missing}; payload={payload!r}")
        return False
    harness.ok(f"{label} contains required keys: {', '.join(keys)}")
    return True


def get_map_value(payload: dict[str, Any], key: str, expected_type: type | tuple[type, ...], harness: Harness) -> Any:
    value = payload.get(key)
    if not isinstance(value, expected_type):
        harness.error(
            f"field {key!r} has wrong type: expected {expected_type}, got {type(value).__name__}; value={value!r}"
        )
        return None
    harness.ok(f"field {key!r} has type {type(value).__name__}")
    return value


def maybe_warn_unavailable(harness: Harness, service: str, payload: dict[str, Any]) -> None:
    if service == "battery" and not payload.get("present", True):
        harness.warn("battery.present=false; no battery or battery backend unavailable")
    elif service == "net.wifi" and payload.get("state") == "unknown":
        harness.warn("net.wifi.state=unknown; likely wpa_supplicant unavailable or permission denied")
    elif service == "bluetooth":
        if payload.get("available") is False:
            harness.warn("bluetooth.available=false; no Bluetooth adapter present")
        elif payload.get("powered") is False:
            harness.warn("bluetooth powered off")
    elif service == "audio" and not payload.get("sinks") and not payload.get("sources"):
        harness.warn("audio has no sinks/sources; PipeWire may be unavailable")
    elif service == "mpris" and not payload.get("players"):
        harness.warn("mpris has no players; media control mutate tests will be skipped")
    elif service == "niri" and not payload.get("workspaces"):
        harness.warn("niri has no workspaces; compositor IPC may be unavailable")
    elif service == "weather":
        status = payload.get("status")
        error = payload.get("error")
        if status in {"init_failed", "refresh_failed"}:
            if isinstance(error, dict):
                harness.warn(
                    f"weather.status={status}; {error.get('kind', 'unknown')}: {error.get('message', '')}"
                )
            else:
                harness.warn(f"weather.status={status}; error details unavailable")
        elif status == "loading" and payload.get("last_success_at") is None:
            harness.warn("weather is still loading and has no successful snapshot yet")


def choose_socket(args: argparse.Namespace) -> str:
    return default_socket_path(args.socket)


def dump_jsonish(value: Any) -> str:
    import json

    try:
        return json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True)
    except TypeError:
        return repr(value)


def main_guard(func) -> None:
    try:
        raise SystemExit(func())
    except KeyboardInterrupt:
        print("INTERRUPTED")
        raise SystemExit(130)
    except (OSError, QsovError, json.JSONDecodeError) as exc:
        print(f"FATAL {exc}", file=sys.stderr)
        raise SystemExit(1)


def expect_rep_or_warn_service_err(harness: Harness, msg: Any, topic: str, label: str) -> bool:
    if not isinstance(msg, dict):
        harness.error(f"{label}: message is not a map: {msg!r}")
        return False
    kind = msg.get("kind")
    if kind == REP and msg.get("topic") == topic:
        harness.ok(f"{label}: got REP")
        return True
    if kind == ERR and msg.get("topic") == topic:
        payload = msg.get("payload")
        code = payload.get("code") if isinstance(payload, dict) else None
        if code in {"E_SERVICE_INTERNAL", "E_SERVICE_UNAVAILABLE"}:
            harness.warn(f"{label}: service returned {code}; backend likely unavailable: {msg!r}")
            return False
    harness.error(f"{label}: unexpected reply: {msg!r}")
    return False
