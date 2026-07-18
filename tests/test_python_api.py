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
    FILE_DESCRIPTION(('SCADEC level2 feature_mode'),'2;1');
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


ATTRIBUTE_SFC = MINIMAL_SFC.replace(
    "/*SXF\n#99 = drawing_sheet_feature",
    """/*SXF
#2 = line_feature('1','1','1','1','0.000000','0.000000','10.000000','10.000000')
SXF*/
/*SXF
#3 = sfig_org_feature('$$ATRU$$42$$等高線$$等高線$$12.5$$LEN$$m','3')
SXF*/
/*SXF
#4 = sfig_locate_feature('0','$$ATRU$$42$$等高線$$等高線$$12.5$$LEN$$m','0.000000','0.000000','0.00000000000000','1.00000000000000','1.00000000000000')
SXF*/
/*SXF
#99 = drawing_sheet_feature""",
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
        self.assertEqual(out["model"]["sheet"]["entity_id"], 99)
        self.assertEqual(out["model"]["code_tables"]["layers"][0]["code"], 1)

    def test_parse_sfc_shift_jis_bytes(self) -> None:
        source = MINIMAL_SFC.replace("layer1", "日本語レイヤ").replace(
            "'sheet'", "'図面'"
        )
        out = ezsxf.parse_sfc(source.encode("cp932"), strict=True)
        layers = [item for item in out["typed_features"] if item["kind"] == "layer"]
        self.assertEqual(layers[0]["name"], "日本語レイヤ")
        self.assertEqual(out["warnings"], [])

    def test_parse_sfc_attribute_attachment(self) -> None:
        out = ezsxf.parse_sfc(ATTRIBUTE_SFC, strict=True)
        model = out["model"]
        self.assertEqual(model["sfig_definitions"], [])
        self.assertEqual(model["sfig_references"], [])
        attachment = model["attribute_attachments"][0]
        self.assertEqual(attachment["definition_id"], 3)
        self.assertEqual(attachment["component_ids"], [2])
        self.assertEqual(attachment["placement_ids"], [4])
        self.assertIsNone(attachment["resolved_attribute_file_name"])
        self.assertEqual(attachment["attribute"]["mechanism"], "ATRU")
        self.assertEqual(attachment["attribute"]["figure_id"], "42")
        self.assertEqual(attachment["attribute"]["attribute_type"], "LEN")

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
