"""Bulk transactions, sheet authoring, part placements and AP202 output."""

from __future__ import annotations

import copy
import math
import tempfile
import unittest
from pathlib import Path

import ezsxf
from ezsxf._drawing import build_drawing


def document():
    return ezsxf.new_sfc(timestamp="2026-10-07T00:00:00")


def features(doc):
    return {f["id"]: f for f in doc.to_dict()["typed_features"]}


class WriterRequestsTest(unittest.TestCase):
    def assert_drawing_equal(self, left, right):
        self.assertEqual(left.warnings, [])
        self.assertEqual(right.warnings, [])
        self.assertEqual(len(left.paths), len(right.paths))
        self.assertEqual(len(left.texts), len(right.texts))
        for a, b in zip(left.paths, right.paths):
            self.assertEqual(a.closed, b.closed)
            self.assertEqual(a.style, b.style)
            self.assertEqual(len(a.points), len(b.points))
            for p, q in zip(a.points, b.points):
                self.assertAlmostEqual(p[0], q[0], places=8)
                self.assertAlmostEqual(p[1], q[1], places=8)
        for a, b in zip(left.texts, right.texts):
            for name in ("text", "base_point", "direction", "style"):
                self.assertEqual(getattr(a, name), getattr(b, name))
            for name in ("height", "width", "angle_deg"):
                self.assertAlmostEqual(getattr(a, name), getattr(b, name), places=8)
            for p, q in zip(a.anchor, b.anchor):
                self.assertAlmostEqual(p, q, places=8)

    def test_bulk_matches_individual_factories_and_preserves_existing_group(self):
        bulk, individual = document(), document()
        inputs = [
            {"kind": "line", "start": (1, 2), "end": (10, 20)},
            {"kind": "circle", "center": (40, 30), "radius": 5},
            {"kind": "arc", "center": (50, 60), "radius": 8,
             "start_angle": 30, "end_angle": 270},
            {"kind": "polyline", "points": [(10, 40), (20, 50), (30, 40)]},
            {"kind": "text", "text": "東京都ABC", "anchor": (2, 80)},
        ]
        expected = [
            individual.add_line((1, 2), (10, 20)),
            individual.add_circle((40, 30), 5),
            individual.add_arc((50, 60), 8, 30, 270),
            individual.add_polyline([(10, 40), (20, 50), (30, 40)]),
            individual.add_text("東京都ABC", (2, 80)),
        ]
        self.assertEqual(bulk.extend(iter(inputs)), expected)
        # The scalar spelling may be "1" or "1.0"; resolved values must agree.
        for key in ("typed_features", "model", "header"):
            self.assertEqual(bulk.to_dict()[key], individual.to_dict()[key])
        self.assert_drawing_equal(
            build_drawing(bulk.to_bytes()), build_drawing(individual.to_bytes())
        )
        group = bulk.group_elements("group", expected[:2])
        before = bulk.to_dict()
        extra = bulk.extend([{"kind": "ellipse", "radius_x": 12},
                             {"kind": "point_marker", "x": 80}])
        after = bulk.to_dict()
        self.assertEqual(after["model"]["sfig_definitions"],
                         before["model"]["sfig_definitions"])
        self.assertEqual(after["model"]["sheet"]["component_ids"],
                         [group] + expected[2:] + extra)
        self.assertEqual(ezsxf.parse_sfc(bulk.to_bytes()), after)

    def test_bulk_late_errors_and_generator_errors_are_atomic(self):
        doc = document()
        doc.add_circle((1, 2), 3)
        before = doc.to_dict()
        for bad in (
            {"kind": "circle", "radius": -1},
            {"kind": "line", "start": (0, True)},
            {"kind": "line", "start_x": "1"},
            {"kind": "text", "text": "😀"},
            {"kind": "text", "text": "A", "height": 1e300},
            {"kind": "line", "layer": 99},
            {"kind": "line", "start": (1, 2), "start_x": 1},
            {"kind": "unknown"},
            {"radius": 10},
            123,
        ):
            with self.subTest(bad=bad), self.assertRaises((ValueError, TypeError)):
                doc.extend([{"kind": "circle"}, bad])
            self.assertEqual(doc.to_dict(), before)

        def broken():
            yield {"kind": "circle"}
            raise RuntimeError("input failed")

        with self.assertRaisesRegex(RuntimeError, "input failed"):
            doc.extend(broken())
        self.assertEqual(doc.extend([]), [])
        self.assertEqual(doc.to_dict(), before)

    def test_standard_papers_and_integral_floats(self):
        sizes = [(841, 1189), (594, 841), (420, 594), (297, 420), (210, 297)]
        for code, (short, long) in enumerate(sizes):
            for orientation, expected in (("portrait", (short, long)),
                                          ("landscape", (long, short))):
                for paper in (code, f"A{code}"):
                    doc = ezsxf.new_sfc(paper=paper, orientation=orientation,
                                       width_mm=float(expected[0]), height_mm=float(expected[1]))
                    sheet = next(f for f in features(doc).values()
                                 if f["kind"] == "drawing_sheet")
                    self.assertEqual(sheet["sheet_type"], code)
                    self.assertEqual((sheet["free_x_mm"], sheet["free_y_mm"]), expected)
                    self.assertEqual(ezsxf.parse_sfc(doc.to_bytes()), doc.to_dict())
                    self.assertIn(f"A{code}_", doc.to_p21_bytes().decode("ascii"))
        doc = ezsxf.new_sfc(width_mm=841.0, height_mm=594.0)
        self.assertEqual(features(doc)[6]["free_x_mm"], 841)
        for fields in ({"width_mm": 841.5}, {"width_mm": True},
                       {"width_mm": "841"}, {"width_mm": math.nan},
                       {"width_mm": math.inf}, {"paper": "A5"},
                       {"paper": 5}, {"paper": True}, {"orientation": 2},
                       {"paper": "A1", "width_mm": 297.0}):
            with self.subTest(fields=fields), self.assertRaises((ValueError, TypeError)):
                ezsxf.new_sfc(**fields)

    def test_text_width_estimation_explicit_width_and_vertical_contract(self):
        self.assertEqual(ezsxf.estimate_text_width("ＡＢＣABC", height=10, spacing=5), 70)
        self.assertEqual(ezsxf.estimate_text_width("ｱA", height=4, spacing=1), 5)
        doc = document()
        auto = doc.add_text("東京都ABC", (1, 2), height=4, spacing=1)
        explicit = doc.add_text("東京都ABC", (3, 4), width=40)
        vertical = doc.add_text("縦書き", (5, 6), direction=2, width=10)
        values = features(doc)
        self.assertEqual(values[auto]["width"], 23)
        self.assertEqual(values[explicit]["width"], 40)
        before = doc.to_dict()
        for operation in (lambda: doc.add_text("縦", (0, 0), direction=2),
                          lambda: ezsxf.estimate_text_width("😀"),
                          lambda: ezsxf.estimate_text_width("A\nB"),
                          lambda: ezsxf.estimate_text_width("A", spacing=-1)):
            with self.assertRaises(ValueError):
                operation()
            self.assertEqual(doc.to_dict(), before)
        self.assertEqual(values[vertical]["direction"], 2)

    def test_shared_part_only_renders_explicit_placements_and_nested_scale(self):
        doc = document()
        line = doc.add_line((0, 0), (10, 0))
        text = doc.add_text("縦書き", (5, 6), angle=30, width=10, direction=2)
        placements = doc.create_part("部品", [line, text], [
            {"position": (100, 0)},
            {"position": (200, 30), "scale": (2, 3), "angle": 90},
        ])
        self.assertEqual(doc.to_dict()["model"]["sheet"]["component_ids"], placements)
        sfc = build_drawing(doc.to_bytes())
        self.assertEqual(len(sfc.paths), 2)
        self.assertEqual(len(sfc.texts), 2)
        self.assertEqual(sfc.paths[0].points, ((100, 0), (110, 0)))
        self.assertEqual(ezsxf.to_dxf(doc.to_dict()).count("\r\nLINE\r\n"), 2)
        self.assert_drawing_equal(sfc, build_drawing(doc.to_p21_bytes()))
        # No placement or definition body is rendered a second time as a root.
        doc.group_elements("部分図", placements, kind=1, position=(10, 20), scale=(0.5, 0.25))
        self.assert_drawing_equal(
            build_drawing(doc.to_bytes()), build_drawing(doc.to_p21_bytes())
        )
        before = doc.to_dict()
        for placements in ([], [{"position": (0, 0), "scale": (0, 1)}],
                           [{"position": (0, 0)}, {"position": (True, 0)}]):
            with self.assertRaises((ValueError, TypeError)):
                doc.create_part("bad", [line], placements)
            self.assertEqual(doc.to_dict(), before)

    def test_p21_curves_major_arcs_and_all_text_base_points(self):
        doc = document()
        doc.extend([
            {"kind": "line", "start": (2, 3), "end": (20, 30)},
            {"kind": "circle", "center": (50, 50), "radius": 8},
            {"kind": "polyline", "points": [(0, 0), (2, 3), (4, 0)]},
            *[{"kind": "arc", "center": (30, 40), "radius": 5,
               "start_angle": start, "end_angle": end, "direction": direction}
              for start, end, direction in [(30, 300, 0), (300, 30, 1), (350, 10, 0), (10, 350, 1)]],
            *[{"kind": "text", "text": "日本語 ' C:\\temp", "anchor": (10 * base, 80),
               "angle": 30, "base_point": base, "width": 30}
              for base in range(1, 10)],
        ])
        data = doc.to_p21_bytes()
        self.assertTrue(data.isascii())
        self.assertNotIn(b"/*SXF", data)
        self.assertIn(b"AP202_mode", data)
        self.assertIn(b"\\X2\\", data)
        self.assert_drawing_equal(build_drawing(doc.to_bytes()), build_drawing(data))
        parsed = ezsxf.parse_p21(data)
        self.assertEqual(parsed["warnings"], [])
        self.assertEqual(ezsxf.serialize_p21(doc.to_dict()), data)
        stale = copy.deepcopy(doc.to_dict())
        stale["typed_features"][-2]["width"] = 999
        with self.assertRaisesRegex(ValueError, "disagrees"):
            ezsxf.serialize_p21(stale)

    def test_p21_prefix_does_not_reduce_valid_part_name_length(self):
        doc = document()
        line = doc.add_line((0, 0), (10, 0))
        doc.create_part("a" * 256, [line], [{"position": (100, 0)}])
        self.assert_drawing_equal(build_drawing(doc.to_bytes()), build_drawing(doc.to_p21_bytes()))

    def test_p21_text_baseline_coordinates_match_independent_corpus_convention(self):
        doc = document()
        for base in (1, 4, 7):
            doc.add_text("AB", (10, 20), height=4, width=4, base_point=base, angle=90)
        parsed = ezsxf.parse_p21(doc.to_p21_bytes())
        records = {e["id"]: e["record"] for e in parsed["entities"] if "record" in e}
        points = []
        for record in records.values():
            if record["keyword"] == "TEXT_LITERAL_WITH_EXTENT":
                axis = records[record["parameters"][2]["value"]]
                point = records[axis["parameters"][1]["value"]]
                points.append(point["parameters"][1])
        # Lower/middle/upper box anchors use three distinct baseline origins.
        self.assertEqual(points, [[8.0, 20.0], [12.0, 20.0], [14.0, 20.0]])

    def test_p21_styles_unicode_fonts_hidden_layers_and_custom_pattern(self):
        doc = document()
        layer = doc.add_layer("非表示層", visible=False)
        rgb = doc.add_color((12, 34, 56))
        line_type = doc.add_line_type("独自線", pattern=[5, 2, 1, 2])
        width = doc.add_line_width(0.42)
        font = doc.add_font("ＭＳ 明朝")
        doc.add_line((1, 2), (3, 4), color=rgb, line_type=line_type, line_width=width, layer=layer)
        doc.add_text("縦書き", (20, 30), width=12, direction=2, font=font, layer=layer)
        doc.extend([{"kind": "line", "start": (code * 2, 0), "end": (code * 2, 10),
                     "color": code, "line_type": min(code, 15), "line_width": min(code, 9)}
                    for code in range(1, 17)])
        self.assert_drawing_equal(build_drawing(doc.to_bytes()), build_drawing(doc.to_p21_bytes()))
        parsed = ezsxf.parse_p21(doc.to_p21_bytes())
        patterns = [e["record"]["parameters"] for e in parsed["entities"]
                    if e.get("record", {}).get("keyword") == "CURVE_STYLE_FONT_PATTERN"]
        self.assertEqual(patterns, [[5.0, 2.0], [1.0, 2.0]])

    def test_p21_unsupported_features_do_not_replace_files_and_cli_works(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            doc = document()
            doc.add_line((0, 0), (10, 10))
            output = root / "出力.p21"
            doc.save_p21(output)
            original = output.read_bytes()
            ezsxf.write_p21(doc.to_dict(), output)
            self.assertEqual(output.read_bytes(), original)
            sfc = root / "input.sfc"
            doc.save(sfc)
            self.assertEqual(ezsxf.main(["to-p21", str(sfc), str(output)]), 0)
            self.assertEqual(output.read_bytes(), original)
            doc.add_feature("spline")
            with self.assertRaisesRegex(ValueError, "spline_feature"):
                doc.save_p21(output)
            self.assertEqual(output.read_bytes(), original)


if __name__ == "__main__":
    unittest.main()
