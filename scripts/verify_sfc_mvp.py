"""Qualify an installed writer package; generate owned inputs for CAD review."""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
from pathlib import Path

import ezsxf


def create_drawing() -> ezsxf.SfcDocument:
    doc = ezsxf.new_sfc("mvp.sfc", name="基本図面", timestamp="2026-10-06T00:00:00")
    layer = doc.add_layer("構造")
    red = doc.add_color("red")
    rgb = doc.add_color((32, 96, 160))
    dashed = doc.add_line_type("dashed")
    custom = doc.add_line_type("独自線", pattern=[5, 2, 1, 2])
    standard_width = doc.add_line_width(0.35)
    custom_width = doc.add_line_width(0.42)
    assert (red, rgb, dashed, custom, standard_width, custom_width) == (
        2,
        17,
        2,
        17,
        4,
        11,
    )
    styles = {
        "layer": layer,
        "color": rgb,
        "line_type": custom,
        "line_width": custom_width,
    }
    line = doc.add_line((10, 10), (100, 10))
    circle = doc.add_circle((60, 60), 20, **styles)
    doc.add_arc(
        (120, 60),
        20,
        30,
        150,
        direction=0,
        layer=layer,
        color=red,
        line_type=dashed,
        line_width=standard_width,
    )
    doc.add_polyline([(10, 100), (40, 120), (70, 100)], **styles)
    doc.add_text(
        r"日本語 C:\temp\図面.sfc", (10, 150), height=3.5, width=90, layer=layer
    )
    doc.update_element(line, end_x=100.125)
    doc.update_element(circle, radius=25)
    before = doc.to_dict()
    assert doc.add_color([32, 96, 160]) == rgb
    assert doc.add_line_type("独自線", pattern=[5, 2, 1, 2]) == custom
    assert doc.add_line_width(0.42) == custom_width
    assert doc.to_dict() == before
    return doc


def verify(output: Path) -> dict:
    output.mkdir(parents=True, exist_ok=False)
    doc = create_drawing()
    snapshot = doc.to_dict()
    assert not snapshot["warnings"]
    assert ezsxf.parse_sfc(doc.to_bytes(), strict=True) == snapshot
    assert ezsxf.serialize_sfc(snapshot) == doc.to_bytes()
    assert create_drawing().to_bytes() == doc.to_bytes()
    standard = output / "mvp.sfc"
    doc.save(standard)
    doc.save(standard)
    assert ezsxf.parse_sfc(standard.read_bytes(), strict=True) == snapshot
    japanese = output / "日本語.sfc"
    doc.save(japanese)
    doc.save(japanese)
    assert japanese.read_bytes() == standard.read_bytes()
    literal = output / "mvp-literal.sfc"
    doc.save(literal, literal_backslashes=True)
    assert ezsxf.parse_sfc(literal.read_bytes(), strict=True) == snapshot
    invalid = [
        lambda: doc.add_text("😀", (0, 0)),
        lambda: doc.add_line_width(0.130000001),
        lambda: doc.add_line_type("invalid", pattern=[0, 2]),
        lambda: doc.update_element(
            snapshot["model"]["sheet"]["component_ids"][0], end_x=float("nan")
        ),
        lambda: doc.add_circle((0, 0), -1),
    ]
    for operation in invalid:
        try:
            operation()
        except ValueError:
            pass
        else:
            raise AssertionError("Invalid operation succeeded")
        assert doc.to_dict() == snapshot
        assert japanese.read_bytes() == standard.read_bytes()
    bad = dict(snapshot, warnings=[{"code": "partial", "message": "test"}])
    before = standard.read_bytes()
    try:
        ezsxf.write_sfc(bad, standard)
    except ValueError:
        pass
    else:
        raise AssertionError("Partial output was saved")
    assert standard.read_bytes() == before
    edited = ezsxf.edit_sfc(ezsxf.parse_sfc(standard.read_bytes(), strict=True))
    assert edited.to_bytes() == before
    for name in ("_core.pyi", "py.typed"):
        assert (Path(ezsxf.__file__).parent / name).is_file(), f"Missing {name}"
    report = {
        "version": ezsxf.__version__,
        "platform": platform.platform(),
        "package": str(Path(ezsxf.__file__).resolve()),
        "checks": {
            key: True
            for key in (
                "five_primitives_and_styles",
                "strict_structural_roundtrip",
                "deterministic_output",
                "existing_file_replace",
                "japanese_path",
                "standard_and_literal_backslashes",
                "invalid_edit_rollback",
                "failed_save_preserves_file",
                "type_information",
            )
        },
        "sha256": {
            p.name: hashlib.sha256(p.read_bytes()).hexdigest()
            for p in output.glob("*.sfc")
        },
        "cad_review": "not performed by this script",
    }
    (output / "verification.json").write_text(
        json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    return report


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output", type=Path, required=True, help="new evidence directory"
    )
    args = parser.parse_args()
    # Console encodings on native Windows can be CP1252 even when files are UTF-8.
    print(json.dumps(verify(args.output), ensure_ascii=True))


if __name__ == "__main__":
    main()
