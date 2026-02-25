from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

import ezsxf


MINIMAL_P21 = textwrap.dedent(
    """\
    ISO-10303-21;
    HEADER;
    FILE_DESCRIPTION(('SCADEC level2 AP202_mode'),'1');
    FILE_NAME('sample.p21','2007-06-29T07:56:58',('author'),('organization'),'translator$$3.1','system','');
    FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING'));
    ENDSEC;
    DATA;
    #1=CARTESIAN_POINT(' ',(0.0,1.0));
    ENDSEC;
    END-ISO-10303-21;
    """
)


MINIMAL_SFC = textwrap.dedent(
    """\
    ISO-10303-21;
    HEADER;
    FILE_DESCRIPTION(('SCADEC level2 AP202_mode'),'1');
    FILE_NAME('sample.sfc','2007-06-29T07:56:58',('author'),('organization'),'translator$$3.1','system','');
    FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING'));
    ENDSEC;
    DATA;
    /*SXF
    #1 = layer_feature('layer1','1')
    SXF*/
    /*SXF
    #99 = drawing_sheet_feature('sheet','9','1','100','300')
    SXF*/
    ENDSEC;
    END-ISO-10303-21;
    """
)


class PythonApiTest(unittest.TestCase):
    def test_version_string(self) -> None:
        self.assertTrue(isinstance(ezsxf.__version__, str))
        self.assertTrue(len(ezsxf.__version__) > 0)

    def test_hello_from_bin(self) -> None:
        self.assertEqual(ezsxf.hello_from_bin(), "Hello from ezsxf!")

    def test_parse_p21_text(self) -> None:
        out = ezsxf.parse_p21(MINIMAL_P21, strict=True)
        self.assertEqual(out["format"], "p21")
        self.assertEqual(len(out["entities"]), 1)
        self.assertEqual(len(out["typed_features"]), 0)

    def test_parse_sfc_text(self) -> None:
        out = ezsxf.parse_sfc(MINIMAL_SFC, strict=True)
        self.assertEqual(out["format"], "sfc")
        self.assertTrue(any(item["kind"] == "drawing_sheet" for item in out["typed_features"]))

    def test_cli_default_hello(self) -> None:
        result = subprocess.run(
            [sys.executable, "-m", "ezsxf"],
            capture_output=True,
            text=True,
            check=True,
        )
        self.assertEqual(result.stdout.strip(), "Hello from ezsxf!")

    def test_cli_parse_pretty_json(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "sample.p21"
            path.write_text(MINIMAL_P21, encoding="utf-8")
            result = subprocess.run(
                [sys.executable, "-m", "ezsxf", "parse", "p21", str(path), "--pretty"],
                capture_output=True,
                text=True,
                check=True,
            )
        parsed = json.loads(result.stdout)
        self.assertEqual(parsed["format"], "p21")
        self.assertEqual(len(parsed["entities"]), 1)

    def test_cli_parse_invalid_input_returns_nonzero(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "invalid.p21"
            path.write_text("not-a-valid-p21", encoding="utf-8")
            result = subprocess.run(
                [sys.executable, "-m", "ezsxf", "parse", "p21", str(path)],
                capture_output=True,
                text=True,
                check=False,
            )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("parse error:", result.stderr)


if __name__ == "__main__":
    unittest.main()
