"""Synthetic detector regressions; these do not qualify a third-party CAD."""

import importlib.util
import re
import tempfile
import unittest
from pathlib import Path

import ezsxf

SCRIPT = Path(__file__).parents[1] / "scripts/verify_sfc_reexport.py"
spec = importlib.util.spec_from_file_location("verify_sfc_reexport", SCRIPT)
verification = importlib.util.module_from_spec(spec)
spec.loader.exec_module(verification)


class ReexportVerificationTest(unittest.TestCase):
    def test_group_loss_and_membership_change_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            document = ezsxf.new_sfc(timestamp="2026-10-06T00:00:00")
            circle = document.add_circle((20, 20), 10)
            group = document.group_elements("kept", [circle])
            source, exported = root / "source.sfc", root / "exported.sfc"
            document.save(source)
            document.save(exported)
            self.assertTrue(verification.verify(source, exported)["passed"])
            document.rename_group(group, "CAD-renamed")
            document.save(exported)
            renamed = verification.verify(source, exported)
            self.assertFalse(renamed["passed"])
            self.assertTrue(
                renamed["checks"]["geometry_hierarchy_styles_attributes_images"]
            )
            self.assertTrue(renamed["checks"]["group_definition_reuse"])
            self.assertFalse(renamed["checks"]["group_names_and_reuse"])
            document.update_element(circle, radius=12)
            document.save(exported)
            self.assertFalse(verification.verify(source, exported)["passed"])
            document.ungroup(group)
            document.save(exported)
            self.assertFalse(verification.verify(source, exported)["passed"])

    def test_saf_rename_passes_but_value_change_and_missing_file_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            document = ezsxf.new_sfc(timestamp="2026-10-06T00:00:00")
            circle = document.add_circle((20, 20), 10)
            document.set_attribute(circle, "材料", "鉄 & 鋼", group=["設計"])
            note = root / "notes.txt"
            note.write_bytes(b"owned dependency")
            document.add_dependency(note)
            document.set_attribute(circle, "ファイル名", "notes.txt")
            document.save_bundle(root / "source", file_name="source.sfc")
            document.save_bundle(root / "exported", file_name="exported.sfc")
            source, exported = (
                root / "source/source.sfc",
                root / "exported/exported.sfc",
            )
            self.assertTrue(verification.verify(source, exported)["passed"])
            dependency = root / "exported/notes.txt"
            original = dependency.read_bytes()
            dependency.write_bytes(b"changed dependency")
            self.assertFalse(verification.verify(source, exported)["passed"])
            dependency.write_bytes(original)
            output_bytes = exported.read_bytes()
            exported.write_bytes(output_bytes.replace(b"$$ATRF$$", b"-000-$$ATRF$$"))
            self.assertFalse(verification.verify(source, exported)["passed"])
            exported.write_bytes(output_bytes)
            document.set_attribute(circle, "材料", "違う値", group=["設計"])
            document.save_bundle(root / "changed", file_name="changed.sfc")
            self.assertFalse(
                verification.verify(source, root / "changed/changed.sfc")["passed"]
            )
            (root / "exported/exported.SAF").unlink()
            self.assertFalse(verification.verify(source, exported)["passed"])

    def test_ids_can_change_but_shared_part_reuse_cannot_be_lost(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            document = ezsxf.new_sfc(timestamp="2026-10-06T00:00:00")
            circle = document.add_circle((20, 20), 10)
            part = document.group_elements("part", [circle], kind=4)
            document.place_part(part, position=(60, 0))
            source, exported = root / "source.sfc", root / "exported.sfc"
            document.save(source)
            data = source.read_bytes()
            # All record IDs change together; SXF references use names/codes.
            data = re.sub(
                rb"#(\d+)", lambda match: b"#" + str(int(match[1]) + 100).encode(), data
            )
            exported.write_bytes(data)
            self.assertTrue(verification.verify(source, exported)["passed"])
            flattened = ezsxf.new_sfc(timestamp="2026-10-06T00:00:00")
            flattened.add_circle((20, 20), 10)
            flattened.add_circle((80, 20), 10)
            flattened.save(exported)
            self.assertFalse(verification.verify(source, exported)["passed"])

    def test_image_rename_passes_but_bytes_placement_and_missing_file_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            image = root / "scan.tif"
            image.write_bytes(
                bytes.fromhex(
                    "49492a001000000026a2fffe00200200090000010300010000000800000001010300"
                    "0100000008000000020103000100000001000000030103000100000004000000"
                    "0601030001000000010000001101040001000000080000001601030001000000"
                    "080000001701040001000000070000001c010300010000000100000000000000"
                )
            )
            document = ezsxf.new_sfc(timestamp="2026-10-06T00:00:00")
            raster = document.add_image(image, (20, 20), 40, 20, angle=15)
            document.save_bundle(root / "source", file_name="source.sfc")
            document.update_image(
                raster, (20, 20), 40, 20, angle=15, image=image, file_name="renamed.tif"
            )
            document.save_bundle(root / "exported", file_name="exported.sfc")
            source, exported = (
                root / "source/source.sfc",
                root / "exported/exported.sfc",
            )
            self.assertTrue(verification.verify(source, exported)["passed"])
            output_bytes = exported.read_bytes()
            exported.write_bytes(output_bytes.replace(b"$$ATRU$$", b"-000-$$ATRU$$"))
            self.assertFalse(verification.verify(source, exported)["passed"])
            exported.write_bytes(output_bytes)
            document.update_image(raster, (25, 20), 40, 20, angle=15)
            document.save_bundle(root / "moved", file_name="moved.sfc")
            self.assertFalse(
                verification.verify(source, root / "moved/moved.sfc")["passed"]
            )
            output_image = root / "exported/renamed.tif"
            original = output_image.read_bytes()
            output_image.write_bytes(original + b"metadata")
            self.assertFalse(verification.verify(source, exported)["passed"])
            output_image.unlink()
            self.assertFalse(verification.verify(source, exported)["passed"])

    def test_empty_export_fails(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, exported = root / "source.sfc", root / "exported.sfc"
            ezsxf.new_sfc().save(source)
            exported.write_bytes(b"")
            self.assertFalse(verification.verify(source, exported)["passed"])


if __name__ == "__main__":
    unittest.main()
