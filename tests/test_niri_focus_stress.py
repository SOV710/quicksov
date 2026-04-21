#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import queue
import socket
import threading
import time
from pathlib import Path
from typing import Any

from _qsov_testlib import (
    PUB,
    Harness,
    QsovClient,
    add_common_args,
    choose_socket,
    connect_and_hello,
    expect_envelope,
    main_guard,
)


def default_niri_socket(explicit: str | None = None) -> str:
    if explicit:
        return explicit
    env_socket = os.getenv("NIRI_SOCKET")
    if env_socket:
        return env_socket
    xdg_runtime = os.getenv("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")
    candidates = sorted(Path(xdg_runtime).glob("niri*.sock"))
    if candidates:
        return str(candidates[-1])
    return str(Path(xdg_runtime) / "niri" / "socket")


def request_niri(socket_path: str, request: str, timeout: float) -> Any:
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
        sock.settimeout(timeout)
        sock.connect(socket_path)
        sock.sendall(request.encode("utf-8") + b"\n")
        data = bytearray()
        while b"\n" not in data:
            chunk = sock.recv(4096)
            if not chunk:
                break
            data.extend(chunk)
    if not data:
        raise RuntimeError("empty response from niri socket")
    line = bytes(data).split(b"\n", 1)[0]
    return json.loads(line.decode("utf-8"))


def focus_workspace_via_niri(socket_path: str, idx: int, timeout: float) -> Any:
    cmd = json.dumps(
        {"Action": {"FocusWorkspace": {"reference": {"Index": idx}}}},
        separators=(",", ":"),
    )
    return request_niri(socket_path, cmd, timeout)


def direct_workspaces_from_niri(socket_path: str, timeout: float) -> list[dict[str, Any]]:
    payload = request_niri(socket_path, '"Workspaces"', timeout)
    workspaces = (
        payload.get("Ok", {}).get("Workspaces")
        if isinstance(payload, dict)
        else None
    )
    if not isinstance(workspaces, list):
        return []
    return [ws for ws in workspaces if isinstance(ws, dict)]


def direct_focused_idx_for_output(socket_path: str, output: str, timeout: float) -> int | None:
    for ws in direct_workspaces_from_niri(socket_path, timeout):
        if ws.get("output") == output and ws.get("is_focused") is True and isinstance(ws.get("idx"), int):
            return ws["idx"]
    return None


def parse_indices(raw: str | None) -> list[int]:
    if not raw:
        return []
    out: list[int] = []
    for part in raw.split(","):
        part = part.strip()
        if not part:
            continue
        out.append(int(part))
    return out


def focused_workspaces(snapshot: dict[str, Any]) -> list[dict[str, Any]]:
    workspaces = snapshot.get("workspaces")
    if not isinstance(workspaces, list):
        return []
    return [ws for ws in workspaces if isinstance(ws, dict) and ws.get("focused") is True]


def focused_idx_for_output(snapshot: dict[str, Any], output: str) -> int | None:
    for ws in focused_workspaces(snapshot):
        if ws.get("output") == output and isinstance(ws.get("idx"), int):
            return ws["idx"]
    return None


def choose_output_and_indices(
    h: Harness,
    snapshot: dict[str, Any],
    requested_output: str | None,
    requested_indices: list[int],
) -> tuple[str, list[int]]:
    workspaces = snapshot.get("workspaces")
    if not isinstance(workspaces, list):
        raise RuntimeError("niri snapshot has no workspaces list")

    focused = focused_workspaces(snapshot)
    if requested_output:
        output = requested_output
    elif focused and isinstance(focused[0].get("output"), str):
        output = focused[0]["output"]
    else:
        outputs = [ws.get("output") for ws in workspaces if isinstance(ws, dict) and isinstance(ws.get("output"), str)]
        if not outputs:
            raise RuntimeError("cannot determine target output")
        output = sorted(outputs)[0]

    idxs = sorted(
        {
            ws["idx"]
            for ws in workspaces
            if isinstance(ws, dict)
            and ws.get("output") == output
            and isinstance(ws.get("idx"), int)
        }
    )
    if not idxs:
        raise RuntimeError(f"no workspaces found for output {output!r}")

    if requested_indices:
        chosen = requested_indices
    else:
        non_one = [idx for idx in idxs if idx != 1]
        chosen = non_one[-2:] if len(non_one) >= 2 else idxs[-2:]

    if len(chosen) < 2:
        raise RuntimeError(
            f"need at least two workspace indices on output {output!r}; available={idxs}"
        )

    h.ok(f"focus stress output={output} indices={chosen}")
    if 1 in chosen:
        h.warn("chosen indices include 1; this weakens detection of the 'jump to spot 1' bug")
    return output, chosen


class Observer(threading.Thread):
    def __init__(self, client: QsovClient, out_queue: queue.Queue[tuple[float, dict[str, Any]]]) -> None:
        super().__init__(daemon=True)
        self.client = client
        self.out_queue = out_queue
        self.stop_event = threading.Event()

    def stop(self) -> None:
        self.stop_event.set()

    def run(self) -> None:
        assert self.client.sock is not None
        self.client.sock.settimeout(0.2)
        while not self.stop_event.is_set():
            try:
                msg = self.client.recv_obj()
            except socket.timeout:
                continue
            except OSError:
                break
            if not isinstance(msg, dict):
                continue
            if msg.get("kind") != PUB or msg.get("topic") != "niri":
                continue
            payload = msg.get("payload")
            if isinstance(payload, dict):
                self.out_queue.put((time.monotonic(), payload))


def wait_for_focus(
    out_queue: queue.Queue[tuple[float, dict[str, Any]]],
    output: str,
    expected_idx: int,
    timeout: float,
) -> dict[str, Any] | None:
    deadline = time.monotonic() + timeout
    last = None
    while time.monotonic() < deadline:
        remaining = max(0.01, deadline - time.monotonic())
        try:
            _ts, snap = out_queue.get(timeout=min(0.1, remaining))
        except queue.Empty:
            continue
        last = snap
        if focused_idx_for_output(snap, output) == expected_idx:
            return snap
    return last


def stress_sender(
    *,
    socket_path: str,
    timeout: float,
    mode: str,
    niri_socket: str,
    send_queue: queue.Queue[int],
    send_delay: float,
    errors: list[str],
) -> None:
    try:
        if mode == "direct-niri":
            while True:
                try:
                    idx = send_queue.get_nowait()
                except queue.Empty:
                    break
                reply = focus_workspace_via_niri(niri_socket, idx, timeout)
                if not isinstance(reply, dict) or "Ok" not in reply:
                    errors.append(f"direct-niri focus_workspace {idx} failed: {reply!r}")
                if send_delay > 0:
                    time.sleep(send_delay)
            return

        client = QsovClient(socket_path, timeout)
        client.connect()
        try:
            client.hello(client_name="niri-focus-stress", client_version="0.1")
            while True:
                try:
                    idx = send_queue.get_nowait()
                except queue.Empty:
                    break
                if mode == "oneshot":
                    client.oneshot("niri", "focus_workspace", {"idx": idx})
                else:
                    reply = client.req("niri", "focus_workspace", {"idx": idx})
                    if not isinstance(reply, dict) or reply.get("kind") != 1:
                        errors.append(f"daemon focus_workspace {idx} failed: {reply!r}")
                if send_delay > 0:
                    time.sleep(send_delay)
        finally:
            client.close()
    except Exception as exc:  # noqa: BLE001
        errors.append(f"sender crashed: {exc}")


def run() -> int:
    parser = argparse.ArgumentParser(description="Stress test qsov niri focus snapshots")
    add_common_args(parser, mutate=False)
    parser.add_argument(
        "--mode",
        choices=["req", "oneshot", "direct-niri"],
        default="req",
        help="how to trigger workspace focus changes",
    )
    parser.add_argument(
        "--rounds",
        type=int,
        default=80,
        help="number of workspace switches per sender",
    )
    parser.add_argument(
        "--parallel",
        type=int,
        default=1,
        help="number of concurrent senders",
    )
    parser.add_argument(
        "--indices",
        default=None,
        help="comma-separated workspace indices to alternate, e.g. 3,4",
    )
    parser.add_argument(
        "--output",
        default=None,
        help="target output name; defaults to currently focused output",
    )
    parser.add_argument(
        "--send-delay-ms",
        type=float,
        default=0.0,
        help="delay between sends in milliseconds",
    )
    parser.add_argument(
        "--settle-ms",
        type=float,
        default=1200.0,
        help="extra time to keep observing after the senders finish",
    )
    parser.add_argument(
        "--niri-socket",
        default=None,
        help="override direct niri IPC socket path for --mode=direct-niri",
    )
    args = parser.parse_args()

    h = Harness("niri-focus-stress", strict=args.strict)
    socket_path = choose_socket(args)
    observer_client, _ack = connect_and_hello(h, socket_path, args.timeout, "niri")
    out_queue: queue.Queue[tuple[float, dict[str, Any]]] = queue.Queue()

    try:
        sub = observer_client.sub("niri")
        env = expect_envelope(h, sub, kind=PUB, topic="niri")
        if not env or not isinstance(env.get("payload"), dict):
            h.error("initial niri PUB snapshot missing")
            return h.finish()
        initial_snapshot = env["payload"]

        target_output, indices = choose_output_and_indices(
            h,
            initial_snapshot,
            args.output,
            parse_indices(args.indices),
        )

        allowed = set(indices)
        warm_idx = indices[0]
        send_delay = args.send_delay_ms / 1000.0
        settle_delay = args.settle_ms / 1000.0
        niri_socket = default_niri_socket(args.niri_socket)

        observer = Observer(observer_client, out_queue)
        observer.start()

        warm_errors: list[str] = []
        warm_queue: queue.Queue[int] = queue.Queue()
        warm_queue.put(warm_idx)
        stress_sender(
            socket_path=socket_path,
            timeout=args.timeout,
            mode=args.mode,
            niri_socket=niri_socket,
            send_queue=warm_queue,
            send_delay=0.0,
            errors=warm_errors,
        )
        if warm_errors:
            for err in warm_errors:
                h.error(err)
            return h.finish()

        snap = wait_for_focus(out_queue, target_output, warm_idx, args.timeout)
        if snap is None or focused_idx_for_output(snap, target_output) != warm_idx:
            h.error(
                f"warmup failed to focus output={target_output} idx={warm_idx}; last={snap!r}"
            )
            return h.finish()
        h.ok(f"warmup focused {target_output}:{warm_idx}")

        sequence = [indices[(i + 1) % len(indices)] for i in range(args.rounds)]
        send_queue: queue.Queue[int] = queue.Queue()
        for idx in sequence:
            send_queue.put(idx)
        errors: list[str] = []
        threads: list[threading.Thread] = []
        start_ts = time.monotonic()
        for worker in range(args.parallel):
            thread = threading.Thread(
                target=stress_sender,
                kwargs={
                    "socket_path": socket_path,
                    "timeout": args.timeout,
                    "mode": args.mode,
                    "niri_socket": niri_socket,
                    "send_queue": send_queue,
                    "send_delay": send_delay,
                    "errors": errors,
                },
                daemon=True,
            )
            thread.start()
            threads.append(thread)

        anomalies: list[str] = []
        seen_snaps = 0
        deadline = None
        while True:
            alive = any(thread.is_alive() for thread in threads)
            if not alive and deadline is None:
                deadline = time.monotonic() + settle_delay
            if deadline is not None and time.monotonic() >= deadline and out_queue.empty():
                break

            timeout = 0.1 if deadline is None else min(0.1, max(0.01, deadline - time.monotonic()))
            try:
                ts, snap = out_queue.get(timeout=timeout)
            except queue.Empty:
                continue

            seen_snaps += 1
            focused = focused_workspaces(snap)
            if len(focused) != 1:
                anomalies.append(
                    f"{ts - start_ts:8.3f}s invalid global focus count={len(focused)} payload={focused!r}"
                )
                continue

            focused_idx = focused_idx_for_output(snap, target_output)
            if focused_idx is None:
                anomalies.append(
                    f"{ts - start_ts:8.3f}s no focused workspace on target output={target_output!r}"
                )
                continue

            if focused_idx not in allowed:
                direct_idx = direct_focused_idx_for_output(niri_socket, target_output, args.timeout)
                anomalies.append(
                    f"{ts - start_ts:8.3f}s unexpected focused idx={focused_idx} on output={target_output}; "
                    f"allowed={sorted(allowed)}; direct_niri_idx={direct_idx}"
                )

        for thread in threads:
            thread.join(timeout=0.1)

        observer.stop()
        observer.join(timeout=0.5)

        if errors:
            for err in errors[:8]:
                h.error(err)
            if len(errors) > 8:
                h.error(f"... and {len(errors) - 8} more sender errors")

        h.ok(
            f"observed {seen_snaps} niri snapshots during stress run "
            f"(mode={args.mode}, parallel={args.parallel}, rounds={args.rounds})"
        )

        if anomalies:
            for line in anomalies[:12]:
                h.error(line)
            if len(anomalies) > 12:
                h.error(f"... and {len(anomalies) - 12} more focus anomalies")
        else:
            h.ok("no out-of-set focused workspace observed during stress run")
    finally:
        try:
            observer_client.unsub("niri")
        except Exception:  # noqa: BLE001
            pass
        observer_client.close()

    return h.finish()


if __name__ == "__main__":
    main_guard(run)
