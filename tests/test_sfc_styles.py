"""Public style factories, transactional validation and imported-code stability."""

from __future__ import annotations

import unittest
from pathlib import Path

import ezsxf


class SfcStyleTest(unittest.TestCase):
    def test_style_creation_reuse_and_primitive_references(self) -> None:
        doc = ezsxf.new_sfc(timestamp="2026-10-06T00:00:00")
        red = doc.add_color(" RED ")
        rgb = doc.add_color((12, 34, 56))
        dashed = doc.add_line_type("dashed")
        custom = doc.add_line_type("独自線", pattern=[5, 2, 1, 2])
        width = doc.add_line_width(0.42)
        self.assertEqual((red, rgb, dashed, custom, width), (2, 17, 2, 17, 11))
        styles = {"color": rgb, "line_type": custom, "line_width": width}
        ids = [
            doc.add_line((0, 0), (10, 20), **styles),
            doc.add_circle((20, 20), 5, **styles),
            doc.add_arc((40, 20), 5, 30, 150, direction=1, **styles),
            doc.add_polyline([(0, 40), (10, 50), (20, 40)], **styles),
        ]
        doc.add_text(r"日本語 C:\temp\drawing.sfc", (0, 70), color=red, width=70)
        before = doc.to_dict()
        for operation, expected in (
            (lambda: doc.add_color([12, 34, 56]), rgb),
            (lambda: doc.add_color("red"), red),
            (lambda: doc.add_line_type("dashed"), dashed),
            (lambda: doc.add_line_type("独自線", pattern=(5, 2, 1, 2)), custom),
            (lambda: doc.add_line_width(0.42), width),
        ):
            self.assertEqual(operation(), expected)
            self.assertEqual(doc.to_dict(), before)
        self.assertEqual(ezsxf.parse_sfc(doc.to_bytes(), strict=True), before)
        for feature in before["typed_features"]:
            if feature["id"] in ids:
                self.assertEqual(feature["style"]["color_code"], rgb)
                self.assertEqual(feature["style"]["line_type_code"], custom)
                self.assertEqual(feature["style"]["line_width_code"], width)

    def test_invalid_python_types_and_definitions_preserve_document(self) -> None:
        doc = ezsxf.new_sfc()
        before = doc.to_dict()
        operations = (
            [
                lambda value=value: doc.add_color(value)
                for value in (
                    True,
                    2,
                    (1, 2),
                    (1, 2, 3, 4),
                    (True, 2, 3),
                    (1.0, 2, 3),
                    ("1", 2, 3),
                    (-1, 2, 3),
                    (256, 2, 3),
                    "unknown",
                )
            ]
            + [
                lambda value=value: doc.add_line_width(value)
                for value in (
                    True,
                    "0.42",
                    0,
                    -1,
                    float("nan"),
                    float("inf"),
                    0.130000001,
                )
            ]
            + [
                lambda value=value: doc.add_line_type("custom", pattern=value)
                for value in (
                    "1,2",
                    [True, 2],
                    ["1", 2],
                    [0, 2],
                    [1, 2, 3],
                    [1.1234567, 2],
                )
            ]
        )
        for operation in operations:
            with self.assertRaises((TypeError, ValueError)):
                operation()
            self.assertEqual(doc.to_dict(), before)

    def test_imported_tables_and_entities_keep_codes_and_ids(self) -> None:
        source = (Path(__file__).parent / "fixtures/writer_all_features.sfc").read_text(
            encoding="utf-8"
        )
        # Definitions with uppercase keywords must also be appended correctly.
        source = source.replace(
            "user_defined_colour_feature", "USER_DEFINED_COLOUR_FEATURE"
        )
        doc = ezsxf.edit_sfc(ezsxf.parse_sfc(source))
        before = doc.to_dict()
        doc.add_color((5, 6, 7))
        doc.add_line_type("追加線", pattern=[4, 2])
        doc.add_line_width(0.42)
        after = doc.to_dict()
        for table in ("colors", "line_types", "line_widths"):
            for binding in before["model"]["code_tables"][table]:
                self.assertIn(binding, after["model"]["code_tables"][table])
        for entity in before["entities"]:
            self.assertIn(entity, after["entities"])
        self.assertEqual(
            ezsxf.parse_sfc(doc.to_bytes(allow_external_references=True)), after
        )


if __name__ == "__main__":
    unittest.main()
