from __future__ import annotations

import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

import ezsxf

# Authored 8x8 monochrome G4 strip (Pillow encoder); no imaging dependency at runtime.
TIFF = bytes.fromhex(
    "49492a001000000026a2fffe00200200090000010300010000000800000001010300"
    "0100000008000000020103000100000001000000030103000100000004000000"
    "0601030001000000010000001101040001000000080000001601030001000000"
    "080000001701040001000000070000001c010300010000000100000000000000"
)


class SfcAttributesTest(unittest.TestCase):
    def test_attribute_creation_edit_removal_and_reloaded_bundle(self) -> None:
        doc = ezsxf.new_sfc("original.sfc", timestamp="2026-10-06T12:00:00")
        first = doc.add_line((1, 2), (3, 4))
        middle = doc.add_circle((20, 20), 5)
        last = doc.add_text("図面", (10, 10))
        original_entities = {e["id"]: e for e in doc.to_dict()["entities"]}
        figure = doc.set_attribute(middle, "材料", "鉄 & <鋼>\n", group=["設計", "仕様"])
        doc.set_attribute(middle, "高さ", "12.5", attribute_type="LEN", unit="m")
        doc.set_attribute(first, "memo", "first")
        doc.set_attribute(last, "値", "$$$")
        self.assertEqual(len(doc.to_dict()["model"]["attribute_attachments"]), 3)
        self.assertEqual(len(set(a["attribute"]["figure_id"] for a in doc.to_dict()["model"]["attribute_attachments"])), 3)
        for entity in doc.to_dict()["entities"]:
            if entity["id"] in original_entities:
                self.assertEqual(entity, original_entities[entity["id"]])
        self.assertEqual(doc.set_attribute(middle, "高さ", "13", attribute_type="LEN", unit="m"), figure)
        doc.update_element(middle, radius=6)
        ezsxf.validate_saf(doc.saf_bytes())
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            info = doc.save_bundle(root / "日本語", file_name="納品.sfc")
            self.assertEqual(info["files"], ["納品.SAF", "納品.sfc"])
            saved = root / "日本語" / "納品.sfc"
            original_bytes = saved.read_bytes()
            loaded = ezsxf.edit_sfc_bundle(saved)
            self.assertEqual(loaded.get_attributes(middle), doc.get_attributes(middle))
            loaded.remove_attribute(middle, "材料", group=["設計", "仕様"])
            loaded.remove_attribute(middle, "高さ")
            self.assertEqual(loaded.get_attributes(middle), [])
            self.assertIn(middle, loaded.to_dict()["model"]["sheet"]["component_ids"])
            loaded.remove_element(first)
            loaded.remove_attachment(last)
            self.assertIsNone(loaded.saf_bytes())
            loaded.save_bundle(root / "plain")
            self.assertEqual([p.name for p in (root / "plain").iterdir()], ["納品.sfc"])
            self.assertEqual(saved.read_bytes(), original_bytes)
            with self.assertRaises(FileExistsError):
                doc.save_bundle(root / "日本語")

    def test_inline_attributes_and_transactional_errors(self) -> None:
        doc = ezsxf.new_sfc()
        line = doc.add_line((0, 0), (1, 1))
        text = doc.add_text("工事", (5, 5))
        doc.set_single_attribute(line, "等高線", "等高線", "12", attribute_type="LEN", unit="m")
        doc.set_text_attribute(text, "表題_工事名", attribute_type="STR")
        self.assertEqual(ezsxf.parse_sfc(doc.to_bytes()), doc.to_dict())
        doc.update_element(text, text="変更")
        before = doc.to_dict()
        for operation in (
            lambda: doc.set_text_attribute(line, "表題_工事名"),
            lambda: doc.set_single_attribute(line, "x", "bad$$name", "v"),
            lambda: doc.set_attribute(line, "memo", "cannot silently replace inline"),
        ):
            with self.assertRaises(ValueError):
                operation()
            self.assertEqual(doc.to_dict(), before)
        doc.remove_attachment(line)
        doc.set_attribute(line, "memo", "new")
        before = doc.to_dict()
        saf_before = doc.saf_bytes()
        for kwargs in ({"value": "$$$"}, {"value": "bad\x00"}, {"value": "v", "group": ["a", "b", "c"]}, {"value": "v", "attribute_type": "bad"}):
            with self.assertRaises(ValueError):
                doc.set_attribute(line, "memo", **kwargs)
            self.assertEqual(doc.to_dict(), before)
            self.assertEqual(doc.saf_bytes(), saf_before)

    def test_image_geometry_replacement_dependencies_and_removal(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            image = root / "画像.tif"
            image.write_bytes(TIFF)
            doc = ezsxf.new_sfc("image.sfc")
            id_ = doc.add_image(image, (80, 40), 20, 10)
            polyline = next(f for f in doc.to_dict()["typed_features"] if f["id"] == id_)
            self.assertEqual([(p["x"], p["y"]) for p in polyline["points"]], [(80, 40), (80, 50), (100, 50), (100, 40), (80, 40)])
            doc.update_image(id_, (20, 30), 40, 20, angle=15, image=image, file_name="new.tif")
            result = doc.save_bundle(root / "bundle", file_name="新規.sfc")
            self.assertEqual(result["files"], ["new.tif", "新規.sfc"])
            self.assertIsNone(doc.saf_bytes())
            self.assertEqual((root / "bundle/new.tif").read_bytes(), TIFF)
            loaded = ezsxf.edit_sfc_bundle(root / "bundle/新規.sfc")
            attachment = loaded.to_dict()["model"]["attribute_attachments"][0]
            self.assertEqual(attachment["attribute"]["mechanism"], "ATRU")
            self.assertEqual(attachment["attribute"]["attribute_value"], "new.tif")
            before = loaded.to_dict()
            with self.assertRaises(ValueError):
                loaded.update_element(id_, points=[(0, 0), (1, 1)])
            self.assertEqual(before, loaded.to_dict())
            for payload in (b"opaque", TIFF[:30], TIFF.replace(b"\x03\x01\x03\x00\x01\x00\x00\x00\x04", b"\x03\x01\x03\x00\x01\x00\x00\x00\x01")):
                image.write_bytes(payload)
                with self.assertRaises(ValueError):
                    loaded.add_image(image, (0, 0), 10, 10, file_name="invalid.tif")
                self.assertEqual(before, loaded.to_dict())
            loaded.remove_element(id_)
            loaded.save_bundle(root / "removed")
            self.assertEqual([p.name for p in (root / "removed").iterdir()], ["新規.sfc"])

    def test_legacy_saf_image_edit_preserves_attribute_mechanism(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            image = root / "scan.tif"
            image.write_bytes(TIFF)
            doc = ezsxf.new_sfc()
            id_ = doc.add_polyline([(0, 0), (0, 10), (20, 10), (20, 0), (0, 0)])
            doc.set_attribute(id_, "画像", "scan.tif", set_name="フィーチャ定義属性セット", set_version="1.0", designed_by="SCADEC")
            doc.add_dependency(image)
            doc.save_bundle(root / "legacy")
            saf = root / "legacy/drawing.SAF"
            saf.write_bytes(saf.read_bytes().replace(b'version="1.0"', b'version="0"'))
            loaded = ezsxf.edit_sfc_bundle(root / "legacy/drawing.sfc")
            loaded.update_image(id_, (10, 20), 40, 20, image=image, file_name="replacement.tif")
            loaded.save_bundle(root / "updated")
            self.assertEqual(loaded.get_attributes(id_)[0]["value"], "replacement.tif")
            self.assertEqual(loaded.get_attributes(id_)[0]["set_version"], "0")
            self.assertEqual((root / "updated/replacement.tif").read_bytes(), TIFF)

    def test_missing_file_reference_publishes_nothing(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            doc = ezsxf.new_sfc()
            line = doc.add_line((0, 0), (1, 1))
            doc.set_attribute(line, "ファイル名", "note.txt")
            with self.assertRaisesRegex(OSError, "Missing"):
                doc.save_bundle(root / "output")
            self.assertFalse((root / "output").exists())
            source = root / "note.txt"
            source.write_bytes(b"unchanged attachment")
            doc.add_dependency(source)
            doc.save_bundle(root / "output")
            self.assertEqual((root / "output/note.txt").read_bytes(), source.read_bytes())

    def test_full_saf_grammar_and_legacy_upgrade(self) -> None:
        legacy = b'<SxfAttributeXML version="3.0" date="2026-10-06" application="test" sxfFile="x.sfc"><Figure id="1" name="line"><AttributeSet name="set" version="1" designedBy="test"><Attr name="note">ok</Attr></AttributeSet></Figure></SxfAttributeXML>'
        ezsxf.validate_saf(legacy)
        with self.assertRaises(ValueError):
            ezsxf.validate_saf(legacy.replace(b'"3.0"', b'"3.1"'))
        doc = ezsxf.new_sfc()
        id_ = doc.add_line((0, 0), (1, 1))
        doc.set_attribute(id_, "note", "ok")
        xml = doc.saf_bytes()
        for invalid in (
            xml.replace(b'<AttrSetRef id="1">', b'<AttrSetRef id="9">'),
            xml.replace(b'designedBy="ezsxf"', b'unknown="ezsxf"'),
            xml.replace(b'</Figure>', b'<Attr name="bad">v</Attr></Figure>'),
            xml.replace(b'</AttrSetRef>', b'<Attr name="bad"><Unknown/></Attr></AttrSetRef>'),
            xml.replace(b'</SxfAttributeXML>', b'<Figure id="1" name="duplicate"><AttrSetRef id="1"><Attr name="a">v</Attr></AttrSetRef></Figure></SxfAttributeXML>'),
            b'<!DOCTYPE SxfAttributeXML [<!ENTITY x "unsafe">]>' + xml.split(b'?>', 1)[1],
        ):
            with self.subTest(xml=invalid), self.assertRaises(ValueError):
                ezsxf.validate_saf(invalid)
        root = ET.fromstring(xml)
        self.assertEqual(root.tag, "SxfAttributeXML")
        self.assertIsNotNone(root.find("AttributeSet"))
        self.assertIsNotNone(root.find("Figure/AttrSetRef/Attr"))


if __name__ == "__main__":
    unittest.main()
