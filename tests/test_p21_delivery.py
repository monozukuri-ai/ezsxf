"""AP202 callouts/fills and complete, transactional P21/P2Z delivery."""
from __future__ import annotations

import io
import math
import subprocess
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

import ezsxf
from ezsxf._drawing import build_drawing
from ezsxf._p21_drawing import decode_step_string

TIFF = bytes.fromhex(
    "49492a001000000026a2fffe00200200090000010300010000000800000001010300"
    "0100000008000000020103000100000001000000030103000100000004000000"
    "0601030001000000010000001101040001000000080000001601030001000000"
    "080000001701040001000000070000001c010300010000000100000000000000"
)


def records(parsed):
    return {e["id"]: {r["keyword"]: r["parameters"] for r in
            ([e["record"]] if "record" in e else e["records"])}
            for e in parsed["entities"]}


def values(parsed, keyword):
    return [r[keyword] for r in records(parsed).values() if keyword in r]


def annotations():
    doc = ezsxf.new_sfc(timestamp="2026-10-07T00:00:00")
    doc.add_feature("linear_dimension", start_x=10, start_y=30, end_x=60, end_y=30,
                    extension1_present=1, extension1_base_x=10, extension1_base_y=10,
                    extension1_start_x=10, extension1_start_y=12, extension1_end_x=10, extension1_end_y=35,
                    extension2_present=1, extension2_base_x=60, extension2_base_y=10,
                    extension2_start_x=60, extension2_start_y=12, extension2_end_x=60, extension2_end_y=35,
                    arrow1_code=9, arrow1_direction=1, arrow1_x=10, arrow1_y=30, arrow1_scale=.3,
                    arrow2_code=9, arrow2_direction=1, arrow2_x=60, arrow2_y=30, arrow2_scale=.3,
                    text="50", text_x=30, text_y=35, text_height=3, text_width=6)
    for kind, x in [("curve_dimension", 90), ("angular_dimension", 130)]:
        doc.add_feature(kind, center_x=x, center_y=30, radius=20, start_angle=0, end_angle=90,
                        arrow1_code=9, arrow1_direction=1, arrow1_x=x+20, arrow1_y=30, arrow1_scale=.3,
                        arrow2_code=9, arrow2_direction=1, arrow2_x=x, arrow2_y=50, arrow2_scale=.3,
                        text="90" if kind == "angular_dimension" else "31.4", text_x=x, text_y=55,
                        text_height=3, text_width=10)
    doc.add_feature("radius_dimension", start_x=20, start_y=80, end_x=40, end_y=80,
                    arrow1_code=5, arrow1_direction=1, arrow1_x=40, arrow1_y=80, arrow1_scale=.3,
                    text="R20", text_x=25, text_y=85, text_height=3, text_width=8)
    doc.add_feature("diameter_dimension", start_x=70, start_y=80, end_x=110, end_y=80,
                    arrow1_code=9, arrow1_direction=1, arrow1_x=70, arrow1_y=80, arrow1_scale=.3,
                    arrow2_code=9, arrow2_direction=1, arrow2_x=110, arrow2_y=80, arrow2_scale=.3,
                    text="40", text_x=85, text_y=85, text_height=3, text_width=6)
    doc.add_feature("label", vertices=[(140,70),(150,80),(160,80)], arrow_code=9, arrow_scale=.3,
                    text="注記", text_x=155, text_y=85, text_height=3, text_width=8)
    doc.add_feature("balloon", vertices=[(180,70),(190,80)], center_x=195, center_y=80, radius=5,
                    arrow_code=9, arrow_scale=.3, text="1", text_x=195, text_y=80,
                    text_height=3, text_width=3, text_base_point=5)
    return doc


class P21DeliveryTest(unittest.TestCase):
    def test_five_dimensions_and_leaders_keep_semantic_graph(self):
        doc = annotations()
        original = doc.to_dict()
        parsed = ezsxf.parse_p21(doc.to_p21_bytes())
        for kind in ["LINEAR_DIMENSION", "CURVE_DIMENSION", "ANGULAR_DIMENSION", "RADIUS_DIMENSION", "DIAMETER_DIMENSION"]:
            self.assertEqual(len(values(parsed, kind)), 1)
        self.assertEqual(len(values(parsed, "DIMENSION_CALLOUT_RELATIONSHIP")), 5)
        self.assertEqual(len(values(parsed, "LEADER_DIRECTED_CALLOUT")), 2)
        self.assertEqual(len(values(parsed, "PROJECTION_CURVE")), 2)
        self.assertEqual(len(values(parsed, "DIMENSION_CURVE_TERMINATOR")), 9)
        self.assertEqual(len(values(parsed, "LEADER_TERMINATOR")), 2)
        graph = records(parsed)
        bases = []
        for occurrence in graph.values():
            if "PROJECTION_CURVE" in occurrence:
                trimmed = graph[occurrence["STYLED_ITEM"][1]["value"]]["TRIMMED_CURVE"]
                line = graph[trimmed[1]["value"]]["LINE"]
                bases.append(graph[line[1]["value"]]["CARTESIAN_POINT"][1])
        self.assertEqual(bases, [[10,10], [60,10]])
        for data in values(parsed, "TERMINATOR_SYMBOL"):
            self.assertTrue({"DIMENSION_CURVE", "LEADER_CURVE"} & graph[data[0]["value"]].keys())
        source, result = build_drawing(doc.to_bytes()), build_drawing(doc.to_p21_bytes())
        self.assertEqual([t.text for t in result.texts], [t.text for t in source.texts])
        for a, b in zip(source.texts, result.texts):
            for x, y in zip(a.anchor, b.anchor): self.assertAlmostEqual(x, y)
        self.assertEqual(doc.to_dict(), original)

    def test_omitted_members_are_not_generated(self):
        doc = ezsxf.new_sfc()
        doc.add_feature("linear_dimension", end_x=20, text_present=0, arrow1_code=0, arrow2_code=0)
        parsed = ezsxf.parse_p21(doc.to_p21_bytes())
        for kind in ["PROJECTION_CURVE", "DIMENSION_CURVE_TERMINATOR", "TEXT_LITERAL_WITH_EXTENT", "STRUCTURED_DIMENSION_CALLOUT"]:
            self.assertEqual(values(parsed, kind), [])
        self.assertEqual(len(values(parsed, "DIMENSION_CURVE")), 1)

    def test_terminator_codes_follow_feature_and_sfc_specifications(self):
        doc = ezsxf.new_sfc()
        names = ["blanked arrow", "blanked box", "blanked dot", "dimension origin", "filled box", "filled arrow", "filled dot", "integral symbol", "open arrow", "slash", "unfilled arrow"]
        for code in range(1,12):
            doc.add_feature("radius_dimension", end_x=20, arrow1_code=code, arrow1_direction=1, arrow1_x=20, arrow1_scale=.3, text_present=0)
        self.assertEqual([v[0] for v in values(ezsxf.parse_p21(doc.to_p21_bytes()), "PRE_DEFINED_TERMINATOR_SYMBOL")], names)

    def test_all_fill_styles_and_title_metadata_from_authored_fixture(self):
        doc = ezsxf.edit_sfc(ezsxf.parse_sfc((Path(__file__).parent / "fixtures/writer_all_features.sfc").read_bytes()))
        for id_ in (20, 28, 29): doc.remove_element(id_)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            # The authored fixture's SAF dependency is loaded explicitly.
            source = root / "writer-fixture.sfc"
            source.write_bytes(doc.to_bytes(allow_external_references=True))
            (root / "writer-fixture.SAF").write_text('<SxfAttributeXML version="3.0" date="2026-10-07" application="test" sxfFile="writer-fixture.sfc"><Figure id="102" name="line"><AttributeSet name="set" version="1" designedBy="test"><Attr name="材料">鋼</Attr></AttributeSet></Figure></SxfAttributeXML>', encoding="utf-8")
            ezsxf.write_p21_bundle(source, root / "p21")
            parsed = ezsxf.parse_p21(str(root / "p21/writer-fixture.p21"))
        for kind in ["EXTERNALLY_DEFINED_HATCH_STYLE", "FILL_AREA_STYLE_COLOUR", "FILL_AREA_STYLE_HATCHING", "FILL_AREA_STYLE_TILES"]:
            self.assertTrue(values(parsed, kind), kind)
        self.assertEqual(len(values(parsed, "ANNOTATION_FILL_AREA")), 4)
        for area in values(parsed, "ANNOTATION_FILL_AREA"):
            self.assertEqual(len(area[1]), 2)
        self.assertEqual([[decode_step_string(x) for x in row] for row in values(parsed, "DRAWING_DEFINITION")], [["1$$2", "平面図"]])
        self.assertEqual(values(parsed, "CALENDAR_DATE"), [[2026, 5, 10]])
        self.assertEqual([decode_step_string(x[0]) for x in values(parsed, "CONTRACT")], ["工事名"])
        self.assertEqual(len(values(parsed, "DRAUGHTING_ORGANIZATION_ASSIGNMENT")), 2)

    def test_fills_retain_holes_and_boundary_visibility(self):
        doc = ezsxf.new_sfc()
        a = doc.add_polyline([(0,0),(40,0),(40,40),(0,40),(0,0)])
        b = doc.add_arc((20,20), 8, 0, 180)
        c = doc.add_arc((20,20), 8, 180, 0)
        outer = doc.add_composite_curve([a], visible=False)
        hole = doc.add_composite_curve([b,c], visible=True)
        doc.add_fill(outer, holes=[hole], color=2)
        doc.add_hatch(outer, [[1,1,1,0,0,3,30], [2,1,1,5,5,4,120]], holes=[hole])
        parsed = ezsxf.parse_p21(doc.to_p21_bytes())
        self.assertEqual(len(values(parsed, "FILL_AREA_STYLE_HATCHING")), 2)
        graph = records(parsed)
        for data in values(parsed, "ANNOTATION_FILL_AREA_OCCURRENCE"):
            point = graph[data[0]["value"]]["CARTESIAN_POINT"][1]
            self.assertTrue(0 < point[0] < 40 and 0 < point[1] < 40)
            self.assertGreater(math.hypot(point[0]-20, point[1]-20), 8)
        drawing = build_drawing(doc.to_p21_bytes())
        self.assertEqual(len(drawing.fills), 1)
        self.assertEqual(len(drawing.fills[0].holes), 1)
        self.assertTrue(drawing.paths)

    def test_inline_attributes_retain_names(self):
        doc = ezsxf.new_sfc()
        line = doc.add_line((10,10),(50,10))
        text = doc.add_text("工事名", (10,20))
        doc.set_single_attribute(line, "等高線", "等高線", "12.5", attribute_type="LEN", unit="m")
        doc.set_text_attribute(text, "表題_工事名", attribute_type="STR")
        names = [decode_step_string(v[0]) for v in values(ezsxf.parse_p21(doc.to_p21_bytes()), "DRAUGHTING_SUBFIGURE_REPRESENTATION")]
        self.assertTrue(any("$$ATRU$$" in n and n.endswith("$$LEN$$m") for n in names))
        self.assertTrue(any("$$ATRS$$" in n and "表題_工事名" in n for n in names))

    def test_p21_p2z_with_image_saf_and_unicode_rename(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            image = root / "scan.tif"; image.write_bytes(TIFF)
            doc = ezsxf.new_sfc()
            id_ = doc.add_circle((80,50), 5)
            doc.set_attribute(id_, "材料", "鋼 & 鉄", group=["設計", "仕様"])
            doc.add_image(image, (10,20), 40, 20, angle=30, file_name="画像.tif")
            before = doc.to_dict()
            report = doc.save_p21_bundle(root / "out", file_name="日本語.P21")
            self.assertEqual(report["files"], ["日本語.P21", "日本語.SAF", "画像.tif"])
            self.assertEqual((root / "out/画像.tif").read_bytes(), TIFF)
            xml = ET.fromstring((root / "out/日本語.SAF").read_bytes())
            self.assertEqual(xml.attrib["sxfFile"], "日本語.P21")
            self.assertEqual(next(a.text for a in xml.iter("Attr") if a.attrib.get("name") == "材料"), "鋼 & 鉄")
            parsed = ezsxf.parse_p21(str(root / "out/日本語.P21"))
            header = next(r for r in parsed["header"]["entities"] if r["keyword"] == "FILE_NAME")
            self.assertEqual(decode_step_string(header["parameters"][0]), "日本語.P21")
            names = [decode_step_string(v[0]) for v in values(parsed, "DRAUGHTING_SUBFIGURE_REPRESENTATION")]
            self.assertTrue(any("$$ATRF$$" in n and n.endswith("$$日本語.SAF") for n in names))
            self.assertTrue(any("$$画像$$画像.tif$$STR" in n for n in names))
            doc.save_p2z(root / "納品.p2z")
            self.assertEqual(doc.to_p2z_bytes(file_name="納品.p21"), doc.to_p2z_bytes(file_name="納品.p21"))
            with zipfile.ZipFile(root / "納品.p2z") as archive:
                self.assertEqual(sorted(archive.namelist()), ["画像.tif", "納品.SAF", "納品.p21"])
                self.assertIsNone(archive.testzip())
                self.assertTrue(all(e.compress_type == zipfile.ZIP_DEFLATED and e.flag_bits & 1 == 0 for e in archive.infolist()))
                self.assertEqual(archive.read("画像.tif"), TIFF)
                self.assertEqual(ET.fromstring(archive.read("納品.SAF")).attrib["sxfFile"], "納品.p21")
                ezsxf.parse_p21(archive.read("納品.p21"))
            self.assertEqual(doc.to_dict(), before)
            with self.assertRaises(ValueError): doc.to_p21_bytes()

    def test_p2z_contains_only_referenced_files_and_one_drawing(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            doc = ezsxf.new_sfc()
            line = doc.add_line((0,0),(10,0))
            unused = root / "unused.tif"; unused.write_bytes(TIFF)
            doc.add_dependency(unused)
            with zipfile.ZipFile(io.BytesIO(doc.to_p2z_bytes())) as archive:
                self.assertEqual(archive.namelist(), ["drawing.p21"])
            another = root / "another.p21"; another.write_bytes(b"external drawing")
            doc.add_dependency(another)
            doc.set_single_attribute(line, "attachment", "ファイル名", "another.p21")
            with self.assertRaisesRegex(ValueError, "only"): doc.to_p2z_bytes()

    def test_failure_preserves_destination_and_rejects_arbitrary_p2z_attachments(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            doc = ezsxf.new_sfc()
            line = doc.add_line((0,0),(10,0))
            doc.set_single_attribute(line, "attachment", "ファイル名", "note.txt")
            target = root / "delivery.p2z"; target.write_bytes(b"original")
            with self.assertRaisesRegex(ValueError, "Missing"): doc.save_p2z(target)
            self.assertEqual(target.read_bytes(), b"original")
            source = root / "note.txt"; source.write_bytes(b"note")
            doc.add_dependency(source)
            with self.assertRaisesRegex(ValueError, "only"): doc.save_p2z(target)
            self.assertEqual(target.read_bytes(), b"original")
            doc.save_p21_bundle(root / "bundle")
            with self.assertRaises(FileExistsError): doc.save_p21_bundle(root / "bundle")
            with self.assertRaises(ValueError): doc.to_p2z_bytes(file_name="../x.p21")
            empty = ezsxf.new_sfc()
            empty.add_line((0,0),(1,1))
            empty.add_feature("spline")
            with self.assertRaises(ValueError): empty.save_p2z(target)
            self.assertEqual(target.read_bytes(), b"original")

    def test_cli_and_file_based_delivery(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "a.sfc"
            doc = annotations(); doc.save(source)
            ezsxf.write_p2z(source, root / "direct.p2z")
            for command, destination in [("to-p2z", root / "cli.p2z"), ("bundle-p21", root / "bundle")]:
                result = subprocess.run([sys.executable,"-m","ezsxf",command,str(source),str(destination)], capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertTrue(destination.exists())


if __name__ == "__main__": unittest.main()
