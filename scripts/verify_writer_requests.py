"""Compare bulk authoring and prepare vendor-free Windows scale review inputs."""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import sys
import time
from pathlib import Path

import ezsxf
from ezsxf._drawing import build_drawing


def new_document(name="drawing.sfc"):
    return ezsxf.new_sfc(name, paper="A1", timestamp="2026-10-07T00:00:00")


def benchmark(count):
    inputs = [{"kind": "line", "start": (i % 100, i // 100),
               "end": (i % 100 + 1, i // 100 + 1)} for i in range(count)]
    batch, individual = new_document(), new_document()
    start = time.perf_counter()
    batch.extend(inputs)
    batch_seconds = time.perf_counter() - start
    start = time.perf_counter()
    batch_bytes = batch.to_bytes()
    serialization_seconds = time.perf_counter() - start
    start = time.perf_counter()
    for item in inputs:
        individual.add_line(item["start"], item["end"])
    individual_seconds = time.perf_counter() - start
    a, b = batch.to_dict(), individual.to_dict()
    assert a["typed_features"] == b["typed_features"]
    assert a["model"] == b["model"]
    assert ezsxf.parse_sfc(batch_bytes) == a
    return {"count": count, "extend_seconds": batch_seconds,
            "to_bytes_seconds": serialization_seconds,
            "individual_add_seconds": individual_seconds,
            "addition_speedup": individual_seconds / batch_seconds,
            "sfc_sha256": hashlib.sha256(batch_bytes).hexdigest(),
            "models_equal": True}


def prepare(root):
    cases, expected = [{"id": "basic", "path": "basic.sfc"}], []
    control = new_document("basic.sfc")
    control.add_line((20, 20), (120, 20))
    control.add_text("日本語ABC", (20, 40))
    control.save(root / "basic.sfc")
    control.save_p21(root / "basic.p21")
    anchors = new_document("anchors-curves.sfc")
    anchors.extend([
        {"kind": "line", "start": (10, 10), "end": (110, 10)},
        {"kind": "circle", "center": (140, 30), "radius": 10},
        {"kind": "polyline", "points": [(160, 10), (180, 20), (200, 10)]},
        {"kind": "arc", "center": (230, 30), "radius": 10, "start_angle": 30,
         "end_angle": 300, "direction": 0},
        {"kind": "arc", "center": (270, 30), "radius": 10, "start_angle": 300,
         "end_angle": 30, "direction": 1},
        *[{"kind": "text", "text": f"基点{base}ABC", "anchor": (20 * base, 80),
           "angle": 30, "base_point": base} for base in range(1, 10)],
    ])
    anchors.save(root / "anchors-curves.sfc")
    anchors.save_p21(root / "anchors-curves.p21")
    cases.append({"id": "anchors-curves", "path": "anchors-curves.sfc"})
    part = new_document("shared-part.sfc")
    circle = part.add_circle((0, 0), 10)
    part.create_part("shared", [circle], [
        {"position": (100, 100)}, {"position": (200, 100)},
    ])
    part.save(root / "shared-part.sfc")
    part.save_p21(root / "shared-part.p21")
    cases.append({"id": "shared-part", "path": "shared-part.sfc"})
    for denominator in (1, 50, 100):
        name = f"scale-1-{denominator}"
        doc = new_document(name + ".sfc")
        ids = doc.extend([
            {"kind": "line", "start": (20 * denominator, 20 * denominator),
             "end": (120 * denominator, 20 * denominator)},
            {"kind": "text", "text": "日本語ABC",
             "anchor": (20 * denominator, 40 * denominator), "height": 3.5 * denominator},
        ])
        doc.group_elements(name, ids, kind=1, scale=(1 / denominator, 1 / denominator))
        doc.save(root / (name + ".sfc"))
        doc.save_p21(root / (name + ".p21"))
        for extension in ("sfc", "p21"):
            drawing = build_drawing(root / (name + "." + extension))
            assert drawing.warnings == []
            assert len(drawing.paths) == len(drawing.texts) == 1
            assert drawing.paths[0].points == ((20, 20), (120, 20))
            text = drawing.texts[0]
            assert text.anchor == (20, 40) and text.height == 3.5 and text.width == 15.75
        cases.append({"id": name, "path": name + ".sfc"})
        expected.append({"id": name, "scale_denominator": denominator,
                         "line_length_paper_mm": 100, "text_height_paper_mm": 3.5,
                         "text_box_width_paper_mm": 15.75, "text_anchor_paper_mm": [20, 40]})
    (root / "windows-sfc-cases.json").write_text(json.dumps(cases, indent=2), encoding="utf-8")
    return expected


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--counts", type=int, nargs="+", default=[100, 1000])
    args = parser.parse_args()
    if any(count < 1 for count in args.counts):
        parser.error("counts must be positive")
    args.output.mkdir(parents=True, exist_ok=False)
    expected = prepare(args.output)
    timings = []
    for count in args.counts:
        row = benchmark(count)
        timings.append(row)
        print(json.dumps(row), flush=True)
    report = {"ezsxf_version": ezsxf.__version__, "platform": platform.platform(),
              "python": sys.version, "scope": "local model checks and authoring timings",
              "timings": timings, "expected_windows_cases": expected,
              "native_windows_cad_status": "pending", "independent_p21_cad_status": "pending",
              "files_sha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                               for p in sorted(args.output.iterdir()) if p.is_file()}}
    (args.output / "report.json").write_text(json.dumps(report, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
