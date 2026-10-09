"""Consumer regressions for bulk structures, indexed rejection and P21 preflight."""

import re
import unittest

import ezsxf


def document(**kwargs):
    return ezsxf.new_sfc(timestamp="2026-10-08T00:00:00", **kwargs)


def line(x=0):
    return {"kind": "line", "start": (x, 0), "end": (x, 10)}


def rectangle(x=0):
    return [(x, 0), (x + 10, 0), (x + 10, 10), (x, 10)]


def graph(parsed):
    return {
        e["id"]: {
            r["keyword"]: r["parameters"]
            for r in ([e["record"]] if "record" in e else e["records"])
        }
        for e in parsed["entities"]
    }


class WriterRequestsV2Test(unittest.TestCase):
    def test_bulk_composite_boundary_can_be_used_by_individual_fill(self):
        doc = document()
        boundary = doc.extend(
            [
                {
                    "kind": "composite_curve",
                    "visible": True,
                    "elements": [
                        {"kind": "polyline", "points": rectangle() + [(0, 0)]}
                    ],
                }
            ]
        )[0]
        fill = doc.add_fill(boundary)
        reference = doc.to_dict()["model"]["hatch_references"][0]
        self.assertEqual(reference["hatch_id"], fill)
        self.assertEqual(reference["outer_definition_id"], boundary)
        self.assertEqual(len(ezsxf.build_drawing(doc.to_p21_bytes()).fills), 1)

    def test_bulk_keeps_legacy_sfc_missing_optional_style_tables(self):
        source = document().to_bytes().decode("cp932")
        source = re.sub(
            r"/\*SXF\r\n#[15] = .*?\r\nSXF\*/\r\n", "", source, flags=re.DOTALL
        )
        doc = ezsxf.edit_sfc(ezsxf.parse_sfc(source))
        doc.extend([line(), {"kind": "text", "text": "legacy", "anchor": (0, 20)}])
        self.assertEqual(ezsxf.parse_sfc(doc.to_bytes())["warnings"], [])

    def test_bulk_structures_preserve_existing_sheet_and_boundary_codes(self):
        doc = document()
        old_line = doc.add_line((80, 0), (80, 10))
        old_curve = doc.add_composite_curve(
            [doc.add_polyline(rectangle(50) + [(50, 0)])]
        )
        old_fill = doc.add_fill(old_curve)
        ids = doc.extend(
            [
                {"kind": "fill", "outer": rectangle()},
                {
                    "kind": "hatch",
                    "outer": rectangle(20),
                    "patterns": [[1, 1, 1, 0, 0, 2, 45]],
                },
                {
                    "kind": "part",
                    "name": "shared",
                    "elements": [line()],
                    "placements": [
                        {"position": (100, 0)},
                        {"position": (120, 0), "scale": (2, 1)},
                    ],
                },
                {
                    "kind": "partial_drawing",
                    "name": "detail",
                    "elements": [line(5)],
                    "position": (150, 0),
                    "scale": (0.5, 0.5),
                },
                {"kind": "group", "name": "group", "elements": [line(200)]},
            ]
        )
        self.assertEqual(len(ids), 5)
        self.assertEqual(len(ids[2]), 2)
        model = doc.to_dict()["model"]
        self.assertIn(old_line, model["sheet"]["component_ids"])
        reference = next(
            r for r in model["hatch_references"] if r["hatch_id"] == old_fill
        )
        self.assertEqual(reference["outer_definition_id"], old_curve)
        self.assertEqual(ezsxf.parse_sfc(doc.to_bytes())["warnings"], [])
        sfc, p21 = (
            ezsxf.build_drawing(doc.to_bytes()),
            ezsxf.build_drawing(doc.to_p21_bytes()),
        )
        self.assertEqual(sfc.warnings, [])
        self.assertEqual(p21.warnings, [])
        self.assertEqual(len(sfc.paths), len(p21.paths))
        self.assertEqual(len(sfc.fills), len(p21.fills))

    def test_nested_parts_and_explicit_reuse_have_no_origin_instance(self):
        doc = document()
        ids = doc.extend(
            [
                {
                    "kind": "part",
                    "name": "bolt",
                    "elements": [line()],
                    "placements": [{"position": (10, 0)}],
                },
                {
                    "kind": "part",
                    "name": "assembly",
                    "elements": [
                        {"kind": "placement", "name": "bolt", "position": (3, 0)},
                        {
                            "kind": "part",
                            "name": "nested",
                            "elements": [line()],
                            "placements": [{"position": (6, 0)}],
                        },
                    ],
                    "placements": [{"position": (100, 0)}, {"position": (200, 0)}],
                },
            ]
        )
        self.assertEqual([len(i) for i in ids], [1, 2])
        drawing = ezsxf.build_drawing(doc.to_bytes())
        self.assertEqual(
            sorted(p.points[0][0] for p in drawing.paths), [10, 103, 106, 203, 206]
        )
        self.assertEqual(len(doc.to_dict()["model"]["sfig_definitions"]), 3)

    def test_arc_and_spline_boundaries_with_holes(self):
        doc = document()
        hole = [
            {
                "kind": "arc",
                "center": (5, 5),
                "radius": 2,
                "start_angle": 0,
                "end_angle": 180,
            },
            {
                "kind": "arc",
                "center": (5, 5),
                "radius": 2,
                "start_angle": 180,
                "end_angle": 0,
            },
        ]
        doc.extend([{"kind": "fill", "outer": rectangle(), "holes": [hole]}])
        for payload in (doc.to_bytes(), doc.to_p21_bytes()):
            drawing = ezsxf.build_drawing(payload)
            self.assertEqual(len(drawing.fills), 1)
            self.assertEqual(len(drawing.fills[0].holes), 1)
            self.assertEqual(drawing.warnings, [])
        spline = [
            {
                "kind": "spline",
                "points": [(0, 0), (15, 0), (0, 15), (0, 0)],
                "open_close": 0,
            }
        ]
        second = document()
        second.extend([{"kind": "fill", "outer": spline}])
        self.assertEqual(len(ezsxf.build_drawing(second.to_p21_bytes()).fills), 1)

    def test_into_existing_definition_keeps_sheet_ownership(self):
        doc = document()
        placement = doc.extend(
            [
                {
                    "kind": "partial_drawing",
                    "name": "detail",
                    "elements": [line()],
                    "scale": (0.01, 0.01),
                }
            ]
        )[0]
        definition = doc.to_dict()["model"]["sfig_definitions"][0]["entity_id"]
        added = doc.extend([line(50), line(60)], into=definition)
        model = doc.to_dict()["model"]
        self.assertEqual(model["sheet"]["component_ids"], [placement])
        self.assertTrue(
            set(added) <= set(model["sfig_definitions"][0]["component_ids"])
        )
        more = doc.extend([line(70)], into=placement)
        self.assertIn(
            more[0], doc.to_dict()["model"]["sfig_definitions"][0]["component_ids"]
        )
        self.assertEqual(ezsxf.parse_sfc(doc.to_bytes())["warnings"], [])

    def test_into_skip_rejects_forward_and_recursive_part_references(self):
        doc = document()
        first = doc.extend(
            [
                {
                    "kind": "part",
                    "name": "first",
                    "elements": [line()],
                    "placements": [{"position": (10, 0)}],
                }
            ]
        )[0][0]
        doc.extend(
            [
                {
                    "kind": "part",
                    "name": "later",
                    "elements": [line()],
                    "placements": [{"position": (20, 0)}],
                }
            ]
        )
        ids, rejected = doc.extend(
            [
                line(50),
                {"kind": "placement", "name": "later"},
                {"kind": "placement", "name": "first"},
            ],
            into=first,
            on_invalid="skip",
        )
        self.assertIsNotNone(ids[0])
        self.assertEqual(ids[1:], [None, None])
        self.assertEqual([i for i, _ in rejected], [1, 2])
        self.assertTrue(all("forward or recursive" in reason for _, reason in rejected))
        self.assertEqual(ezsxf.parse_sfc(doc.to_bytes())["warnings"], [])

    def test_raise_errors_always_name_input_and_leave_document_unchanged(self):
        doc = document()
        doc.add_circle((5, 5), 1)
        original = doc.to_dict()
        bad = [
            {"kind": "circle", "radius": -1},
            {"kind": "line", "layer": 99},
            {"kind": "line", "start": (True, 0)},
            {"kind": "text", "text": "😀"},
            {"kind": "line", "foo": 2},
            42,
            {"kind": "composite_curve"},
        ]
        for value in bad:
            with (
                self.subTest(value=value),
                self.assertRaisesRegex(ValueError, "Element 1:"),
            ):
                doc.extend([line(), value])
            self.assertEqual(doc.to_dict(), original)

    def test_skip_reports_positions_and_discards_entire_invalid_fragment(self):
        doc = document()
        bad_part = {
            "kind": "part",
            "name": "reusable",
            "elements": [line()],
            "placements": [{"position": (1, 1), "scale": (-1, 1)}],
        }
        good_part = dict(bad_part, placements=[{"position": (20, 20)}])
        ids, rejected = doc.extend(
            [
                line(),
                {"kind": "circle", "radius": -1},
                bad_part,
                good_part,
                {"kind": "fill", "outer": [(0, 0), (10, 0), (20, 0)]},
                line(2),
            ],
            on_invalid="skip",
        )
        self.assertEqual([i for i, _ in rejected], [1, 2, 4])
        self.assertEqual([i for i, value in enumerate(ids) if value is None], [1, 2, 4])
        self.assertEqual(len(ids[3]), 1)
        self.assertIn("positive", rejected[1][1])
        self.assertEqual(
            [d["name"] for d in doc.to_dict()["model"]["sfig_definitions"]],
            ["reusable"],
        )
        self.assertEqual(ezsxf.parse_sfc(doc.to_bytes())["warnings"], [])

    def test_generator_failure_aborts_even_skip_mode(self):
        doc = document()
        before = doc.to_dict()

        def broken():
            yield line()
            raise RuntimeError("source failed")

        with self.assertRaisesRegex(RuntimeError, "Element 1:.*source failed"):
            doc.extend(broken(), on_invalid="skip")
        self.assertEqual(doc.to_dict(), before)

    def test_nonpositive_placement_scales_are_explicit_for_all_entrypoints(self):
        doc = document()
        member = doc.add_line((0, 0), (0, 1))
        part = doc.create_part("valid", [member], [{"position": (10, 10)}])[0]
        loose = doc.add_line((0, 0), (1, 1))
        before = doc.to_dict()
        for scale in [(-1, 1), (0, 1), (1, -1), (float("nan"), 1)]:
            operations = [
                lambda scale=scale: doc.place_part(
                    part, position=(10, 10), scale=scale
                ),
                lambda scale=scale: doc.create_part(
                    "bad", [loose], [{"position": (1, 1), "scale": scale}]
                ),
                lambda scale=scale: doc.group_elements(
                    "bad", [loose], kind=1, scale=scale
                ),
                lambda scale=scale: doc.extend(
                    [
                        {
                            "kind": "group",
                            "name": "bad",
                            "elements": [line()],
                            "scale": scale,
                        }
                    ]
                ),
                lambda scale=scale: doc.extend(
                    [
                        {
                            "kind": "part",
                            "name": "bad",
                            "elements": [line()],
                            "placements": [{"position": (1, 1), "scale": scale}],
                        }
                    ]
                ),
            ]
            for operation in operations:
                with self.assertRaisesRegex(ValueError, "scale.*positive"):
                    operation()
                self.assertEqual(doc.to_dict(), before)
        doc.place_part(part, position=(30, 30), scale=(2, 0.5))

    def test_all_seven_markers_and_cubic_splines_round_trip(self):
        doc = document()
        doc.extend(
            [
                {
                    "kind": "point_marker",
                    "x": n * 10,
                    "y": 20,
                    "marker": n,
                    "angle": 30,
                    "scale": 2,
                }
                for n in range(1, 8)
            ]
        )
        for closed, controls in [
            (False, [(0, 0), (2, 5), (5, 5), (10, 0)]),
            (False, [(0, 0), (2, 5), (5, 5), (10, 0), (15, -5), (18, -5), (20, 0)]),
            (True, [(0, 0), (10, 0), (0, 10), (0, 0)]),
            (True, [(0, 0), (2, 5), (5, 5), (10, 0), (15, -5), (18, -5), (0, 0)]),
        ]:
            doc.extend(
                [
                    {
                        "kind": "spline",
                        "points": controls,
                        "open_close": 0 if closed else 1,
                    }
                ]
            )
        source, result = (
            ezsxf.build_drawing(doc.to_bytes()),
            ezsxf.build_drawing(doc.to_p21_bytes()),
        )
        records = graph(ezsxf.parse_p21(doc.to_p21_bytes()))
        self.assertEqual(sum("BEZIER_CURVE" in r for r in records.values()), 6)
        self.assertEqual(sum("COMPOSITE_CURVE" in r for r in records.values()), 2)
        self.assertFalse(any("POLYLINE" in r for r in records.values()))
        self.assertEqual(result.warnings, [])
        self.assertEqual(
            [(m.position, m.marker_code, m.scale) for m in source.markers],
            [(m.position, m.marker_code, m.scale) for m in result.markers],
        )
        self.assertEqual(len(source.paths), len(result.paths))
        for a, b in zip(source.paths, result.paths):
            self.assertEqual(a.closed, b.closed)
            self.assertEqual(len(a.points), len(b.points))
            for p, q in zip(a.points, b.points):
                for x, y in zip(p, q):
                    self.assertAlmostEqual(x, y, places=6)

    def test_explicit_drop_reports_dependencies_without_changing_source(self):
        doc = document()
        kept = doc.add_circle((50, 50), 1)
        clothoid = doc.add_feature("clothoid")
        geo_member = doc.add_line((0, 0), (1, 1))
        geo = doc.group_elements("survey", [geo_member], kind=2)
        doc.extend(
            [
                {
                    "kind": "part",
                    "name": "mixed",
                    "elements": [line(), {"kind": "clothoid"}],
                    "placements": [{"position": (20, 0)}],
                }
            ]
        )
        before = doc.to_dict()
        with self.assertRaises(ValueError):
            doc.to_p21_bytes()
        payload, dropped = doc.to_p21_bytes(unsupported="drop", report=True)
        self.assertIn(clothoid, {id_ for id_, _ in dropped})
        self.assertTrue({geo_member, geo}.isdisjoint({id_ for id_, _ in dropped}))
        self.assertNotIn(kept, {id_ for id_, _ in dropped})
        result = ezsxf.build_drawing(payload)
        self.assertEqual(result.warnings, [])
        self.assertEqual(len(result.paths), 3)
        self.assertEqual(doc.to_dict(), before)
        doc.to_bytes()

    def test_p21_preflight_and_target_reject_with_entity_ids(self):
        source = document()
        zero = source.add_line((1, 1), (1, 1))
        spline = source.add_feature("spline")
        issues = dict(source.validate_p21())
        self.assertIn("zero-length", issues[zero])
        self.assertIn("3n+1", issues[spline])
        with self.assertRaisesRegex(ValueError, f"Entity #{zero}:"):
            source.to_p21_bytes()
        target = document(target="p21")
        before = target.to_dict()
        for operation in [
            lambda: target.add_line((1, 1), (1, 1)),
            lambda: target.add_feature("clothoid"),
            lambda: target.extend(
                [line(), {"kind": "line", "start": (0, 0), "end": (0, 0)}]
            ),
        ]:
            with self.assertRaisesRegex(ValueError, r"Entity #\d+:"):
                operation()
            self.assertEqual(target.to_dict(), before)
        target.extend([line()])
        self.assertEqual(target.validate_p21(), [])

    def test_p21_preflight_includes_degenerate_projection_and_leader(self):
        doc = document()
        projection = doc.add_feature(
            "linear_dimension",
            end_x=10,
            extension1_present=1,
            extension1_start_x=0,
            extension1_start_y=0,
            extension1_end_x=0,
            extension1_end_y=0,
        )
        leader = doc.add_feature("label", vertices=[(0, 0), (0, 0), (10, 0)])
        issues = dict(doc.validate_p21())
        self.assertIn("projection", issues[projection])
        self.assertIn("leader", issues[leader])

    def test_zero_area_fill_is_rejected_at_addition_in_both_targets(self):
        for target in ["sfc", "p21"]:
            doc = document(target=target)
            boundary = doc.add_composite_curve(
                [doc.add_polyline([(0, 0), (10, 0), (20, 0), (0, 0)])]
            )
            before = doc.to_dict()
            with self.assertRaisesRegex(ValueError, r"Entity #\d+:.*no interior point"):
                doc.add_fill(boundary)
            self.assertEqual(doc.to_dict(), before)
            with self.assertRaisesRegex(
                ValueError, r"Element 0: Entity #\d+:.*no interior point"
            ):
                doc.extend([{"kind": "fill", "outer": [(0, 0), (10, 0), (20, 0)]}])
            self.assertEqual(doc.to_dict(), before)

    def test_shared_cartesian_points_are_unique_and_lines_use_start_as_basis(self):
        doc = document()
        doc.extend([line(10), line(10), line(20)])
        records = graph(ezsxf.parse_p21(doc.to_p21_bytes()))
        coordinates = [
            tuple(r["CARTESIAN_POINT"][1])
            for r in records.values()
            if "CARTESIAN_POINT" in r
        ]
        self.assertEqual(len(coordinates), len(set(coordinates)))
        for r in records.values():
            if "TRIMMED_CURVE" not in r:
                continue
            trim = r["TRIMMED_CURVE"]
            basis = records[trim[1]["value"]]["LINE"]
            self.assertEqual(basis[1], trim[2][0])
        self.assertEqual(ezsxf.build_drawing(doc.to_p21_bytes()).warnings, [])

    def test_depth_limit_and_invalid_options_are_atomic(self):
        doc = document()
        before = doc.to_dict()
        element = line()
        for n in range(34):
            element = {
                "kind": "part",
                "name": str(n),
                "elements": [element],
                "placements": [{"position": (0, 0)}],
            }
        with self.assertRaisesRegex(ValueError, "Element 0:.*depth 32"):
            doc.extend([element])
        with self.assertRaisesRegex(ValueError, "on_invalid"):
            doc.extend([], on_invalid="ignore")
        with self.assertRaisesRegex(ValueError, "unsupported"):
            doc.to_p21_bytes(unsupported="ignore")
        self.assertEqual(doc.to_dict(), before)


if __name__ == "__main__":
    unittest.main()
