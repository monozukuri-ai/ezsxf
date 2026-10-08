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
| Ellipse and ellipse arc | Elliptic geometry and parameter trims, also usable in composite boundaries |
| Point markers 1–7 | Predefined AP202 point marker symbols with position, rotation and scale |
| Cubic spline (`3n+1` controls) | Exact cubic Bezier segments, joined by a composite curve for multiple segments; open/closed state retained |
| Linear, arc-length, angular, radius and diameter dimensions | Semantic callouts, dimension/projection curves, numbered terminators and structured dimension text; omission flags are retained |
| Leader and balloon | Leader callout, curve/terminator, text and balloon circle |
| Solid fill, user hatch, predefined hatch and pattern hatch | Composite outer/hole boundaries, boundary visibility, fill colour, repeat factors or external hatch/tile identifiers |
| Drawing attributes | Drawing number/type/title/scale, contract/project, approval date and creator/owner organizations |
| ATRF, ATRU and ATRS | Named subfigures retain figure IDs, types, units and values; external SAF is delivered by the bundle APIs |
| TIFF/JPEG image | Standard image attribute on a clockwise placement rectangle; bytes and filename are preserved in the bundle/P2Z |
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

Clothoids and spline control layouts other than `3n+1` fail with an
unsupported-feature error. Predefined symbols retain their external identifiers and colour mode;
their graphics depend on the receiving CAD's symbol library. Geodetic
partial drawings (kind 2) also fail, because their exchanged local axes need a
separate implementation. Zero-length lines cannot form valid AP202 directions
and are rejected. Errors identify source entities. `new_sfc(target="p21")`
checks P21 constraints at insertion; `validate_p21()` lists issues in existing
documents. Standalone `to_p21_bytes(unsupported="drop", report=True)` explicitly
omits unsupported features and dependent content and returns `(bytes, dropped)`.
Default export and all bundle/P2Z APIs remain strict. See
[bulk authoring and preflight](bulk-writing.md) for the complete contract.
Composite fill boundaries must be connected, directed closed loops. Their exact
curves are retained; sampling is used only to choose a representative fill point
outside holes. Boundaries must also satisfy SXF's nonintersection rules;
the writer does not perform a complete geometric topology certification.

This is a bounded P21 writer, not an OCF certification or a guarantee of
electronic-delivery compliance. CAD acceptance, applicable delivery rules,
filenames, metadata and drawing conventions require their own verification.
Local graph round trips and comparison with ezsxf drawing primitives are
regression evidence. The separate native-CAD checks below cover specific inputs.

## Delivering P21, SAF and images

Standalone `to_p21_bytes`, `save_p21`, `serialize_p21` and `write_p21` reject SAF
and file/image dependencies. Use the complete delivery APIs for those drawings:

```python
import ezsxf

doc = ezsxf.new_sfc("drawing.sfc")
circle = doc.add_circle((80, 50), 5)
doc.set_attribute(circle, "材料", "鋼", group=["設計", "仕様"])
doc.add_image("scan.tif", (10, 20), 40, 20, angle=30)
report = doc.save_p21_bundle("delivery", file_name="drawing.p21")
doc.save_p2z("drawing.p2z")
data = doc.to_p2z_bytes(file_name="drawing.p21")

# Load an existing SFC together with its SAF and referenced files.
ezsxf.write_p21_bundle("original.sfc", "converted", file_name="result.p21")
ezsxf.write_p2z("original.sfc", "result.p2z")
```

Bundle destinations must be new directories. Preparation validates SAF figure
IDs, required dependencies, portable filenames and image metadata before
publishing. Renaming updates P21 `FILE_NAME`, ATRF references and SAF `sxfFile`
together. Attribute sets, grouped values and figure IDs remain intact.
Image bytes are copied without conversion; existing SFC image constraints
apply (single-page monochrome G4 TIFF or supported JPEG).

P2Z is a deterministic, unencrypted ZIP using Deflate, with one `.p21` file and
only its referenced SAF and TIFF/JPEG files at the archive root. `save_p2z` uses
the archive basename for its contained drawing and SAF. `to_p2z_bytes` takes a
P21 filename. Unreferenced registered dependencies are excluded. Arbitrary
attachments, additional drawing files and external DTD dependencies are rejected
for P2Z; use an uncompressed bundle for those. Existing archives are replaced
atomically after validation; symlinks are rejected. P2Z parsing/extraction is not
added to `parse_p21`; use a ZIP reader to access the contained P21/SAF/images.
Packaging follows [OCF SXF implementation convention §26](https://ocf.or.jp/pdf/kiyaku201604a.pdf).

Prefer P2Z for large deliveries when the receiver confirms direct P2Z support;
otherwise send the uncompressed P21 bundle. Jw_cad imports the `.p21` drawing;
direct P2Z opening has not been qualified. Identical Cartesian coordinates are
shared across the P21 graph, including line basis/start points. Compression
further reduces the graph size without changing its members.

```bash
python -m ezsxf bundle-p21 original.sfc converted --file-name result.p21
python -m ezsxf to-p2z original.sfc result.p2z
```

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

Additional native Windows checks on the same date covered the expanded writer.
Jw_cad 8.25a retained the tested linear, angular, radius and diameter dimensions,
leader and balloon through JWW save/reopen. Independent parsing matched their
geometry, text and terminator symbols to the corresponding SFC input. Jw_cad
omitted the arc-length dimension in both source formats; DynaCAD Viewer 9.0
displayed all five P21 dimension types and both leaders. Projection-curve bases
and terminator codes 5 (filled box) / 6 (filled arrow) follow the SXF Feature/SFC
specifications and native import behavior.

DynaCAD Viewer 9.0 also displayed the tested P21 solid fill and two-pattern
hatch with interior holes. Windows .NET ZIP extraction preserved every P2Z
member byte-for-byte, and the extracted P21 displayed the actual monochrome
TIFF pattern and coloured JPEG pixels. The drawing viewports matched the paired
SFC/P21 inputs pixel-for-pixel. The P21 SAF attribute inspector displayed the
same figure ID, attribute-set metadata, Japanese group/material/remark values
as the SFC control; their attribute-dialog RGB pixels matched. Input files,
SAF and rasters retained their recorded hashes. This qualifies the tested container and
extracted drawing; direct P2Z opening in that viewer is not qualified. Predefined
hatch/tile rendering remains dependent on the receiver's external libraries.

References: bundled SXF Ver.3.1 AP202 subset specification §3-1 (header),
§3-2-2 (sheet), §3-2-10 (point markers), §§3-2-11–16 (basic curves),
§3-2-17 (text), §3-2-18 (splines),
§§3-2-20–21 (subfigures), §§3-2-23–29 (dimensions/leaders),
§§3-2-30–34 (fills/boundaries), §3-2-1 (drawing attributes), the attribute
mechanism appendix §2-4 and common attribute sets §3-4 (images).
Text uses a rotated local baseline offset of
`height / 2`, `-height / 2` or `-height` for lower, middle or upper SXF box
anchors respectively; the reader reverses it. This mapping is verified against
paired real SFC/P21 data and the native SXF common-library import in Jw_cad.
The newer point-marker/spline output and shared Cartesian-point changes have
local regression coverage. They are outside the native verification recorded
above; paired controls are generated by `scripts/verify_writer_requests_v2.py`.
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
