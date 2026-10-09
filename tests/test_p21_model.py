"""Common SFC/P21 structural metadata and geodetic writer regressions."""

import unittest

import ezsxf


def features(parsed, kind):
    return [item for item in parsed["typed_features"] if item["kind"] == kind]


def document(**kwargs):
    return ezsxf.new_sfc(timestamp="2026-10-09T00:00:00", **kwargs)


class P21ModelTest(unittest.TestCase):
    def test_geodetic_export_keeps_sfc_coordinates_and_unequal_scale(self):
        for angle, scale in [(0, (1, 1)), (66.88, (0.02, 0.02)), (330, (2, 0.5))]:
            with self.subTest(angle=angle, scale=scale):
                doc = document(target="p21")
                ids = doc.extend(
                    [
                        {"kind": "line", "start": (1, 2), "end": (4, 6)},
                        {"kind": "circle", "center": (8, 9), "radius": 2},
                    ]
                )
                doc.group_elements(
                    "測地座標系",
                    ids,
                    kind=2,
                    position=(100, -200),
                    angle=angle,
                    scale=scale,
                )
                self.assertEqual(doc.validate_p21(), [])
                source = ezsxf.build_drawing(doc.to_bytes())
                payload = doc.to_p21_bytes()
                parsed = ezsxf.parse_p21(payload)
                result = ezsxf.build_drawing(parsed)
                self.assertEqual(result.warnings, [])
                self.assertEqual(len(result.paths), len(source.paths))
                for original, emitted in zip(source.paths, result.paths):
                    self.assertEqual(len(original.points), len(emitted.points))
                    for p, q in zip(original.points, emitted.points):
                        for a, b in zip(p, q):
                            self.assertAlmostEqual(a, b, places=7)
                self.assertEqual(parsed["model"]["sfig_definitions"][0]["kind_flag"], 2)
                self.assertEqual(
                    features(parsed, "sfig_locate")[0]["name"], "測地座標系"
                )
                _, dropped = doc.to_p21_bytes(unsupported="drop", report=True)
                self.assertEqual(dropped, [])

    def test_common_sheet_metadata_for_all_standard_and_free_papers(self):
        for paper in ["A0", "A1", "A2", "A3", "A4", "FREE"]:
            for landscape in [False, True]:
                with self.subTest(paper=paper, landscape=landscape):
                    size = (
                        {
                            "width_mm": 510 if landscape else 330,
                            "height_mm": 330 if landscape else 510,
                        }
                        if paper == "FREE"
                        else {}
                    )
                    doc = document(
                        name="図面 '日本語'",
                        paper=paper,
                        orientation="landscape" if landscape else "portrait",
                        **size,
                    )
                    source, parsed = doc.to_dict(), ezsxf.parse_p21(doc.to_p21_bytes())
                    original = features(source, "drawing_sheet")[0]
                    emitted = features(parsed, "drawing_sheet")[0]
                    for key in [
                        "kind",
                        "name",
                        "sheet_type",
                        "orientation",
                        "free_x_mm",
                        "free_y_mm",
                    ]:
                        self.assertEqual(emitted[key], original[key], key)
                    self.assertEqual(set(parsed["model"]), set(source["model"]))
                    self.assertEqual(
                        set(parsed["model"]["sheet"]), set(source["model"]["sheet"])
                    )
                    self.assertEqual(
                        parsed["model"]["sheet"]["entity_id"], emitted["id"]
                    )
                    self.assertEqual(parsed["warnings"], [])

    def test_nested_parts_groups_and_partial_drawings_use_common_references(self):
        doc = document()
        doc.extend(
            [
                {
                    "kind": "group",
                    "name": "線群",
                    "elements": [
                        {
                            "kind": "part",
                            "name": "部品",
                            "elements": [
                                {"kind": "line", "start": (1, 2), "end": (3, 4)}
                            ],
                            "placements": [
                                {"position": (10, 20), "angle": 30, "scale": (2, 1)},
                                {"position": (-10, -20), "angle": 330, "scale": (1, 2)},
                            ],
                        },
                    ],
                },
            ]
        )
        doc.group_elements(
            "測地",
            doc.to_dict()["model"]["sheet"]["component_ids"],
            kind=2,
            position=(100, 200),
            angle=66.88,
            scale=(0.5, 0.25),
        )
        source, parsed = doc.to_dict(), ezsxf.parse_p21(doc.to_p21_bytes())
        expected = [
            (item["name"], item["kind_flag"])
            for item in source["model"]["sfig_definitions"]
        ]
        self.assertEqual(
            [
                (item["name"], item["kind_flag"])
                for item in parsed["model"]["sfig_definitions"]
            ],
            expected,
        )
        self.assertEqual(len(parsed["model"]["sfig_references"]), 4)
        by_id = {item["id"]: item for item in parsed["typed_features"]}
        definitions = {
            item["entity_id"]: item for item in parsed["model"]["sfig_definitions"]
        }
        for ref in parsed["model"]["sfig_references"]:
            self.assertEqual(
                by_id[ref["placement_id"]]["name"],
                definitions[ref["definition_id"]]["name"],
            )
        before, after = features(source, "sfig_locate"), features(parsed, "sfig_locate")
        for original, emitted in zip(before, after):
            self.assertEqual(original["name"], emitted["name"])
            for key in ["angle_deg", "ratio_x", "ratio_y"]:
                self.assertAlmostEqual(original[key], emitted[key], places=8)
            for key in ["x", "y"]:
                self.assertAlmostEqual(
                    original["position"][key], emitted["position"][key], places=8
                )
        # Common-model traversal reaches the P21 renderer's actual occurrence IDs.
        refs = {
            ref["placement_id"]: ref["definition_id"]
            for ref in parsed["model"]["sfig_references"]
        }
        reachable, pending = (
            set(),
            [
                definitions[
                    next(id_ for id_, d in definitions.items() if d["kind_flag"] == 2)
                ]
            ],
        )
        while pending:
            for member in pending.pop()["component_ids"]:
                reachable.add(member)
                if member in refs:
                    pending.append(definitions[refs[member]])
        drawing = ezsxf.build_drawing(parsed)
        self.assertTrue({path.source_id for path in drawing.paths} <= reachable)
        self.assertEqual(len(drawing.paths), 2)
        self.assertEqual(drawing.warnings, [])

    def test_fractional_free_paper_and_unicode_x4_metadata(self):
        doc = document(name="", paper="FREE", width_mm=510, height_mm=330)
        payload = doc.to_p21_bytes().decode("ascii")
        payload = payload.replace("510.", "510.25").replace("330.", "330.75")
        # The writer uses X2; independent X4 input must also be decoded by the reader.
        payload = payload.replace(
            "'JAPANESE',''", "'JAPANESE','\\X4\\000056F3000097620001F600\\X0\\'"
        )
        parsed = ezsxf.parse_p21(payload)
        sheet = features(parsed, "drawing_sheet")[0]
        self.assertEqual(sheet["sheet_type"], 9)
        self.assertEqual((sheet["free_x_mm"], sheet["free_y_mm"]), (510.25, 330.75))
        self.assertEqual(sheet["name"], "図面😀")
        self.assertEqual(parsed["warnings"], [])

    def test_free_paper_matching_standard_dimensions_stays_free(self):
        doc = document(paper="FREE", width_mm=420, height_mm=297)
        sheet = features(ezsxf.parse_p21(doc.to_p21_bytes()), "drawing_sheet")[0]
        self.assertEqual(sheet["sheet_type"], 9)

    def test_multiple_sheets_model_and_renderer_use_the_first_sheet(self):
        doc = document(paper="A3")
        doc.add_line((1, 2), (3, 4))
        payload = doc.to_p21_bytes().decode("ascii")
        original = ezsxf.parse_p21(payload)
        next_id = max(item["id"] for item in original["entities"]) + 1
        second = f"#{next_id}=DRAWING_SHEET_REVISION('A4_vertical',(),$,'02');\r\n"
        payload = payload.replace(
            "ENDSEC;\r\nEND-ISO-10303-21;", second + "ENDSEC;\r\nEND-ISO-10303-21;"
        )
        parsed = ezsxf.parse_p21(payload)
        self.assertEqual(parsed["model"]["sheet"], original["model"]["sheet"])
        self.assertEqual(len(parsed["warnings"]), 1)
        self.assertEqual(parsed["warnings"][0]["code"], "p21-sheet-model")
        drawing = ezsxf.build_drawing(parsed)
        self.assertEqual(drawing.paths[0].points, ((1.0, 2.0), (3.0, 4.0)))
        self.assertEqual(len(drawing.paths), 1)

    def test_attribute_and_dimension_membership_use_common_model(self):
        doc = document()
        line = doc.add_line((1, 2), (3, 4))
        doc.set_single_attribute(line, "部材", "材料", "鋼")
        dimension = doc.add_feature(
            "linear_dimension", end_x=20, text="20", text_width=7
        )
        doc.group_elements(
            "測地", [dimension], kind=2, position=(100, 200), angle=66.88
        )
        source, parsed = doc.to_dict(), ezsxf.parse_p21(doc.to_p21_bytes())
        original = source["model"]["attribute_attachments"][0]
        emitted = parsed["model"]["attribute_attachments"][0]
        for key in ["name", "kind_flag", "attribute"]:
            self.assertEqual(emitted[key], original[key])
        self.assertEqual(len(emitted["placement_ids"]), 1)
        self.assertEqual(parsed["warnings"], [])
        # Expand figure/attribute references using only the common model contract.
        model = parsed["model"]
        definitions = {d["entity_id"]: d for d in model["sfig_definitions"]}
        definitions[emitted["definition_id"]] = emitted
        refs = {r["placement_id"]: r["definition_id"] for r in model["sfig_references"]}
        refs.update({p: emitted["definition_id"] for p in emitted["placement_ids"]})
        reachable, pending = set(), list(definitions.values())
        while pending:
            for member in pending.pop()["component_ids"]:
                reachable.add(member)
                if member in refs:
                    pending.append(definitions[refs[member]])
        drawing = ezsxf.build_drawing(parsed)
        for collection in [
            drawing.paths,
            drawing.texts,
            drawing.fills,
            drawing.markers,
        ]:
            self.assertTrue({item.source_id for item in collection} <= reachable)
        self.assertEqual(drawing.warnings, [])


if __name__ == "__main__":
    unittest.main()
