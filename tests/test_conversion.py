from __future__ import annotations

import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

import ezsxf
from ezsxf._drawing import build_drawing
from ezsxf._drawing_geometry import clip_hatch_lines

try:
    import matplotlib
except ImportError:
    HAS_MATPLOTLIB = False
else:
    matplotlib.use("Agg")
    HAS_MATPLOTLIB = True


DRAWING_SFC = textwrap.dedent(
    """\
    ISO-10303-21;
    HEADER;
    FILE_DESCRIPTION(('SCADEC level2 feature_mode'),'2;1');
    FILE_NAME('drawing.sfc','2007-06-29T07:56:58',('author'),('organization'),'translator$$3.1','system','');
    FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING'));
    ENDSEC;
    DATA;
    /*SXF
    #1 = pre_defined_colour_feature('red')
    SXF*/
    /*SXF
    #2 = pre_defined_font_feature('continuous')
    SXF*/
    /*SXF
    #3 = width_feature('0.25')
    SXF*/
    /*SXF
    #4 = text_font_feature('Arial')
    SXF*/
    /*SXF
    #10 = line_feature('1','2','1','3','0','0','5','0')
    SXF*/
    /*SXF
    #20 = circle_feature('1','2','1','3','2','2','1')
    SXF*/
    /*SXF
    #30 = sfig_org_feature('group','1')
    SXF*/
    /*SXF
    #50 = polyline_feature('1','2','1','3','5','(0,4,4,0,0)','(0,0,3,3,0)')
    SXF*/
    /*SXF
    #60 = composite_curve_org_feature('2','1','3','1')
    SXF*/
    /*SXF
    #40 = sfig_locate_feature('1','group','10','20','90','2','1')
    SXF*/
    /*SXF
    #70 = fill_area_style_colour_feature('1','2','1','0','()')
    SXF*/
    /*SXF
    #80 = text_string_feature('1','2','1','DXF','1','2','2','6','0','0','0','1','1')
    SXF*/
    /*SXF
    #99 = drawing_sheet_feature('sheet','9','1','100','300')
    SXF*/
    /*SXF
    #100 = layer_feature('VISIBLE','1')
    SXF*/
    ENDSEC;
    END-ISO-10303-21;
    """
)

DRAWING_P21 = textwrap.dedent(
    """\
    ISO-10303-21;
    HEADER;
    FILE_DESCRIPTION(('SCADEC level2 AP202_mode'),'2;1');
    FILE_NAME('drawing.p21','2007-06-29T07:56:58',('author'),('organization'),'translator$$3.1','system','');
    FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING'));
    ENDSEC;
    DATA;
    #1=DRAUGHTING_PRE_DEFINED_COLOUR('red');
    #2=DRAUGHTING_PRE_DEFINED_CURVE_FONT('continuous');
    #3=(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.));
    #4=LENGTH_MEASURE_WITH_UNIT(POSITIVE_LENGTH_MEASURE(0.25),#3);
    #10=CARTESIAN_POINT(' ',(0.0,0.0));
    #11=CARTESIAN_POINT(' ',(5.0,0.0));
    #12=DIRECTION(' ',(1.0,0.0));
    #13=VECTOR(' ',#12,5.0);
    #14=LINE(' ',#10,#13);
    #15=TRIMMED_CURVE(' ',#14,(#10),(#11),.T.,.CARTESIAN.);
    #16=CURVE_STYLE(' ',#2,#4,#1);
    #17=PRESENTATION_STYLE_ASSIGNMENT((#16));
    #18=(ANNOTATION_CURVE_OCCURRENCE() ANNOTATION_OCCURRENCE()
        DRAUGHTING_ANNOTATION_OCCURRENCE() GEOMETRIC_REPRESENTATION_ITEM()
        REPRESENTATION_ITEM(' ') STYLED_ITEM((#17),#15));
    #20=EXTERNAL_SOURCE(IDENTIFIER('test'));
    #21=EXTERNALLY_DEFINED_TEXT_FONT(IDENTIFIER('Arial'),#20);
    #22=CARTESIAN_POINT(' ',(1.0,2.5));
    #23=AXIS2_PLACEMENT_2D(' ',#22,#12);
    #24=PLANAR_EXTENT(' ',2.0,1.0);
    #25=TEXT_LITERAL_WITH_EXTENT('$$SXF_baseline left','P21',#23,
        'baseline left',.RIGHT.,#21,#24);
    #26=PRESENTATION_STYLE_ASSIGNMENT((#16));
    #27=(ANNOTATION_OCCURRENCE() ANNOTATION_TEXT_OCCURRENCE()
        DRAUGHTING_ANNOTATION_OCCURRENCE() GEOMETRIC_REPRESENTATION_ITEM()
        REPRESENTATION_ITEM(' ') STYLED_ITEM((#26),#25));
    #30=REPRESENTATION_CONTEXT('ID','2D');
    #31=DRAWING_SHEET_REVISION('sheet',(#18,#27),#30);
    #32=PRESENTATION_LAYER_ASSIGNMENT('P21_LAYER',' ',(#18,#27));
    ENDSEC;
    END-ISO-10303-21;
    """
)


class DrawingConversionTest(unittest.TestCase):
    def test_build_drawing_flattens_compound_figure_transform(self) -> None:
        drawing = build_drawing(DRAWING_SFC, curve_segments=16)

        line = next(path for path in drawing.paths if path.source_id == 10)
        self.assertAlmostEqual(line.points[0][0], 10.0)
        self.assertAlmostEqual(line.points[0][1], 20.0)
        self.assertAlmostEqual(line.points[1][0], 10.0)
        self.assertAlmostEqual(line.points[1][1], 30.0)
        self.assertEqual(line.style.layer, "VISIBLE")
        self.assertEqual(line.style.color, (255, 0, 0))
        self.assertAlmostEqual(line.style.line_width_mm, 0.25)

        circle = next(path for path in drawing.paths if path.source_id == 20)
        self.assertTrue(circle.closed)
        self.assertEqual(len(circle.points), 16)
        self.assertEqual(len(drawing.fills), 1)
        self.assertEqual(drawing.texts[0].text, "DXF")

    def test_to_dxf_returns_text_and_writes_output(self) -> None:
        parsed = ezsxf.parse_sfc(DRAWING_SFC, strict=True)
        with tempfile.TemporaryDirectory() as tmpdir:
            output = Path(tmpdir) / "drawing.dxf"
            dxf = ezsxf.to_dxf(parsed, output, curve_segments=16)
            self.assertEqual(output.read_bytes().decode("utf-8"), dxf)

        self.assertIn("\r\n$ACADVER\r\n1\r\nAC1021\r\n", dxf)
        self.assertIn("\r\nLINE\r\n", dxf)
        self.assertIn("\r\nLWPOLYLINE\r\n", dxf)
        self.assertIn("\r\nHATCH\r\n", dxf)
        self.assertIn("\r\nTEXT\r\n", dxf)
        self.assertIn("\r\nVISIBLE\r\n", dxf)
        self.assertTrue(dxf.endswith("0\r\nEOF\r\n"))

    def test_build_drawing_and_dxf_from_p21(self) -> None:
        parsed = ezsxf.parse_p21(DRAWING_P21, strict=True)
        drawing = build_drawing(parsed, curve_segments=16)

        self.assertEqual(len(drawing.paths), 1)
        self.assertEqual(drawing.paths[0].points, ((0.0, 0.0), (5.0, 0.0)))
        self.assertEqual(drawing.paths[0].style.layer, "P21_LAYER")
        self.assertEqual(drawing.paths[0].style.color, (255, 0, 0))
        self.assertEqual(len(drawing.texts), 1)
        self.assertEqual(drawing.texts[0].text, "P21")
        self.assertEqual(drawing.texts[0].anchor, (1.0, 2.0))

        dxf = ezsxf.to_dxf(DRAWING_P21, curve_segments=16)
        self.assertIn("\r\nP21_LAYER\r\n", dxf)
        self.assertIn("\r\nP21\r\n", dxf)

    def test_hidden_sfc_layer_is_retained_as_hidden_dxf_layer(self) -> None:
        hidden_sfc = DRAWING_SFC.replace(
            "layer_feature('VISIBLE','1')", "layer_feature('HIDDEN','0')"
        )
        drawing = build_drawing(hidden_sfc, curve_segments=16)
        self.assertTrue(drawing.paths)
        self.assertTrue(all(not path.style.visible for path in drawing.paths))

        dxf = ezsxf.to_dxf(hidden_sfc, curve_segments=16)
        self.assertIn("\r\nHIDDEN\r\n", dxf)
        self.assertIn("\r\n62\r\n-7\r\n", dxf)

    def test_hatch_lines_are_clipped_around_holes(self) -> None:
        segments = clip_hatch_lines(
            [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)],
            [[(4.0, 4.0), (6.0, 4.0), (6.0, 6.0), (4.0, 6.0)]],
            (0.0, 0.0),
            1.0,
            0.0,
        )
        middle = [segment for segment in segments if segment[0][1] == 5.0]
        self.assertEqual(middle, [((0.0, 5.0), (4.0, 5.0)), ((6.0, 5.0), (10.0, 5.0))])

    def test_cli_to_dxf(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            input_path = Path(tmpdir) / "drawing.sfc"
            output_path = Path(tmpdir) / "drawing.dxf"
            input_path.write_text(DRAWING_SFC, encoding="utf-8")
            subprocess.run(
                [
                    sys.executable,
                    "-m",
                    "ezsxf",
                    "to-dxf",
                    str(input_path),
                    str(output_path),
                    "--curve-segments",
                    "16",
                ],
                capture_output=True,
                text=True,
                check=True,
            )
            self.assertGreater(output_path.stat().st_size, 500)
            self.assertIn("SECTION", output_path.read_text(encoding="utf-8"))

    @unittest.skipUnless(
        HAS_MATPLOTLIB,
        "matplotlib is an optional dependency",
    )
    def test_matplotlib_plot_and_cli_image(self) -> None:
        axes = ezsxf.plot(DRAWING_SFC, curve_segments=16, monochrome=True)
        self.assertGreater(len(axes.collections), 0)
        self.assertGreater(len(axes.patches), 0)

        p21_axes = ezsxf.plot(DRAWING_P21, curve_segments=16)
        self.assertGreater(len(p21_axes.collections), 0)
        self.assertGreater(len(p21_axes.patches), 0)

        with tempfile.TemporaryDirectory() as tmpdir:
            input_path = Path(tmpdir) / "drawing.sfc"
            output_path = Path(tmpdir) / "drawing.png"
            input_path.write_text(DRAWING_SFC, encoding="utf-8")
            subprocess.run(
                [
                    sys.executable,
                    "-m",
                    "ezsxf",
                    "plot",
                    str(input_path),
                    str(output_path),
                    "--curve-segments",
                    "16",
                ],
                capture_output=True,
                text=True,
                check=True,
            )
            self.assertGreater(output_path.stat().st_size, 500)


if __name__ == "__main__":
    unittest.main()
