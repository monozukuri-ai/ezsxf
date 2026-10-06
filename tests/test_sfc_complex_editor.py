import unittest
from pathlib import Path

import ezsxf


class ComplexEditorTest(unittest.TestCase):
    def test_all_complex_leaf_kinds_round_trip_and_edit(self):
        doc = ezsxf.new_sfc()
        inputs = {
            "point_marker": {"x": 20, "y": 30, "marker": 7},
            "ellipse": {
                "center_x": 25,
                "center_y": 30,
                "radius_x": 12,
                "radius_y": 6,
                "angle": 15,
            },
            "ellipse_arc": {
                "radius_x": 20,
                "radius_y": 10,
                "start_angle": 10,
                "end_angle": 90,
            },
            "spline": {"points": [(0, 0), (2, 8), (7, 8), (10, 0)]},
            "clothoid": {"parameter": 100, "start_length": 0, "end_length": 50},
            "linear_dimension": {
                "start_x": 10,
                "end_x": 50,
                "end_y": 15,
                "text": "40 日本語",
                "text_x": 25,
                "text_y": 20,
                "arrow1_code": 1,
                "arrow1_x": 10,
            },
            "angular_dimension": {
                "radius": 20,
                "start_angle": 10,
                "end_angle": 90,
                "text": "80",
            },
            "curve_dimension": {
                "radius": 20,
                "start_angle": 0,
                "end_angle": 90,
                "text": "arc",
            },
            "radius_dimension": {"end_x": 25, "text": "R25"},
            "diameter_dimension": {"start_x": -25, "end_x": 25, "text": "50"},
            "label": {"vertices": [(0, 0), (5, 5), (10, 5)], "text": "注記"},
            "balloon": {
                "vertices": [(0, 0), (10, 10)],
                "center_x": 15,
                "center_y": 10,
                "radius": 5,
                "text": "1",
            },
        }
        ids = {kind: doc.add_feature(kind, **fields) for kind, fields in inputs.items()}
        doc.update_element(ids["ellipse"], radius_y=8, angle=30)
        doc.update_element(ids["spline"], points=[(0, 0), (3, 9), (7, 9), (10, 0)])
        doc.update_element(
            ids["linear_dimension"],
            text="更新",
            extension1_present=1,
            extension1_end_y=5,
        )
        doc.update_element(ids["label"], vertices=[(1, 1), (6, 6)])
        parsed = ezsxf.parse_sfc(doc.to_bytes(), strict=True)
        self.assertEqual(parsed, doc.to_dict())
        byid = {f["id"]: f for f in parsed["typed_features"]}
        self.assertEqual(byid[ids["ellipse"]]["radius_y"], 8)
        self.assertEqual(byid[ids["linear_dimension"]]["text"]["text"], "更新")
        self.assertEqual(byid[ids["spline"]]["declared_point_count"], 4)
        before = doc.to_dict()
        for fields in [
            {"radius_x": -1},
            {"radius_x": float("nan")},
            {"radius_x": 1.1234567},
            {"radius_x": True},
            {"bogus": 1},
        ]:
            with self.assertRaises((ValueError, TypeError)):
                doc.update_element(ids["ellipse"], **fields)
            self.assertEqual(doc.to_dict(), before)

    def test_existing_group_and_boundary_children_are_editable_without_restructuring(
        self,
    ):
        fixture = Path(__file__).parent / "fixtures/writer_all_features.sfc"
        doc = ezsxf.edit_sfc(ezsxf.parse_sfc(str(fixture), strict=True))
        before = doc.to_dict()
        doc.update_element(14, end_x=22)
        doc.update_element(10, points=[(0, 0), (12, 0), (12, 12), (0, 12), (0, 0)])
        after = doc.to_dict()
        self.assertEqual(after["model"], before["model"])
        self.assertEqual(
            [e for e in after["entities"] if e["id"] not in [10, 14]],
            [e for e in before["entities"] if e["id"] not in [10, 14]],
        )

    def test_nested_group_creation_rename_insert_and_ungroup(self):
        doc = ezsxf.new_sfc()
        a = doc.add_circle((0, 0), 1)
        b = doc.add_circle((20, 20), 3)
        c = doc.add_line((1, 1), (5, 5))
        inner = doc.group_elements("内側", [b])
        outer = doc.group_elements("外側", [inner, c])
        doc.rename_group(inner, "変更後")
        d = doc.add_feature("ellipse", center_x=10)
        doc.add_to_group(outer, [d])
        doc.update_element(b, radius=4)
        self.assertEqual(doc.to_dict()["model"]["sheet"]["component_ids"], [a, outer])
        before = doc.to_dict()
        for op in [
            lambda: doc.group_elements("bad", [a, a]),
            lambda: doc.group_elements("$$ATRF$$bad", [a]),
            lambda: doc.update_element(inner, x=1),
            lambda: doc.add_to_group(outer, [outer]),
            lambda: doc.rename_group(outer, "変更後"),
        ]:
            with self.assertRaises(ValueError):
                op()
            self.assertEqual(doc.to_dict(), before)
        doc.ungroup(inner)
        doc.ungroup(outer)
        self.assertEqual(doc.to_dict()["model"]["sheet"]["component_ids"], [a, b, c, d])
        self.assertEqual(ezsxf.parse_sfc(doc.to_bytes()), doc.to_dict())

    def test_parts_share_definition_and_reference_safe_removal(self):
        doc = ezsxf.new_sfc()
        child = doc.add_circle((0, 0), 2)
        first = doc.group_elements("部品", [child], kind=4, layer=1)
        second = doc.place_part(first, position=(25, 30), angle=15, scale=(2, 1))
        doc.rename_group(first, "改名")
        model = doc.to_dict()["model"]
        self.assertEqual(len(model["sfig_definitions"]), 1)
        self.assertEqual(len(model["sfig_references"]), 2)
        doc.remove_element(second)
        before = doc.to_dict()
        with self.assertRaises(ValueError):
            doc.remove_element(first)
        self.assertEqual(doc.to_dict(), before)

    def test_fill_hatch_holes_and_boundary_release_preserve_other_definitions(self):
        doc = ezsxf.new_sfc()
        untouched = doc.add_circle((100, 100), 10)
        group = doc.group_elements("unchanged", [untouched])
        a = doc.add_polyline([(0, 0), (20, 0), (20, 20), (0, 20), (0, 0)])
        b = doc.add_polyline([(5, 5), (10, 5), (10, 10), (5, 10), (5, 5)])
        outer = doc.add_composite_curve([a])
        hole = doc.add_composite_curve([b])
        fill = doc.add_fill(outer, holes=[hole], color=2)
        hatch = doc.add_hatch(outer, [[1, 1, 1, 0, 0, 2, 45]], holes=[hole])
        model = doc.to_dict()["model"]
        self.assertEqual(model["sfig_definitions"][0]["component_ids"], [untouched])
        self.assertEqual(model["hatch_references"][0]["inner_definition_ids"], [hole])
        before = doc.to_dict()
        for op in [
            lambda: doc.release_composite_curve(hole),
            lambda: doc.add_hatch(outer, [[1, 1, 1, 0, 0, 2]]),
            lambda: doc.add_fill(outer, holes=[outer]),
            lambda: doc.add_fill(9999),
        ]:
            with self.assertRaises((ValueError, TypeError)):
                op()
            self.assertEqual(doc.to_dict(), before)
        doc.update_hatch_boundaries(fill, outer)
        doc.update_hatch_boundaries(hatch, outer)
        doc.update_hatch_patterns(
            hatch, [[2, 1, 1, 0, 0, 3, 30], [1, 1, 1, 0, 0, 4, 120]]
        )
        doc.update_element(hatch, layer=1)
        doc.release_composite_curve(hole)
        self.assertEqual(
            doc.to_dict()["model"]["sfig_definitions"][0]["component_ids"], [untouched]
        )
        self.assertIn(b, doc.to_dict()["model"]["sheet"]["component_ids"])
        self.assertIn(group, doc.to_dict()["model"]["sheet"]["component_ids"])
        self.assertEqual(ezsxf.parse_sfc(doc.to_bytes()), doc.to_dict())


if __name__ == "__main__":
    unittest.main()
