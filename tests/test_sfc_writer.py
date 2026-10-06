from __future__ import annotations

import copy
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import ezsxf

FIXTURE = Path(__file__).parent / "fixtures" / "writer_all_features.sfc"


def standalone_result() -> dict:
    # The same structure with an ordinary group in place of the external SAF
    # attachment exercises the default drawing-only save path.
    source = FIXTURE.read_text(encoding="utf-8").replace("$$ATRF$$102", "ordinary")
    return ezsxf.parse_sfc(source)


class SfcWriterTest(unittest.TestCase):
    def test_literal_backslashes_are_explicit_and_transactional(self) -> None:
        doc = ezsxf.new_sfc()
        text_id = doc.add_text(r"C:\temp\new.sfc 日本語 ソ 表 + quote' ) ,", (20, 20))
        parsed = doc.to_dict()
        encoded = doc.to_bytes(literal_backslashes=True)
        self.assertEqual(ezsxf.parse_sfc(encoded), parsed)
        self.assertIn(b"C:\\temp\\new.sfc", encoded)
        self.assertIn(b"C:\\\\temp\\\\new.sfc", doc.to_bytes())
        self.assertEqual(ezsxf.serialize_sfc(parsed, literal_backslashes=True), encoded)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / "literal.sfc"
            doc.save(output, literal_backslashes=True)
            self.assertEqual(output.read_bytes(), encoded)
            ezsxf.write_sfc(parsed, output, literal_backslashes=True)
            self.assertEqual(output.read_bytes(), encoded)
            doc.set_attribute(text_id, "note", "keep \\ here")
            before_bundle = doc.to_dict()
            doc.save_bundle(root / "standard-bundle")
            doc.save_bundle(root / "bundle", literal_backslashes=True)
            restored = ezsxf.edit_sfc_bundle(root / "bundle/drawing.sfc")
            standard = ezsxf.edit_sfc_bundle(root / "standard-bundle/drawing.sfc")
            self.assertEqual(restored.to_dict(), standard.to_dict())
            self.assertEqual(doc.to_dict(), before_bundle)
            self.assertEqual(restored.get_attributes(text_id), doc.get_attributes(text_id))
            for unsafe in (r"\\server\share", "end\\", "slash\\'quote"):
                doc.update_element(text_id, text=unsafe)
                before = doc.to_dict()
                with self.subTest(text=unsafe):
                    with self.assertRaisesRegex(ValueError, "literal_backslashes"):
                        doc.save(output, allow_external_references=True, literal_backslashes=True)
                    with self.assertRaisesRegex(OSError, "literal_backslashes"):
                        doc.save_bundle(root / "rejected", literal_backslashes=True)
                    self.assertEqual(output.read_bytes(), encoded)
                    self.assertFalse((root / "rejected").exists())
                    self.assertEqual(doc.to_dict(), before)
                    doc.to_bytes(allow_external_references=True)

    def test_all_features_and_structures_round_trip_through_json(self) -> None:
        parsed = ezsxf.parse_sfc(str(FIXTURE))
        self.assertEqual(
            len({feature["kind"] for feature in parsed["typed_features"]}), 34
        )
        parsed = json.loads(json.dumps(parsed, ensure_ascii=False))
        encoded = ezsxf.serialize_sfc(parsed, allow_external_references=True)
        self.assertIsInstance(encoded, bytes)
        self.assertIn("日本語".encode("cp932"), encoded)
        self.assertIn(b"/*SXF3.1\r\n", encoded)
        self.assertIn(b"/*SXF3\r\n", encoded)
        self.assertEqual(ezsxf.parse_sfc(encoded), parsed)
        self.assertEqual(
            ezsxf.serialize_sfc(parsed, allow_external_references=True), encoded
        )

    def test_write_accepts_pathlike_and_preserves_header(self) -> None:
        parsed = standalone_result()
        original = copy.deepcopy(parsed)
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "copy.sfc"
            ezsxf.write_sfc(parsed, target)
            self.assertEqual(ezsxf.parse_sfc(str(target)), original)
            self.assertEqual(target.read_bytes(), ezsxf.serialize_sfc(parsed))
            self.assertEqual(list(Path(directory).iterdir()), [target])
        self.assertEqual(parsed, original)

    def test_in_place_resave(self) -> None:
        source = FIXTURE.read_text(encoding="utf-8").replace("$$ATRF$$102", "ordinary")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "source.sfc"
            target.write_text(source, encoding="utf-8")
            parsed = ezsxf.parse_sfc(str(target))
            ezsxf.write_sfc(parsed, target)
            self.assertEqual(ezsxf.parse_sfc(str(target)), parsed)
            self.assertEqual(list(Path(directory).iterdir()), [target])

    def test_external_references_require_opt_in_and_are_not_copied(self) -> None:
        parsed = ezsxf.parse_sfc(str(FIXTURE))
        with self.assertRaisesRegex(ValueError, "external SAF"):
            ezsxf.serialize_sfc(parsed)
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "writer-fixture.sfc"
            with self.assertRaisesRegex(ValueError, "external SAF"):
                ezsxf.write_sfc(parsed, target)
            self.assertFalse(target.exists())
            ezsxf.write_sfc(parsed, target, allow_external_references=True)
            self.assertEqual(ezsxf.parse_sfc(str(target)), parsed)
            self.assertEqual(list(Path(directory).iterdir()), [target])

    def test_rejects_changes_to_derived_views_and_header_aliases(self) -> None:
        for field in ("typed_features", "model", "header"):
            parsed = standalone_result()
            if field == "typed_features":
                parsed[field][0]["name"] = "edited only in the typed view"
            elif field == "model":
                parsed[field]["sheet"]["component_ids"].reverse()
            else:
                parsed[field]["file_name"]["parameters"][0] = "edited.sfc"
            with (
                self.subTest(field=field),
                self.assertRaisesRegex(ValueError, "disagrees"),
            ):
                ezsxf.serialize_sfc(parsed)

    def test_failed_validation_keeps_existing_file_and_leaves_no_temporary_file(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "keep.sfc"
            target.write_bytes(b"existing file must survive")
            for failure in ("warnings", "entities", "model"):
                parsed = standalone_result()
                if failure == "warnings":
                    parsed["warnings"].append(
                        {"code": "salvage", "message": "incomplete"}
                    )
                elif failure == "entities":
                    parsed["entities"][0]["record"]["keyword"] = "unsupported_feature"
                else:
                    parsed["model"]["sheet"]["component_ids"] = []
                with self.subTest(failure=failure), self.assertRaises(ValueError):
                    ezsxf.write_sfc(parsed, target)
                self.assertEqual(target.read_bytes(), b"existing file must survive")
                self.assertEqual(list(Path(directory).iterdir()), [target])

    def test_malformed_input_is_rejected_before_creating_output(self) -> None:
        mutations = [
            lambda parsed: parsed.update(format="p21"),
            lambda parsed: parsed.pop("warnings"),
            lambda parsed: parsed["entities"][0].update(id=True),
            lambda parsed: parsed["entities"][0]["record"]["parameters"].__setitem__(
                0, None
            ),
            lambda parsed: parsed["entities"][0]["record"]["parameters"].__setitem__(
                0, "😀"
            ),
            lambda parsed: parsed["entities"][0].update(body_type="complex"),
            lambda parsed: parsed["entities"][0].update(sfc_version="4"),
            lambda parsed: parsed["header"]["entities"][0].update(keyword="BAD);DATA;"),
        ]
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "must-not-exist.sfc"
            for mutate in mutations:
                parsed = standalone_result()
                mutate(parsed)
                with (
                    self.subTest(mutation=mutate),
                    self.assertRaises((ValueError, TypeError)),
                ):
                    ezsxf.write_sfc(parsed, target)
                self.assertFalse(target.exists())

    def test_io_failure_keeps_directory_and_cleans_temporary_file(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / "target.sfc"
            target.mkdir()
            with self.assertRaises(OSError):
                ezsxf.write_sfc(standalone_result(), target)
            self.assertTrue(target.is_dir())
            self.assertEqual(list(root.iterdir()), [target])
            with self.assertRaises(OSError):
                ezsxf.write_sfc(standalone_result(), root / "missing" / "target.sfc")
            self.assertEqual(list(root.iterdir()), [target])

    @unittest.skipUnless(os.name == "posix", "POSIX permission/symlink behavior")
    def test_replacement_preserves_permissions_and_does_not_follow_symlink(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "private.sfc"
            target.write_bytes(b"previous")
            target.chmod(0o600)
            ezsxf.write_sfc(standalone_result(), target)
            self.assertEqual(target.stat().st_mode & 0o777, 0o600)
            link = Path(directory) / "link.sfc"
            link.symlink_to(target)
            original = target.read_bytes()
            with self.assertRaises(OSError):
                ezsxf.write_sfc(standalone_result(), link)
            self.assertTrue(link.is_symlink())
            self.assertEqual(target.read_bytes(), original)
            self.assertEqual(len(list(Path(directory).iterdir())), 2)

    def test_cli_resave_and_failure(self) -> None:
        source = FIXTURE.read_text(encoding="utf-8").replace("$$ATRF$$102", "ordinary")
        with tempfile.TemporaryDirectory() as directory:
            input_path = Path(directory) / "source.sfc"
            output_path = Path(directory) / "copy.sfc"
            input_path.write_text(source, encoding="utf-8")
            result = subprocess.run(
                [
                    sys.executable,
                    "-m",
                    "ezsxf",
                    "resave-sfc",
                    str(input_path),
                    str(output_path),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(ezsxf.parse_sfc(str(output_path)), ezsxf.parse_sfc(source))
            previous = output_path.read_bytes()
            result = subprocess.run(
                [
                    sys.executable,
                    "-m",
                    "ezsxf",
                    "resave-sfc",
                    str(FIXTURE),
                    str(output_path),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("SFC save error:", result.stderr)
            self.assertEqual(output_path.read_bytes(), previous)
            result = subprocess.run(
                [
                    sys.executable,
                    "-m",
                    "ezsxf",
                    "resave-sfc",
                    str(FIXTURE),
                    str(output_path),
                    "--allow-external-references",
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(
                ezsxf.parse_sfc(str(output_path)), ezsxf.parse_sfc(str(FIXTURE))
            )


if __name__ == "__main__":
    unittest.main()
