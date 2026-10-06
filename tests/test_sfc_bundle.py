from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import ezsxf

FIXTURE = Path(__file__).parent / "fixtures" / "writer_all_features.sfc"
SAF = """<?xml version="1.0" encoding="UTF-8"?>
<SxfAttributeXML version="3.1" date="2026-10-05" application="ezsxf-test" sxfFile="writer-fixture.sfc">
<Figure id="102"><AttributeSet name="sample" version="1" designedBy="test">
<Attr name="画像">image.bmp</Attr><Attr name="note">opaque &amp; preserved</Attr>
</AttributeSet></Figure></SxfAttributeXML>"""


def setup_source(root: Path, saf: str = SAF) -> Path:
    source = root / "writer-fixture.sfc"
    source.write_bytes(FIXTURE.read_bytes())
    (root / "writer-fixture.SAF").write_text(saf, encoding="utf-8")
    (root / "image.bmp").write_bytes(b"opaque dependency test payload")
    return source


class SfcBundleTest(unittest.TestCase):
    def test_rename_both_files_and_references_without_changing_geometry(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = setup_source(root)
            original = ezsxf.parse_sfc(str(source))
            source_bytes = source.read_bytes()
            report = ezsxf.write_sfc_bundle(
                source, root / "bundle", file_name="日本語.sfc"
            )
            self.assertEqual(
                report,
                {
                    "drawing": "日本語.sfc",
                    "files": ["image.bmp", "日本語.SAF", "日本語.sfc"],
                },
            )
            result = ezsxf.parse_sfc(str(root / "bundle" / report["drawing"]))
            self.assertEqual(
                [e["id"] for e in result["entities"]],
                [e["id"] for e in original["entities"]],
            )
            self.assertEqual(result["model"]["sheet"], original["model"]["sheet"])
            attachment = next(
                a
                for a in result["model"]["attribute_attachments"]
                if a["attribute"]["mechanism"] == "ATRF"
            )
            self.assertEqual(attachment["resolved_attribute_file_name"], "日本語.SAF")
            self.assertIn(
                'sxfFile="日本語.sfc"',
                (root / "bundle/日本語.SAF").read_text(encoding="utf-8"),
            )
            self.assertIn(
                "opaque &amp; preserved",
                (root / "bundle/日本語.SAF").read_text(encoding="utf-8"),
            )
            self.assertEqual(
                (root / "bundle/image.bmp").read_bytes(),
                (root / "image.bmp").read_bytes(),
            )
            self.assertEqual(source.read_bytes(), source_bytes)

    def test_missing_or_invalid_dependencies_publish_nothing(self) -> None:
        mutations = [
            SAF.replace('id="102"', 'id="103"'),
            SAF.replace('id="102"', 'id=""'),
            SAF.replace("writer-fixture.sfc", "wrong.sfc"),
            SAF.replace("image.bmp", "missing.bmp"),
            SAF.replace("image.bmp", "../image.bmp"),
            SAF.replace("image.bmp", "https://example.invalid/image.bmp"),
            SAF.replace("</SxfAttributeXML>", '<Figure id="102"/></SxfAttributeXML>'),
            SAF.replace('version="3.1"', 'version="4"'),
            "<invalid",
        ]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for saf in mutations:
                source = setup_source(root, saf)
                with self.subTest(saf=saf), self.assertRaises(ValueError):
                    ezsxf.write_sfc_bundle(source, root / "output")
                self.assertFalse((root / "output").exists())
                self.assertEqual(len(list(root.iterdir())), 3)

    def test_default_save_and_extras_keep_names_and_structure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = setup_source(root)
            (root / "extra.bin").write_bytes(b"custom attribute attachment")
            report = ezsxf.write_sfc_bundle(
                source, root / "output", extra_files=["extra.bin"]
            )
            self.assertIn("extra.bin", report["files"])
            self.assertEqual(
                ezsxf.parse_sfc(str(root / "output/writer-fixture.sfc")),
                ezsxf.parse_sfc(str(source)),
            )
            self.assertEqual(
                (root / "output/writer-fixture.SAF").read_bytes(),
                (root / "writer-fixture.SAF").read_bytes(),
            )
            previous = {p.name: p.read_bytes() for p in (root / "output").iterdir()}
            with self.assertRaises(FileExistsError):
                ezsxf.write_sfc_bundle(source, root / "output")
            self.assertEqual(
                {p.name: p.read_bytes() for p in (root / "output").iterdir()}, previous
            )

    def test_windows_filenames_and_destination_collisions_are_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = setup_source(root)
            for filename in (
                "../outside.sfc",
                "CON.sfc",
                "a:sfc",
                "foo.sfc.",
                "wrong.p21",
                "image.bmp.sfc ",
            ):
                with self.subTest(filename=filename), self.assertRaises(ValueError):
                    ezsxf.write_sfc_bundle(source, root / "output", file_name=filename)
                self.assertFalse((root / "output").exists())
            (root / "new.sfc").write_bytes(b"dependency")
            with self.assertRaises(ValueError):
                ezsxf.write_sfc_bundle(
                    source,
                    root / "output",
                    file_name="new.sfc",
                    extra_files=["new.sfc"],
                )
            self.assertFalse((root / "output").exists())
            (root / "empty").mkdir()
            with self.assertRaises(FileExistsError):
                ezsxf.write_sfc_bundle(source, root / "empty")
            self.assertEqual(list((root / "empty").iterdir()), [])

    def test_cp932_saf_and_utf8_bom(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = setup_source(root)
            for encoded, encoding in (
                (SAF.replace("UTF-8", "Shift_JIS").encode("cp932"), "cp932"),
                (b"\xef\xbb\xbf" + SAF.encode("utf-8"), "utf-8-sig"),
            ):
                (root / "writer-fixture.SAF").write_bytes(encoded)
                destination = root / encoding
                ezsxf.write_sfc_bundle(source, destination, file_name="改名.sfc")
                self.assertIn(
                    'sxfFile="改名.sfc"',
                    (destination / "改名.SAF").read_text(encoding=encoding),
                )

    def test_ambiguous_case_fails_before_publication(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = setup_source(root)
            (root / "IMAGE.BMP").write_bytes(b"ambiguous")
            if (root / "IMAGE.BMP").samefile(root / "image.bmp"):
                self.skipTest("Requires a case-sensitive source filesystem")
            with self.assertRaisesRegex(ValueError, "ambiguous"):
                ezsxf.write_sfc_bundle(source, root / "output")
            self.assertFalse((root / "output").exists())

    @unittest.skipUnless(os.name == "posix", "Symlink creation requires privileges on Windows")
    def test_symlinks_fail_before_publication(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = setup_source(root)
            (root / "real.bmp").write_bytes(b"target")
            (root / "image.bmp").unlink()
            (root / "image.bmp").symlink_to(root / "real.bmp")
            with self.assertRaisesRegex(ValueError, "regular file"):
                ezsxf.write_sfc_bundle(source, root / "output")
            self.assertFalse((root / "output").exists())

    def test_cli_bundle_and_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = setup_source(root)
            result = subprocess.run(
                [
                    sys.executable,
                    "-m",
                    "ezsxf",
                    "bundle-sfc",
                    str(source),
                    str(root / "output"),
                    "--file-name",
                    "cli.sfc",
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)["drawing"], "cli.sfc")
            result = subprocess.run(
                [
                    sys.executable,
                    "-m",
                    "ezsxf",
                    "bundle-sfc",
                    str(source),
                    str(root / "output"),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("SFC bundle error", result.stderr)


if __name__ == "__main__":
    unittest.main()
