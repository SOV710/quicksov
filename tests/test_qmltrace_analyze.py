#!/usr/bin/env python3
from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
ANALYZER = REPO_ROOT / "scripts" / "qmltrace_analyze.py"


FIXTURE = """<?xml version="1.0" encoding="UTF-8"?>
<trace version="1.02" traceStart="0" traceEnd="10000000">
  <eventData totalTime="2000000">
    <event index="0">
      <displayname>Foo.qml:10</displayname>
      <type>Javascript</type>
      <filename>qs:@/qs/widgets/Foo.qml</filename>
      <line>10</line>
      <column>5</column>
      <details>function hotPath()</details>
    </event>
    <event index="1">
      <displayname>Foo.qml:12</displayname>
      <type>Binding</type>
      <filename>qs:@/qs/widgets/Foo.qml</filename>
      <line>12</line>
      <column>7</column>
      <details>width: parent.width</details>
      <bindingType>0</bindingType>
    </event>
    <event index="2">
      <displayname>SceneGraph:3</displayname>
      <type>SceneGraph</type>
      <sgEventType>3</sgEventType>
    </event>
    <event index="3">
      <displayname>icon.png:0</displayname>
      <type>PixmapCache</type>
      <filename>file:///tmp/icon.png</filename>
      <cacheEventType>0</cacheEventType>
    </event>
    <event index="4">
      <displayname>MemoryAllocation:1</displayname>
      <type>MemoryAllocation</type>
      <memoryEventType>1</memoryEventType>
    </event>
    <event index="5">
      <displayname>AnimationFrame</displayname>
      <type>Event</type>
      <animationFrame>3</animationFrame>
    </event>
  </eventData>
  <profilerDataModel>
    <range startTime="1000" duration="2000000" eventIndex="0"/>
    <range startTime="2500" duration="1000000" eventIndex="1"/>
    <range startTime="5000" eventIndex="2" timing1="1000000" timing2="2000000" timing3="3000000"/>
    <range startTime="6000" eventIndex="3" width="16" height="32"/>
    <range startTime="7000" eventIndex="4" amount="4096"/>
    <range startTime="8000" eventIndex="5" framerate="60" animationcount="2" thread="0"/>
  </profilerDataModel>
</trace>
"""


class QmlTraceAnalyzeTest(unittest.TestCase):
    def test_analyze_fixture_outputs_compact_json(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            trace = root / "fixture.qtd"
            out_dir = root / "analysis"
            trace.write_text(FIXTURE, encoding="utf-8")

            result = subprocess.run(
                [
                    sys.executable,
                    str(ANALYZER),
                    "analyze",
                    str(trace),
                    "--out-dir",
                    str(out_dir),
                    "--top",
                    "5",
                ],
                check=False,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            summary = json.loads((out_dir / "summary.json").read_text(encoding="utf-8"))
            hotspots = json.loads((out_dir / "hotspots.json").read_text(encoding="utf-8"))
            scenegraph = json.loads((out_dir / "scenegraph.json").read_text(encoding="utf-8"))
            events = json.loads((out_dir / "events.json").read_text(encoding="utf-8"))
            integrity = json.loads((out_dir / "integrity.json").read_text(encoding="utf-8"))

            self.assertEqual(summary["schema"]["name"], "quicksov.qmltrace_analysis")
            self.assertEqual(summary["qml"]["measured_time_ms"], 2.0)
            self.assertEqual(summary["qml"]["inclusive_range_time_ms"], 3.0)
            self.assertEqual(summary["qml"]["merged_range_time_ms"], 2.0)
            self.assertEqual(hotspots["hotspots"][0]["repo_path"], "shell/widgets/Foo.qml")
            self.assertEqual(hotspots["hotspots"][0]["source"], "shell/widgets/Foo.qml:10")

            frame = scenegraph["by_frame_type"][0]
            self.assertEqual(frame["name"], "SceneGraphRenderLoopFrame")
            self.assertEqual(frame["fields"]["sync_time_ms"]["total_ms"], 1.0)
            self.assertEqual(frame["fields"]["render_time_ms"]["total_ms"], 2.0)
            self.assertEqual(frame["fields"]["swap_time_ms"]["total_ms"], 3.0)

            self.assertEqual(events["pixmap_cache"]["sizes"][0], {"width": 16, "height": 32, "count": 1})
            self.assertEqual(events["memory"]["by_event_type"]["LargeItem"]["total"], 4096)
            self.assertEqual(events["animation_frames"]["threads"], {"GuiThread": 1})
            self.assertEqual(integrity["qml_measured_vs_merged_delta_ns"], 0)
            self.assertTrue((out_dir / "report.md").is_file())

    def test_rejects_non_qtd_extension(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            trace = root / "fixture.qzt"
            trace.write_text(FIXTURE, encoding="utf-8")

            result = subprocess.run(
                [sys.executable, str(ANALYZER), "analyze", str(trace), "--out-dir", str(root / "out")],
                check=False,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )

            self.assertEqual(result.returncode, 2)
            self.assertIn("expected '.qtd'", result.stderr)


if __name__ == "__main__":
    unittest.main()
