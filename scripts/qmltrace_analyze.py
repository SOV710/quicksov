#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import math
import sys
import xml.etree.ElementTree as ET
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Any

PROFILER_FILE_VERSION = "1.02"
SCHEMA_NAME = "quicksov.qmltrace_analysis"
SCHEMA_VERSION = 1

QML_RANGE_TYPES = {
    "Painting",
    "Compiling",
    "Creating",
    "Binding",
    "HandlingSignal",
    "Javascript",
}

EVENT_TYPES = {
    "0": "FramePaint",
    "1": "Mouse",
    "2": "Key",
    "3": "AnimationFrame",
    "4": "EndTrace",
    "5": "StartTrace",
}

INPUT_EVENT_TYPES = {
    0: "InputKeyPress",
    1: "InputKeyRelease",
    2: "InputKeyUnknown",
    3: "InputMousePress",
    4: "InputMouseRelease",
    5: "InputMouseMove",
    6: "InputMouseDoubleClick",
    7: "InputMouseWheel",
    8: "InputMouseUnknown",
}

PIXMAP_EVENT_TYPES = {
    0: "PixmapSizeKnown",
    1: "PixmapReferenceCountChanged",
    2: "PixmapCacheCountChanged",
    3: "PixmapLoadingStarted",
    4: "PixmapLoadingFinished",
    5: "PixmapLoadingError",
}

MEMORY_EVENT_TYPES = {
    0: "HeapPage",
    1: "LargeItem",
    2: "SmallItem",
}

ANIMATION_THREADS = {
    0: "GuiThread",
    1: "RenderThread",
}

SCENEGRAPH_FRAME_TYPES = {
    0: {
        "name": "SceneGraphRendererFrame",
        "thread": "render",
        "fields": [
            ("preprocess_time", "ns"),
            ("update_time", "ns"),
            ("binding_time", "ns"),
            ("render_time", "ns"),
        ],
    },
    1: {
        "name": "SceneGraphAdaptationLayerFrame",
        "thread": "render",
        "fields": [
            ("glyph_count", "count"),
            ("glyph_render_time", "ns"),
            ("glyph_store_time", "ns"),
        ],
    },
    2: {
        "name": "SceneGraphContextFrame",
        "thread": "render",
        "fields": [("material_compile_time", "ns")],
    },
    3: {
        "name": "SceneGraphRenderLoopFrame",
        "thread": "render",
        "fields": [
            ("sync_time", "ns"),
            ("render_time", "ns"),
            ("swap_time", "ns"),
        ],
    },
    4: {
        "name": "SceneGraphTexturePrepare",
        "thread": "render",
        "fields": [
            ("bind_time", "ns"),
            ("convert_time", "ns"),
            ("swizzle_time", "ns"),
            ("upload_time", "ns"),
            ("mipmap_time", "ns"),
        ],
    },
    5: {
        "name": "SceneGraphTextureDeletion",
        "thread": "render",
        "fields": [("deletion_time", "ns")],
    },
    6: {
        "name": "SceneGraphPolishAndSync",
        "thread": "gui",
        "fields": [
            ("polish_time", "ns"),
            ("wait_time", "ns"),
            ("sync_time", "ns"),
            ("animations_time", "ns"),
        ],
    },
    7: {
        "name": "SceneGraphWindowsRenderShow",
        "thread": "unused",
        "fields": [
            ("gl_time", "ns"),
            ("make_current_time", "ns"),
            ("scenegraph_time", "ns"),
        ],
    },
    8: {
        "name": "SceneGraphWindowsAnimations",
        "thread": "gui",
        "fields": [("update_time", "ns")],
    },
    9: {
        "name": "SceneGraphPolishFrame",
        "thread": "gui",
        "fields": [("polish_time", "ns")],
    },
}


class QmlTraceError(RuntimeError):
    pass


@dataclass(frozen=True)
class EventMeta:
    index: int
    type: str
    displayname: str | None = None
    filename: str | None = None
    repo_path: str | None = None
    line: int | None = None
    column: int | None = None
    details: str | None = None
    binding_type: str | None = None
    event_detail: str | None = None
    pixmap_event_type: int | None = None
    scenegraph_event_type: int | None = None
    memory_event_type: int | None = None


@dataclass(frozen=True)
class RangeRow:
    start_ns: int
    duration_ns: int
    event_index: int
    event: EventMeta
    attrs: dict[str, str]


def ns_to_ms(value: int | float) -> float:
    return round(float(value) / 1_000_000.0, 6)


def parse_int(value: str | None, *, default: int = 0) -> int:
    if value is None or value == "":
        return default
    return int(value)


def child_text(element: ET.Element, name: str) -> str | None:
    child = element.find(name)
    return child.text if child is not None else None


def child_int(element: ET.Element, name: str) -> int | None:
    text = child_text(element, name)
    return int(text) if text not in (None, "") else None


def clamp_text(value: str | None, max_chars: int) -> tuple[str | None, bool]:
    if value is None or len(value) <= max_chars:
        return value, False
    if max_chars <= 1:
        return value[:max_chars], True
    return value[: max_chars - 1] + "...", True


def percentile(sorted_values: list[int | float], pct: float) -> int | float:
    if not sorted_values:
        return 0
    index = math.ceil((pct / 100.0) * len(sorted_values)) - 1
    index = min(max(index, 0), len(sorted_values) - 1)
    return sorted_values[index]


def stats_ns(values: list[int]) -> dict[str, float | int]:
    if not values:
        return {
            "count": 0,
            "total_ms": 0.0,
            "avg_ms": 0.0,
            "max_ms": 0.0,
            "p50_ms": 0.0,
            "p95_ms": 0.0,
        }
    ordered = sorted(values)
    total = sum(ordered)
    return {
        "count": len(ordered),
        "total_ms": ns_to_ms(total),
        "avg_ms": ns_to_ms(total / len(ordered)),
        "max_ms": ns_to_ms(ordered[-1]),
        "p50_ms": ns_to_ms(percentile(ordered, 50)),
        "p95_ms": ns_to_ms(percentile(ordered, 95)),
    }


def stats_numbers(values: list[int]) -> dict[str, float | int]:
    if not values:
        return {"count": 0, "total": 0, "avg": 0.0, "max": 0, "p50": 0, "p95": 0}
    ordered = sorted(values)
    total = sum(ordered)
    return {
        "count": len(ordered),
        "total": total,
        "avg": round(total / len(ordered), 6),
        "max": ordered[-1],
        "p50": percentile(ordered, 50),
        "p95": percentile(ordered, 95),
    }


def map_repo_path(filename: str | None) -> str | None:
    if not filename:
        return None
    prefix = "qs:@/qs/"
    if filename.startswith(prefix):
        return "shell/" + filename[len(prefix) :]
    return None


def format_source(repo_path: str | None, filename: str | None, line: int | None) -> str:
    path = repo_path or filename or "<unknown>"
    if line is not None:
        return f"{path}:{line}"
    return path


def parse_trace(path: Path) -> tuple[ET.Element, dict[int, EventMeta], list[RangeRow]]:
    if path.suffix != ".qtd":
        raise QmlTraceError(f"unsupported trace extension {path.suffix!r}; expected '.qtd'")
    if not path.is_file():
        raise QmlTraceError(f"trace file does not exist: {path}")

    try:
        root = ET.parse(path).getroot()
    except ET.ParseError as exc:
        raise QmlTraceError(f"invalid XML: {exc}") from exc

    if root.tag != "trace":
        raise QmlTraceError(f"invalid root element {root.tag!r}; expected 'trace'")
    version = root.attrib.get("version")
    if version != PROFILER_FILE_VERSION:
        raise QmlTraceError(
            f"unsupported qmlprofiler XML version {version!r}; expected {PROFILER_FILE_VERSION!r}"
        )

    event_data = root.find("eventData")
    if event_data is None:
        raise QmlTraceError("missing eventData element")
    model = root.find("profilerDataModel")
    if model is None:
        raise QmlTraceError("missing profilerDataModel element")

    events: dict[int, EventMeta] = {}
    for event_element in event_data.findall("event"):
        index = parse_int(event_element.attrib.get("index"), default=-1)
        if index < 0:
            raise QmlTraceError("eventData contains event without a valid index")

        filename = child_text(event_element, "filename")
        event_detail = None
        if child_text(event_element, "animationFrame") is not None:
            event_detail = "AnimationFrame"
        elif child_text(event_element, "keyEvent") is not None:
            event_detail = "Key"
        elif child_text(event_element, "mouseEvent") is not None:
            event_detail = "Mouse"

        meta = EventMeta(
            index=index,
            displayname=child_text(event_element, "displayname"),
            type=child_text(event_element, "type") or "",
            filename=filename,
            repo_path=map_repo_path(filename),
            line=child_int(event_element, "line"),
            column=child_int(event_element, "column"),
            details=child_text(event_element, "details"),
            binding_type=child_text(event_element, "bindingType"),
            event_detail=event_detail,
            pixmap_event_type=child_int(event_element, "cacheEventType"),
            scenegraph_event_type=child_int(event_element, "sgEventType"),
            memory_event_type=child_int(event_element, "memoryEventType"),
        )
        events[index] = meta

    ranges: list[RangeRow] = []
    for range_element in model.findall("range"):
        event_index = parse_int(range_element.attrib.get("eventIndex"), default=-1)
        if event_index not in events:
            raise QmlTraceError(f"range references unknown eventIndex {event_index}")
        ranges.append(
            RangeRow(
                start_ns=parse_int(range_element.attrib.get("startTime")),
                duration_ns=parse_int(range_element.attrib.get("duration")),
                event_index=event_index,
                event=events[event_index],
                attrs=dict(range_element.attrib),
            )
        )

    return root, events, ranges


def merge_intervals(intervals: list[tuple[int, int]]) -> int:
    if not intervals:
        return 0
    ordered = sorted(intervals)
    total = 0
    cur_start, cur_end = ordered[0]
    for start, end in ordered[1:]:
        if start <= cur_end:
            cur_end = max(cur_end, end)
        else:
            total += cur_end - cur_start
            cur_start, cur_end = start, end
    total += cur_end - cur_start
    return total


def aggregate_qml(
    ranges: list[RangeRow],
    *,
    details_max_chars: int,
    top: int,
) -> tuple[dict[str, Any], list[dict[str, Any]], list[tuple[int, int]], int]:
    by_type: dict[str, list[int]] = defaultdict(list)
    hotspot_durations: dict[tuple[Any, ...], list[int]] = defaultdict(list)
    qml_intervals: list[tuple[int, int]] = []
    inclusive_total = 0

    for row in ranges:
        event = row.event
        if event.type not in QML_RANGE_TYPES:
            continue
        duration = row.duration_ns
        by_type[event.type].append(duration)
        inclusive_total += duration
        if duration > 0:
            qml_intervals.append((row.start_ns, row.start_ns + duration))

        key = (
            event.type,
            event.displayname,
            event.filename,
            event.repo_path,
            event.line,
            event.column,
            event.details,
            event.binding_type,
        )
        hotspot_durations[key].append(duration)

    by_type_rows = []
    for range_type in sorted(QML_RANGE_TYPES):
        values = by_type.get(range_type, [])
        row = {"type": range_type, **stats_ns(values)}
        by_type_rows.append(row)
    by_type_rows.sort(key=lambda item: (-float(item["total_ms"]), item["type"]))

    hotspots = []
    ordered_hotspots = sorted(
        hotspot_durations.items(),
        key=lambda item: (-sum(item[1]), item[0][0], item[0][3] or item[0][2] or ""),
    )
    for rank, (key, values) in enumerate(ordered_hotspots[:top], start=1):
        (
            range_type,
            displayname,
            filename,
            repo_path,
            line,
            column,
            details,
            binding_type,
        ) = key
        display_details, truncated = clamp_text(details, details_max_chars)
        hotspots.append(
            {
                "rank": rank,
                "type": range_type,
                "displayname": displayname,
                "filename": filename,
                "repo_path": repo_path,
                "line": line,
                "column": column,
                "source": format_source(repo_path, filename, line),
                "binding_type": binding_type,
                "details": display_details,
                "details_truncated": truncated,
                **stats_ns(values),
            }
        )

    qml_summary = {
        "inclusive_range_time_ms": ns_to_ms(inclusive_total),
        "merged_range_time_ms": ns_to_ms(merge_intervals(qml_intervals)),
        "by_type": by_type_rows,
    }
    return qml_summary, hotspots, qml_intervals, inclusive_total


def scenegraph_field_values(row: RangeRow) -> tuple[dict[str, int], list[str]]:
    warnings: list[str] = []
    event_type = row.event.scenegraph_event_type
    if event_type is None or event_type not in SCENEGRAPH_FRAME_TYPES:
        return {}, [f"unknown scenegraph event type: {event_type!r}"]
    spec = SCENEGRAPH_FRAME_TYPES[event_type]
    values: dict[str, int] = {}
    for i, (field_name, _unit) in enumerate(spec["fields"], start=1):
        attr_name = f"timing{i}"
        if attr_name in row.attrs:
            values[field_name] = parse_int(row.attrs[attr_name])
    return values, warnings


def aggregate_scenegraph(ranges: list[RangeRow]) -> tuple[dict[str, Any], list[str], int]:
    by_type: dict[int, dict[str, list[int]]] = defaultdict(lambda: defaultdict(list))
    counts: Counter[int] = Counter()
    warnings: list[str] = []
    negative_count = 0

    for row in ranges:
        if row.event.type != "SceneGraph":
            continue
        event_type = row.event.scenegraph_event_type
        if event_type is None:
            warnings.append(f"scenegraph range at {row.start_ns} has no sgEventType")
            continue
        counts[event_type] += 1
        values, row_warnings = scenegraph_field_values(row)
        warnings.extend(row_warnings)
        for field_name, value in values.items():
            if value < 0:
                negative_count += 1
            by_type[event_type][field_name].append(value)

    frame_rows = []
    total_cost_ns = 0
    for event_type, count in sorted(counts.items()):
        spec = SCENEGRAPH_FRAME_TYPES.get(
            event_type,
            {"name": f"UnknownSceneGraphFrame:{event_type}", "thread": "unknown", "fields": []},
        )
        fields = {}
        frame_cost_ns = 0
        for field_name, unit in spec["fields"]:
            values = by_type[event_type].get(field_name, [])
            if unit == "ns":
                positive_values = [value for value in values if value >= 0]
                negative_values = [value for value in values if value < 0]
                field_stats = stats_ns(positive_values)
                field_stats["negative_count"] = len(negative_values)
                if negative_values:
                    field_stats["negative_min_ms"] = ns_to_ms(min(negative_values))
                fields[f"{field_name}_ms"] = field_stats
                frame_cost_ns += sum(positive_values)
            else:
                fields[field_name] = stats_numbers(values)
        total_cost_ns += frame_cost_ns
        frame_rows.append(
            {
                "sg_event_type": event_type,
                "name": spec["name"],
                "thread": spec["thread"],
                "count": count,
                "cost_total_ms": ns_to_ms(frame_cost_ns),
                "fields": fields,
            }
        )

    frame_rows.sort(key=lambda item: (-item["cost_total_ms"], item["sg_event_type"]))
    return (
        {
            "event_count": sum(counts.values()),
            "cost_total_ms": ns_to_ms(total_cost_ns),
            "negative_timing_count": negative_count,
            "by_frame_type": frame_rows,
        },
        warnings,
        negative_count,
    )


def aggregate_events(
    ranges: list[RangeRow],
    *,
    details_max_chars: int,
    top: int,
) -> tuple[dict[str, Any], list[str]]:
    warnings: list[str] = []
    animation_fps: list[int] = []
    animation_counts: list[int] = []
    animation_threads: Counter[str] = Counter()
    input_counts: Counter[str] = Counter()
    pixmap_counts: Counter[str] = Counter()
    pixmap_locations: dict[tuple[str | None, str | None, str | None], int] = defaultdict(int)
    pixmap_sizes: dict[tuple[int, int], int] = defaultdict(int)
    pixmap_refs: list[int] = []
    memory_amounts: dict[str, list[int]] = defaultdict(list)
    debug_counts: Counter[str] = Counter()
    other_events: Counter[str] = Counter()

    for row in ranges:
        event = row.event
        if event.type == "Event":
            detail = event.event_detail or EVENT_TYPES.get(str(event.displayname), "OtherEvent")
            if detail == "AnimationFrame":
                animation_fps.append(parse_int(row.attrs.get("framerate")))
                animation_counts.append(parse_int(row.attrs.get("animationcount")))
                thread = ANIMATION_THREADS.get(parse_int(row.attrs.get("thread")), "UnknownThread")
                animation_threads[thread] += 1
            elif detail in {"Key", "Mouse"}:
                input_type = parse_int(row.attrs.get("type"), default=-1)
                input_name = INPUT_EVENT_TYPES.get(input_type, f"UnknownInput:{input_type}")
                input_counts[f"{detail}:{input_name}"] += 1
            else:
                other_events[detail] += 1
        elif event.type == "PixmapCache":
            event_type = event.pixmap_event_type
            event_name = PIXMAP_EVENT_TYPES.get(
                event_type if event_type is not None else -1,
                f"UnknownPixmap:{event_type}",
            )
            pixmap_counts[event_name] += 1
            pixmap_locations[(event_name, event.filename, event.details)] += 1
            if "width" in row.attrs or "height" in row.attrs:
                pixmap_sizes[
                    (parse_int(row.attrs.get("width")), parse_int(row.attrs.get("height")))
                ] += 1
            if "refCount" in row.attrs:
                pixmap_refs.append(parse_int(row.attrs.get("refCount")))
        elif event.type == "MemoryAllocation":
            event_type = event.memory_event_type
            event_name = MEMORY_EVENT_TYPES.get(
                event_type if event_type is not None else -1,
                f"UnknownMemory:{event_type}",
            )
            memory_amounts[event_name].append(parse_int(row.attrs.get("amount")))
        elif event.type == "DebugMessage":
            debug_counts[event.displayname or "DebugMessage"] += 1
        elif event.type not in QML_RANGE_TYPES and event.type != "SceneGraph":
            other_events[event.type or "<empty>"] += 1

    top_pixmaps = []
    for rank, ((event_name, filename, details), count) in enumerate(
        sorted(pixmap_locations.items(), key=lambda item: (-item[1], item[0][0], item[0][1] or ""))[
            :top
        ],
        start=1,
    ):
        display_details, truncated = clamp_text(details, details_max_chars)
        top_pixmaps.append(
            {
                "rank": rank,
                "event_type": event_name,
                "filename": filename,
                "details": display_details,
                "details_truncated": truncated,
                "count": count,
            }
        )

    return (
        {
            "animation_frames": {
                "count": len(animation_fps),
                "framerate": stats_numbers(animation_fps),
                "animationcount": stats_numbers(animation_counts),
                "threads": dict(sorted(animation_threads.items())),
            },
            "input": dict(sorted(input_counts.items())),
            "pixmap_cache": {
                "by_event_type": dict(sorted(pixmap_counts.items())),
                "sizes": [
                    {"width": width, "height": height, "count": count}
                    for (width, height), count in sorted(pixmap_sizes.items())
                ],
                "ref_count": stats_numbers(pixmap_refs),
                "top_locations": top_pixmaps,
            },
            "memory": {
                "by_event_type": {
                    name: stats_numbers(values) for name, values in sorted(memory_amounts.items())
                }
            },
            "debug_messages": dict(sorted(debug_counts.items())),
            "other_events": dict(sorted(other_events.items())),
        },
        warnings,
    )


def normalized_range(row: RangeRow, *, details_max_chars: int) -> dict[str, Any]:
    event = row.event
    details, truncated = clamp_text(event.details, details_max_chars)
    base: dict[str, Any] = {
        "start_ms": ns_to_ms(row.start_ns),
        "duration_ms": ns_to_ms(row.duration_ns),
        "event_index": row.event_index,
        "type": event.type,
        "displayname": event.displayname,
        "filename": event.filename,
        "repo_path": event.repo_path,
        "line": event.line,
        "column": event.column,
        "source": format_source(event.repo_path, event.filename, event.line),
        "details": details,
        "details_truncated": truncated,
    }
    if event.type == "SceneGraph":
        values, _warnings = scenegraph_field_values(row)
        event_type = event.scenegraph_event_type
        spec = SCENEGRAPH_FRAME_TYPES.get(event_type or -1)
        base["scenegraph"] = {
            "sg_event_type": event_type,
            "name": spec["name"] if spec else None,
            "values": {
                key: ns_to_ms(value) if key.endswith("_time") else value
                for key, value in values.items()
            },
        }
    return base


def write_json(path: Path, data: Any) -> None:
    path.write_text(
        json.dumps(data, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def write_ranges(path: Path, ranges: list[RangeRow], *, details_max_chars: int) -> None:
    with path.open("w", encoding="utf-8") as handle:
        for row in ranges:
            handle.write(
                json.dumps(
                    normalized_range(row, details_max_chars=details_max_chars),
                    ensure_ascii=False,
                    sort_keys=True,
                    separators=(",", ":"),
                )
            )
            handle.write("\n")


def markdown_escape(value: Any) -> str:
    return str(value).replace("|", "\\|").replace("\n", " ")


def write_report(
    path: Path,
    *,
    trace_file: Path,
    summary: dict[str, Any],
    hotspots: list[dict[str, Any]],
    scenegraph: dict[str, Any],
) -> None:
    lines = [
        f"# QML Trace Analysis: {trace_file.name}",
        "",
        "## Summary",
        "",
        f"- Trace duration: {summary['trace']['duration_ms']} ms",
        f"- QML measured time: {summary['qml']['measured_time_ms']} ms",
        f"- QML inclusive range time: {summary['qml']['inclusive_range_time_ms']} ms",
        f"- SceneGraph timing cost: {scenegraph['cost_total_ms']} ms",
        f"- Ranges: {summary['trace']['ranges']}, event types: {summary['trace']['event_types']}",
        "",
        "## QML Range Types",
        "",
        "| type | count | total_ms | max_ms | p95_ms |",
        "| --- | ---: | ---: | ---: | ---: |",
    ]
    for item in summary["qml"]["by_type"][:8]:
        lines.append(
            f"| {item['type']} | {item['count']} | {item['total_ms']} | {item['max_ms']} | {item['p95_ms']} |"
        )

    lines.extend(
        [
            "",
            "## Top Hotspots",
            "",
            "| rank | type | source | total_ms | count | max_ms |",
            "| ---: | --- | --- | ---: | ---: | ---: |",
        ]
    )
    for item in hotspots[:12]:
        lines.append(
            "| {rank} | {type} | {source} | {total_ms} | {count} | {max_ms} |".format(
                rank=item["rank"],
                type=markdown_escape(item["type"]),
                source=markdown_escape(item["source"]),
                total_ms=item["total_ms"],
                count=item["count"],
                max_ms=item["max_ms"],
            )
        )

    lines.extend(
        [
            "",
            "## SceneGraph",
            "",
            "| frame_type | thread | count | cost_total_ms |",
            "| --- | --- | ---: | ---: |",
        ]
    )
    for item in scenegraph["by_frame_type"][:12]:
        lines.append(
            f"| {item['name']} | {item['thread']} | {item['count']} | {item['cost_total_ms']} |"
        )

    lines.extend(
        [
            "",
            "## Notes",
            "",
            "- QML range totals are inclusive and may double-count nested work.",
            "- QML measured time is copied from eventData totalTime and compared with merged ranges in integrity.json.",
            "- SceneGraph timing fields are interpreted by sgEventType, not as generic range duration.",
        ]
    )
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def compact_hotspot(item: dict[str, Any]) -> dict[str, Any]:
    return {
        "rank": item["rank"],
        "type": item["type"],
        "source": item["source"],
        "total_ms": item["total_ms"],
        "count": item["count"],
        "max_ms": item["max_ms"],
        "p95_ms": item["p95_ms"],
    }


def compact_scenegraph_frame(item: dict[str, Any]) -> dict[str, Any]:
    return {
        "sg_event_type": item["sg_event_type"],
        "name": item["name"],
        "thread": item["thread"],
        "count": item["count"],
        "cost_total_ms": item["cost_total_ms"],
    }


def analyze_trace(
    trace_path: Path,
    out_dir: Path,
    *,
    top: int,
    details_max_chars: int,
    emit_ranges: bool,
) -> dict[str, Any]:
    root, events, ranges = parse_trace(trace_path)
    out_dir.mkdir(parents=True, exist_ok=True)

    trace_start_ns = parse_int(root.attrib.get("traceStart"))
    trace_end_ns = parse_int(root.attrib.get("traceEnd"))
    event_data = root.find("eventData")
    measured_qml_ns = parse_int(event_data.attrib.get("totalTime") if event_data is not None else None)

    qml_summary, hotspots, qml_intervals, inclusive_qml_ns = aggregate_qml(
        ranges, details_max_chars=details_max_chars, top=top
    )
    merged_qml_ns = merge_intervals(qml_intervals)
    scenegraph, scenegraph_warnings, negative_timing_count = aggregate_scenegraph(ranges)
    event_summary, event_warnings = aggregate_events(
        ranges, details_max_chars=details_max_chars, top=top
    )

    unknown_types = Counter(row.event.type or "<empty>" for row in ranges)
    for known in QML_RANGE_TYPES | {"Event", "PixmapCache", "SceneGraph", "MemoryAllocation", "DebugMessage"}:
        unknown_types.pop(known, None)

    integrity = {
        "schema": {"name": SCHEMA_NAME, "version": SCHEMA_VERSION},
        "profiler_file_version": root.attrib.get("version"),
        "trace_start_ns": trace_start_ns,
        "trace_end_ns": trace_end_ns,
        "duration_ns": trace_end_ns - trace_start_ns,
        "qml_measured_time_ns": measured_qml_ns,
        "qml_inclusive_range_time_ns": inclusive_qml_ns,
        "qml_merged_range_time_ns": merged_qml_ns,
        "qml_measured_vs_merged_delta_ns": measured_qml_ns - merged_qml_ns,
        "negative_scenegraph_timing_count": negative_timing_count,
        "unknown_range_types": dict(sorted(unknown_types.items())),
        "warnings": scenegraph_warnings + event_warnings,
    }

    output_files = {
        "summary": "summary.json",
        "hotspots": "hotspots.json",
        "scenegraph": "scenegraph.json",
        "events": "events.json",
        "integrity": "integrity.json",
        "report": "report.md",
    }
    if emit_ranges:
        output_files["ranges"] = "ranges.ndjson"

    summary = {
        "schema": {"name": SCHEMA_NAME, "version": SCHEMA_VERSION},
        "input": {"file": str(trace_path), "name": trace_path.name},
        "trace": {
            "duration_ms": ns_to_ms(trace_end_ns - trace_start_ns),
            "event_types": len(events),
            "ranges": len(ranges),
            "trace_start_ns": trace_start_ns,
            "trace_end_ns": trace_end_ns,
        },
        "qml": {
            "measured_time_ms": ns_to_ms(measured_qml_ns),
            **qml_summary,
        },
        "scenegraph": {
            "event_count": scenegraph["event_count"],
            "cost_total_ms": scenegraph["cost_total_ms"],
            "negative_timing_count": scenegraph["negative_timing_count"],
            "top_frame_types": [
                compact_scenegraph_frame(item) for item in scenegraph["by_frame_type"][: min(top, 10)]
            ],
        },
        "top_hotspots": [compact_hotspot(item) for item in hotspots[: min(top, 10)]],
        "outputs": output_files,
        "notes": [
            "QML range totals are inclusive and may double-count nested work.",
            "eventData totalTime is preserved as qml.measured_time_ms.",
            "SceneGraph timing fields are interpreted according to sgEventType.",
        ],
    }

    write_json(out_dir / "hotspots.json", {"hotspots": hotspots})
    write_json(out_dir / "scenegraph.json", scenegraph)
    write_json(out_dir / "events.json", event_summary)
    write_json(out_dir / "integrity.json", integrity)
    if emit_ranges:
        write_ranges(out_dir / "ranges.ndjson", ranges, details_max_chars=details_max_chars)
    write_report(out_dir / "report.md", trace_file=trace_path, summary=summary, hotspots=hotspots, scenegraph=scenegraph)
    write_json(out_dir / "summary.json", summary)
    return summary


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Analyze Qt qmlprofiler .qtd XML traces")
    subparsers = parser.add_subparsers(dest="command", required=True)

    analyze = subparsers.add_parser("analyze", help="analyze one .qtd trace")
    analyze.add_argument("trace", type=Path, help="input .qtd trace")
    analyze.add_argument(
        "--out-dir",
        type=Path,
        default=None,
        help="output directory; defaults to TRACE_STEM.analysis next to the trace",
    )
    analyze.add_argument("--top", type=int, default=40, help="maximum rows in hotspot outputs")
    analyze.add_argument(
        "--details-max-chars",
        type=int,
        default=160,
        help="maximum details text length in JSON outputs",
    )
    analyze.add_argument(
        "--emit-ranges",
        action="store_true",
        help="emit one normalized range per line as ranges.ndjson",
    )
    return parser


def run(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    try:
        if args.command == "analyze":
            out_dir = args.out_dir
            if out_dir is None:
                out_dir = args.trace.with_name(f"{args.trace.stem}.analysis")
            summary = analyze_trace(
                args.trace,
                out_dir,
                top=max(args.top, 1),
                details_max_chars=max(args.details_max_chars, 1),
                emit_ranges=args.emit_ranges,
            )
            print(json.dumps({"out_dir": str(out_dir), "summary": summary["outputs"]}, sort_keys=True))
            return 0
    except QmlTraceError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
    return 1


if __name__ == "__main__":
    raise SystemExit(run())
