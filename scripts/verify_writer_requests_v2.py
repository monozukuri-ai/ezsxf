"""Measure structured batches and prepare native-CAD point/spline controls."""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import time
from pathlib import Path

import ezsxf


def document():
    return ezsxf.new_sfc(timestamp="2026-10-08T00:00:00")


def line(x):
    return {"kind": "line", "start": (x, 0), "end": (x, 10)}


def run(output: Path) -> dict:
    output.mkdir(parents=True, exist_ok=False)
    elements = [line(i) for i in range(36_000)]
    elements += [
        {"kind": "fill", "outer": [(i, 20), (i + 1, 20), (i + 1, 21), (i, 21)]}
        for i in range(1_000)
    ]
    elements += [
        {
            "kind": "part",
            "name": f"part-{i}",
            "elements": [line(j) for j in range(50)],
            "placements": [{"position": (j * 60, i * 20)} for j in range(5)],
        }
        for i in range(200)
    ]
    doc = document()
    start = time.perf_counter()
    ids = doc.extend(elements)
    bulk_seconds = time.perf_counter() - start
    parsed = ezsxf.parse_sfc(doc.to_bytes(), strict=True)
    assert not parsed["warnings"] and len(ids) == len(elements)
    model = parsed["model"]
    assert len(model["sfig_definitions"]) == 200
    assert len(model["sfig_references"]) == 1_000
    assert len(model["hatch_references"]) == 1_000

    inputs = [line(i) for i in range(20_000)]
    bad = list(range(0, 20_000, 400))
    for i in bad:
        inputs[i] = {"kind": "circle", "radius": -1}
    clean = document()
    start = time.perf_counter()
    accepted, rejected = clean.extend(inputs, on_invalid="skip")
    skip_seconds = time.perf_counter() - start
    assert [i for i, _ in rejected] == bad
    assert [i for i, id_ in enumerate(accepted) if id_ is None] == bad
    assert not ezsxf.parse_sfc(clean.to_bytes(), strict=True)["warnings"]

    # Adjacent endpoints exercise coordinate sharing without duplicate lines.
    size = document()
    size.extend(
        [{"kind": "line", "start": (i, 0), "end": (i + 1, 0)} for i in range(20_000)]
    )
    sfc, p21, p2z = size.to_bytes(), size.to_p21_bytes(), size.to_p2z_bytes()
    p21_parsed = ezsxf.parse_p21(p21, strict=True)
    points = [
        e["record"]["parameters"][1]
        for e in p21_parsed["entities"]
        if e.get("record", {}).get("keyword") == "CARTESIAN_POINT"
    ]
    assert len(points) == len({tuple(p) for p in points})
    assert not p21_parsed["warnings"]

    control = document()
    control.extend(
        [
            {
                "kind": "point_marker",
                "marker": n,
                "x": n * 20,
                "y": 20,
                "angle": 30,
                "scale": 3,
            }
            for n in range(1, 8)
        ]
    )
    control.extend(
        [
            {"kind": "spline", "points": [(10, 60), (30, 90), (50, 90), (70, 60)]},
            {
                "kind": "spline",
                "open_close": 0,
                "points": [(90, 60), (130, 60), (90, 100), (90, 60)],
            },
            {
                "kind": "spline",
                "points": [
                    (10, 120),
                    (30, 150),
                    (50, 150),
                    (70, 120),
                    (90, 90),
                    (110, 90),
                    (130, 120),
                ],
            },
        ]
    )
    hashes = {}
    for extension, payload in (
        ("sfc", control.to_bytes()),
        ("p21", control.to_p21_bytes()),
    ):
        filename = f"markers-splines.{extension}"
        (output / filename).write_bytes(payload)
        hashes[filename] = hashlib.sha256(payload).hexdigest()
    report = {
        "platform": platform.platform(),
        "python": platform.python_version(),
        "bulk": {
            "input_count": len(elements),
            "feature_count": len(parsed["typed_features"]),
            "fills": 1000,
            "parts": 200,
            "members_per_part": 50,
            "placements_per_part": 5,
            "seconds": bulk_seconds,
            "under_two_seconds": bulk_seconds < 2,
            "strict_warnings": 0,
        },
        "skip": {
            "input_count": 20_000,
            "rejected": rejected,
            "seconds": skip_seconds,
            "under_point_four_seconds": skip_seconds < 0.4,
            "strict_warnings": 0,
        },
        "size_20000_adjacent_lines": {
            "sfc_bytes": len(sfc),
            "p21_bytes": len(p21),
            "p2z_bytes": len(p2z),
            "cartesian_points": len(points),
        },
        "cad_controls": {
            "sha256": hashes,
            "point_marker_codes": list(range(1, 8)),
            "spline_controls": [4, 4, 7],
            "local_strict_roundtrip": True,
            "native_cad": "not run",
        },
    }
    (output / "report.json").write_text(
        json.dumps(report, indent=2) + "\n", encoding="utf-8"
    )
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output", type=Path, required=True, help="New output directory"
    )
    args = parser.parse_args()
    report = run(args.output)
    print(
        json.dumps(
            {
                "bulk": report["bulk"],
                "skip_seconds": report["skip"]["seconds"],
                "size": report["size_20000_adjacent_lines"],
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
