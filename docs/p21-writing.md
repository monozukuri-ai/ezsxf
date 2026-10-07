# Writing SXF P21

```python
import ezsxf

doc = ezsxf.new_sfc("drawing.sfc", name="図面", paper="A1")
doc.extend([
    {"kind": "line", "start": (10, 10), "end": (100, 10)},
    {"kind": "text", "text": "日本語ABC", "anchor": (10, 30)},
])
data = doc.to_p21_bytes()
doc.save_p21("drawing.p21")

parsed_sfc = ezsxf.parse_sfc("existing.sfc", strict=True)
data = ezsxf.serialize_p21(parsed_sfc)
ezsxf.write_p21(parsed_sfc, "existing.p21")
```

`python -m ezsxf to-p21 input.sfc output.p21` uses the same implementation.
The source must be a complete validated SFC model. These APIs create a new
AP202 entity graph; they do not reserialize an arbitrary parsed P21 graph.
Source entity IDs resolve shared content but become new output graph IDs.
The source dictionary and editable document remain unchanged.

## Supported output

| Content | Representation |
| --- | --- |
| A0..A4/FREE sheet, orientation and title | Drawing definition/revision, sheet usage, planar box and presentation size |
| Line, circle, circular arc, polyline | AP202 geometry and annotation curve occurrences; clockwise and major arcs retain their sense |
| Text | Text literal with complete extent, all nine base points, rotation, slant, spacing, horizontal/vertical path and external font identifier |
| Layers and visibility | Presentation layer assignments/usages and invisibility |
| Predefined/RGB colours, predefined/custom line types and widths | Presentation styles, draw/gap pattern pairs and millimetre measures |
| Mathematical partial drawing (kind 1), group (kind 3), drawing part (kind 4) | Subfigure representations, shared symbol maps and explicit placements, including nested and unequal X/Y scale |

Input validation follows the SFC precision/CP932 contract. P21 uses ASCII with
standard STEP `X2` Unicode escapes for Japanese and special strings, CRLF,
millimetre/radian units and an `AP202_mode` header. Other header metadata
is retained; `FILE_NAME` receives a `.p21` extension and `FILE_DESCRIPTION`
identifies AP202 output. Each file is strictly reparsed to compare the emitted
graph and header before it is returned or saved. File saving uses the same
atomic regular-file replacement and symlink rejection as SFC saving.

Ellipses, ellipse arcs, splines, clothoids, point markers, dimensions, leaders,
hatches/fills, composite boundaries, external files, SAF/inline attributes and
images currently fail with an unsupported-feature/reference error. Geodetic
partial drawings (kind 2) also fail, because their exchanged local axes need a
separate implementation. Zero-length lines cannot form valid AP202 directions
and are rejected. No unsupported feature is dropped or approximated.
P2Z packaging is not implemented.

This is a bounded P21 writer, not an OCF certification or a guarantee of
electronic-delivery compliance. CAD acceptance, applicable delivery rules,
filenames, metadata and drawing conventions require their own verification.
Local graph round trips and comparison with ezsxf drawing primitives are
regression evidence. The separate native-CAD checks below cover specific inputs.

## Recorded native Windows verification

On 2026-10-07, Jw_cad 8.25a imported 12 deterministic SFC/P21 inputs, saved JWW,
closed, reopened and saved them again on native Windows 11 build 26200. The system
locale was ja-JP (ANSI 932); user culture was en-US and remained unchanged.
Independent JWW parsing verified the input/output hashes, retained geometry,
text and block definitions, and equality of the six corresponding SFC/P21 pairs.
The inputs cover the five basic types, clockwise/major arcs, all nine horizontal
text anchors at 30 degrees, shared-part reuse with two explicit placements,
and mathematical partial-drawing scales 1/1, 1/50 and 1/100.

| Partial-drawing scale | Saved JWW layer-group scale | Saved block X/Y ratios | Paper line / text height / text box width |
| --- | --- | --- | --- |
| 1/1 | 1 | 1 / 1 | 100 / 3.5 / 15.75 mm |
| 1/50 | 1 | 0.02 / 0.02 | 100 / 3.5 / 15.75 mm |
| 1/100 | 1 | 0.01 / 0.01 | 100 / 3.5 / 15.75 mm |

For these inputs, the partial-drawing ratio remains on the block placement,
while the JWW layer-group scale stays 1. Shared parts have one definition and
two placements, with no extra origin instance. The P21 baseline mapping above
retains the same native text positions as the corresponding SFC.
These observations apply to the tested version and inputs. Other CAD versions,
nonuniform/vertical text, custom-style rendering and native SFC re-export need
separate qualification; no OCF/electronic-delivery certification is claimed.

References: bundled SXF Ver.3.1 AP202 subset specification §3-1 (header),
§3-2-2 (sheet), §§3-2-11–15 (basic curves), §3-2-17 (text),
§§3-2-20–21 (subfigures). Text uses a rotated local baseline offset of
`height / 2`, `-height / 2` or `-height` for lower, middle or upper SXF box
anchors respectively; the reader reverses it. This mapping is verified against
paired real SFC/P21 data and the native SXF common-library import in Jw_cad.
SXF Feature/SFC arc direction is 0 counterclockwise, 1 clockwise; it maps to
STEP sense agreement `.T.` / `.F.` independently of sweep size.

## Reproducible performance and Windows scale review

After building the extension in release mode:

```bash
maturin develop --release
python scripts/verify_writer_requests.py --output /tmp/ezsxf-writer-review --counts 100 1000 10000
```

The destination must be new. The script verifies a bulk-versus-individual model
comparison, records separate addition/serialization times, and prepares paired
SFC/P21 inputs for scales 1/1, 1/50 and 1/100. Each scale input should display a
100 mm horizontal line and the same horizontal text with height 3.5 mm and box
width 15.75 mm at sheet coordinate `(20,40)`. It also creates basic-curve/all-anchor
and shared-part inputs. Model-space values grow with the
scale denominator; the partial-drawing placement divides them by it.
The manifest includes hashes and expected values. Timing ratios apply only to
that machine, build and workload; the script does not benchmark JWW.

On native Windows with Japanese system/user regional settings, copy the inputs
and use Jw_cad to open each SFC/P21. Record the program version, Windows version,
font, screenshot, displayed layer-group scale, measured line length, text size
and box width. Save JWW, reopen it and repeat the measurements. Determine from
the saved layer-group setting and coordinates whether Jw_cad stores the partial
drawing scale in that group or bakes it into the geometry. Either storage choice
must retain the same sheet result. Do not infer this from Wine rendering.

For the existing native save/reopen harness, the generated
`windows-sfc-cases.json` includes a basic control and the three scale cases:

```powershell
powershell -File scripts/verify_sfc_cad_windows.ps1 -Version 8.25a `
  -CaseManifest C:\review\windows-sfc-cases.json `
  -Output C:\review\native-results -RequireJapaneseLocale
```

That harness captures saved JWW/SFC files and screenshots; it does not inspect
JWW layer-group scales or certify P21 import. Complete those checks separately.
The recorded verification above used a dedicated import/JWW-save/reopen run,
without requiring native SFC re-export to succeed.
The generated report keeps native Windows and independent P21-CAD checks pending
until their evidence is recorded.
