from __future__ import annotations

import copy
import tempfile
import unittest
from pathlib import Path

import ezsxf

FIXTURE = Path(__file__).parent / "fixtures" / "writer_all_features.sfc"


class SfcEditorTest(unittest.TestCase):
    def test_create_edit_remove_and_save_all_basic_primitives(self) -> None:
        doc = ezsxf.new_sfc("created.sfc", name="図面", width_mm=300, height_mm=200)
        layer = doc.add_layer("構造")
        font = doc.add_font("Arial")
        ids = [
            doc.add_line((0, 0), (100, 50), layer=layer),
            doc.add_circle((50, 50), 10, layer=layer),
            doc.add_arc((40, 30), 5, 0, 90),
            doc.add_polyline([(0, 0), (10, 0), (10, 10)]),
            doc.add_text("日本語 '), \\path", (10, 80), font=font),
        ]
        self.assertEqual(len(set(ids)), 5)
        doc.update_element(ids[0], end_x=120.125, color=2)
        doc.update_element(ids[1], radius=15)
        doc.update_element(ids[2], start_angle=30, end_angle=120)
        doc.update_element(ids[3], points=[(1, 2), (3, 4), (5, 6)])
        doc.update_element(ids[4], text="変更後", height=5)
        doc.rename_layer(layer, "編集後")
        snapshot = doc.to_dict()
        by_id = {f["id"]: f for f in snapshot["typed_features"]}
        self.assertEqual(by_id[ids[0]]["end"]["x"], 120.125)
        self.assertEqual(by_id[ids[1]]["radius"], 15)
        self.assertEqual(by_id[ids[4]]["text"], "変更後")
        self.assertEqual(snapshot["model"]["sheet"]["component_ids"], ids)
        self.assertEqual(ezsxf.parse_sfc(doc.to_bytes()), snapshot)
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "日本語.sfc"
            doc.save(output)
            doc.save(output)  # also verify replacing an existing file on Windows
            self.assertEqual(ezsxf.parse_sfc(str(output)), snapshot)
        doc.remove_element(ids[1])
        self.assertNotIn(ids[1], doc.to_dict()["model"]["sheet"]["component_ids"])

    def test_failed_edit_is_transactional(self) -> None:
        doc = ezsxf.new_sfc()
        circle = doc.add_circle((0, 0), 10)
        before = doc.to_dict()
        for changes in (
            {"radius": -1},
            {"radius": float("nan")},
            {"radius": 1.1234567},
            {"layer": 99},
            {"color": 999},
            {"unknown": 1},
            {"radius": "1"},
            {"radius": True},
        ):
            with (
                self.subTest(changes=changes),
                self.assertRaises((ValueError, TypeError)),
            ):
                doc.update_element(circle, **changes)
            self.assertEqual(doc.to_dict(), before)
        for operation in (
            lambda: doc.add_circle((0, 0), -1),
            lambda: doc.add_line((0, 0), (float("inf"), 1)),
            lambda: doc.add_polyline([(0, 0)]),
            lambda: doc.add_text("😀", (0, 0)),
        ):
            with self.assertRaises(ValueError):
                operation()
            self.assertEqual(doc.to_dict(), before)

    def test_edit_existing_preserves_complex_structure_and_input(self) -> None:
        parsed = ezsxf.parse_sfc(str(FIXTURE))
        original = copy.deepcopy(parsed)
        doc = ezsxf.edit_sfc(parsed)
        self.assertEqual(doc.to_dict(), original)
        doc.update_element(21, end_x=25)
        result = doc.to_dict()
        self.assertEqual(result["model"], original["model"])
        for before, after in zip(original["entities"], result["entities"]):
            if before["id"] != 21:
                self.assertEqual(before, after)
        self.assertEqual(parsed, original)
        with self.assertRaisesRegex(ValueError, "external SAF"):
            doc.to_bytes()
        self.assertEqual(
            ezsxf.parse_sfc(doc.to_bytes(allow_external_references=True)), result
        )

    def test_group_components_definitions_and_unknown_ids_cannot_be_edited(
        self,
    ) -> None:
        doc = ezsxf.edit_sfc(ezsxf.parse_sfc(str(FIXTURE)))
        before = doc.to_dict()
        for entity_id in (14, 900, 999, 123456):
            with self.subTest(entity_id=entity_id):
                with self.assertRaises(ValueError):
                    doc.update_element(entity_id, radius=2)
                with self.assertRaises(ValueError):
                    doc.remove_element(entity_id)
                self.assertEqual(doc.to_dict(), before)

    def test_definition_codes_and_existing_styles_remain_stable(self) -> None:
        doc = ezsxf.edit_sfc(ezsxf.parse_sfc(str(FIXTURE)))
        before = doc.to_dict()
        self.assertEqual(doc.add_layer("追加"), 3)
        self.assertEqual(doc.add_font("Arial"), 2)
        after = doc.to_dict()
        self.assertEqual(
            after["model"]["code_tables"]["layers"][:2],
            before["model"]["code_tables"]["layers"],
        )
        for old, new in zip(
            before["typed_features"],
            [
                f
                for f in after["typed_features"]
                if f["id"] in {e["id"] for e in before["entities"]}
            ],
        ):
            self.assertEqual(old, new)

    def test_snapshot_is_independent_and_constructor_rejects_stale_cache(self) -> None:
        doc = ezsxf.new_sfc()
        snapshot = doc.to_dict()
        snapshot["model"]["sheet"]["component_ids"].append(777)
        self.assertEqual(doc.to_dict()["model"]["sheet"]["component_ids"], [])
        with self.assertRaisesRegex(ValueError, "disagrees"):
            ezsxf.edit_sfc(snapshot)
        for dimensions in ((0, 10), (10, -1)):
            with self.assertRaises(ValueError):
                ezsxf.new_sfc(width_mm=dimensions[0], height_mm=dimensions[1])


if __name__ == "__main__":
    unittest.main()
