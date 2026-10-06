# ezsxf

A fast SXF parser and drawing converter for Python, implemented in Rust with
PyO3.

`ezsxf` supports both SXF formats:
- `P21` (ISO-10303-21 based)
- `SFC` (SXF feature blocks)

## Status

The SFC parser covers all 34 feature types in the SXF Ver.3.1 SFC specification,
including resolved drawing/compound-figure/composite-curve structure. The P21
reader currently exposes the generic Part 21 entity representation.

## Features

- Parse `P21` and `SFC` from file path, text, or bytes
- Resave parsed SFC drawings as validated Shift-JIS/CP932 bytes or files
- Save an SFC/SAF/image bundle with consistent names and dependency validation
- Create and edit basic SFC lines, circles, arcs, polylines, text and layers
- Decode SFC files as UTF-8 or Windows Shift-JIS/CP932 without lossy replacement
- Strict and lenient parsing modes
- Structured parse output (`header`, `entities`, `typed_features`, `model`, `warnings`)
- Resolve layer/style code tables, compound-figure placements, and hatch boundaries
- Decode `ATRF`/`ATRU`/`ATRS` attribute attachments separately from drawing groups
- Validate compound-figure placement counts, hierarchy, and drawing-group transforms
- Convert SFC drawings to AutoCAD 2007 ASCII DXF without another runtime dependency
- Draw SFC drawings with the optional `matplotlib` backend
- Python API + CLI (`ezsxf` / `python -m ezsxf`)

## Installation

### From source (recommended currently)

```bash
python -m venv .venv
source .venv/bin/activate
pip install maturin
maturin develop
```

Install the plotting extra when using the matplotlib backend:

```bash
pip install ".[plot]"
```

## Quick Start

```python
import ezsxf

result = ezsxf.parse_sfc("./data/D0LS004ZSFC/D0LS004Z.SFC", strict=True)
print(result["format"])          # "sfc"
print(len(result["typed_features"]))
print(result["model"]["sheet"]["component_ids"])
```

## Resaving SFC

```python
import ezsxf

parsed = ezsxf.parse_sfc("drawing.sfc", strict=True)
sfc_bytes = ezsxf.serialize_sfc(parsed)
ezsxf.write_sfc(parsed, "copy.sfc")
```

This API resaves an existing complete SFC parse result. It preserves entity IDs,
record order, parameter values, the typed features, hierarchy, style codes and
attribute attachments. Output uses Shift-JIS/CP932 and CRLF line endings.
Comments, whitespace, original encoding and quote spelling are not preserved.
The header is retained, including `FILE_NAME` and the original timestamp, even
when writing to a different path. New drawing construction and P21 output are
not supported by these functions.

`entities` and `header.entities` are the serialization source. Before returning
bytes or touching the destination, the writer checks field syntax and precision,
reparses the output without warnings and verifies the derived `typed_features`,
`model` and header aliases. It rejects inconsistent dictionary edits, skipped
features, invalid references/order, unencodable or changed characters, and
excess precision (lengths beyond six fractional digits or angles/scales beyond
15 digits excluding the sign and decimal point). It does not round values or
silently repair input.
`write_sfc` writes a temporary file beside the destination and replaces the
destination after validation and a successful write. Existing regular-file
permissions are retained; symlink destinations are rejected.

Drawings with external SAF references require explicit opt-in:

```python
ezsxf.write_sfc(parsed, "copy.sfc", allow_external_references=True)
```

This preserves the references only: it does not read, validate, rename or copy
SAF or image files. Arrange the referenced files separately, retaining their
names and directory relationships. In particular, when the SAF name is omitted,
keep the drawing basename consistent with the retained `FILE_NAME` and SAF
basename. Use the default to reject such drawings until those dependencies are
handled. Inline ATRU/ATRS attachments do not require this opt-in.

The writer follows SXF Ver.3.1 SFC specification §§1-1-1 and 1-1-2 (body
pp. 2–6) for representation and §§1-3 (body pp. 99–103) for ordering and
relationships. The checked-in small fixture covers all 34 feature types,
nested figures, hatch holes and ATRF/ATRU/ATRS. Local round-trip tests also use
available real samples under `data/`. These checks establish parser/model
round trips; external CAD interoperability and OCF certification are separate
validation steps.

## Saving SFC/SAF bundles

```python
report = ezsxf.write_sfc_bundle(
    "source/drawing.sfc", "delivery", file_name="renamed.sfc",
    extra_files=["custom-attachment.bin"],  # optional vendor-specific dependencies
)
print(report["files"])
```

The destination must be a **new directory**. The operation validates the drawing,
SAF root metadata and matching ATRF/Figure IDs, gathers local image filenames
from SAF `Attr` names `画像`/`ファイル名` and inline ATRU attachments, then copies
dependencies unchanged. It also copies a declared local SYSTEM DTD. Missing,
ambiguous, unsafe or symlink dependencies fail before publishing the directory.
Images and copied DTDs are opaque bytes; this is not image decoding or full SAF
DTD/schema validation. Unknown vendor attachment references must be supplied
through `extra_files`.

An explicit `file_name` updates the header, ATRF definitions/placements and SAF
`sxfFile`, and renames the SAF to the new drawing stem. Other XML spelling and
opaque attributes are retained; UTF-8 (including BOM) and Shift-JIS/CP932 SAFs
are supported. Without a rename, drawing/SAF filenames and records are retained
(the header filename is aligned with the physical source basename). Dependencies
must be regular files in the source directory; path references, multiple SAF
files, PUBLIC/internal DTD declarations and existing destinations are rejected.
Validated files are staged next to the destination and published together using
an exclusive directory rename on Linux, macOS or Windows. This prevents partial
deliveries after ordinary errors; it does not promise power-loss durability.

## Creating and editing basic SFC elements

```python
doc = ezsxf.new_sfc("created.sfc", name="Drawing", width_mm=297, height_mm=210)
layer = doc.add_layer("Structure")
line_id = doc.add_line((0, 0), (100, 50), layer=layer)
doc.add_circle((50, 50), 10)
doc.add_arc((80, 50), 10, 0, 90)  # angles in degrees
doc.add_polyline([(0, 0), (10, 0), (10, 10)])
doc.add_text("SXF 日本語", (10, 80), height=3.5, width=24)
doc.update_element(line_id, end_x=120.125, color=2)
doc.save("created.sfc")

existing = ezsxf.edit_sfc(ezsxf.parse_sfc("created.sfc"))
existing.update_element(line_id, start_x=5, start_y=10)
existing.remove_element(line_id)
existing.save("created.sfc")
```

`new_sfc` creates a free-size sheet in millimetres, layer code 1 and text-font
code 1. `timestamp` can be specified; it defaults to the local current datetime.
Default style declarations are black, continuous, and 0.13 mm width.
Text `width` is the width of the complete text box in sheet millimetres.
`add_layer`/`add_font` return codes; primitive additions return entity IDs.
`rename_layer` keeps its code. `to_dict` returns an independent parser-compatible
snapshot; `to_bytes` and `save` use the validated writer. Existing header metadata
is retained. External SAF references require the same explicit opt-in for these
two methods; save to a source directory and use `write_sfc_bundle` for delivery.

`update_element(id, **changes)` retains unspecified values, ID and record order:

| Element | Geometry/text fields |
| --- | --- |
| Line | `start_x`, `start_y`, `end_x`, `end_y` |
| Circle | `center_x`, `center_y`, `radius` |
| Arc | `center_x`, `center_y`, `radius`, `direction`, `start_angle`, `end_angle` |
| Polyline | `points` (list of coordinate pairs; at least two) |
| Text | `text`, `x`, `y`, `height`, `width`, `spacing`, `angle`, `slant`, `base_point`, `direction` |

All support `layer`/`color`; line/curve elements also support `line_type` and
`line_width`, text supports `font`. Numeric updates require actual finite
numbers. Each operation validates a candidate in Rust and commits only on
success. Excess precision is rejected without rounding. Updates/removals are
supported geometry, including existing group and composite-boundary children.
Ordinary group placements, dimensions and supported fills/hatches have explicit
editing APIs; see [complex-element editing](docs/sfc-editing.md). SAF attachment
operations remain limited to the five basic primitives directly on the sheet.
P21 generation is not implemented.

## Creating and editing SAF attributes and raster placements

```python
doc = ezsxf.new_sfc("drawing.sfc")
circle = doc.add_circle((50, 50), 10)
doc.set_attribute(circle, "Material", "Steel")
doc.set_attribute(circle, "Height", "12.5", attribute_type="LEN", unit="m")
image = doc.add_image("scan.tif", (80, 40), 100, 60, angle=15)
doc.save_bundle("delivery", file_name="drawing.sfc")

edited = ezsxf.edit_sfc_bundle("delivery/drawing.sfc")
edited.set_attribute(circle, "Material", "Concrete")
edited.remove_attribute(circle, "Height")
edited.update_image(image, (90, 45), 100, 60, image="replacement.tif")
edited.save_bundle("revised", file_name="revised.sfc")
```

`set_attribute` creates an ATRF wrapper or updates its loaded SAF. It returns
the stable figure identifier and keeps the target entity ID. Attribute identity
is its set metadata (`set_name`, `set_version`, `designed_by`), `group` path
(at most two levels), and `name`. Repeated names at that address are ambiguous
and cannot be updated by name. `get_attributes` returns the values and metadata.
Removing the final attribute unwraps the element; `remove_attachment` removes
all its attributes, and `remove_element` removes the wrapper and target together.
Elements referenced by a SAF `ターゲット` attribute cannot be detached.

Use `edit_sfc_bundle` to load an existing SFC, SAF and file dependencies before
editing SAF data. This API validates the supported SAF grammar, rejects unknown
XML fields, and upgrades legacy 3.0 inline `AttributeSet` data to 3.1 root
definitions and `AttrSetRef` children. Authored SAFs use UTF-8 and normalize XML
formatting, comments and encoding. `write_sfc_bundle` remains the byte-preserving
route for unedited vendor XML. `saf_bytes` returns the authored SAF (or `None`);
`validate_saf(bytes)` checks the 3.1 DTD structure, required fields, unique set
metadata/IDs, figure IDs, set references and group depth. External DTD contents
and custom vendor schemas are not evaluated; type/unit/value interpretation
remains the caller's responsibility.

`add_dependency(path, file_name=...)` supplies files referenced by `ファイル名`
attributes. `save_bundle` checks dependencies and collisions before publishing
a new directory; SFC header, ATRF names and SAF `sxfFile` are renamed together.
Unused dependencies are omitted. Loading captures dependency bytes, so later
changes to source files do not affect the edited document.

`add_image` and `update_image` accept TIFF/JPEG files and generate the standard
image attribute and a clockwise closed rectangle. New images use the SXF 3.1
common-set ATRU mechanism and need no SAF of their own. Existing SXF 3.0
SAF/ATRF images retain their mechanism when edited. Inline image metadata is
available in `to_dict()["model"]["attribute_attachments"]`;
`get_attributes` reads SAF attributes.
The anchor is the lower-left corner; dimensions are sheet millimetres and angles
are degrees. Computed corners are rounded to the SFC six-decimal coordinate
limit. TIFF metadata must describe one page of G4 strips, monochrome 1-bit
pixels, normal orientation and dimensions no greater than 13,000 pixels.
JPEG frame/segment headers are checked. Pixel decoding and image conversion are
not performed, and supported headers alone do not establish valid pixel data.
Image geometry updates must retain a clockwise rectangle.

`set_single_attribute` authors ATRU names and `set_text_attribute` authors ATRS
names (text targets only); use prescribed common attribute names/types for these
mechanisms. Arbitrary attribute sets should use SAF. All attachment APIs
currently target the same five basic element types; grouped elements are not
editable. Failed operations keep both SFC and SAF state unchanged.

See [writer validation and Windows procedure](docs/sfc-writer-validation.md) for
the distinction between model tests, Wine CAD checks and native Windows saving.

For group/SAF/image re-export review, prepare seven owned inputs (Pillow is
required for preparing the image cards), then compare each input with its
actual CAD export:

```bash
python scripts/prepare_sfc_reexport.py cad-reexport-inputs
python scripts/verify_sfc_reexport.py cad-reexport-inputs/group.sfc cad-output/group.sfc --report group-review.json
```

The report must be a new file. The comparator exits with status 1 for lost or
changed data and unsupported features. It checks bounded basic geometry,
group hierarchy/reuse, SAF values/bindings and referenced file bytes. TIFF/JPEG
recompression requires separate pixel/visual review. Per-check results help
explain a failure; only `passed: true` satisfies the complete comparison gate.
Record actual CAD operations and display separately: a successful comparison
of local copies does not qualify a third-party CAD. See the
[native re-export results](docs/sfc-writer-validation.md#group-and-bundle-re-export-follow-up-on-2026-10-06).

## Title-block (sheet) attribute names

SXF represents title-block information in two related places. The
`drawing_attribute_feature` stores the drawing-wide values and is returned in
`typed_features` with `kind == "drawing_attribute"`. Text drawn in the title
block is linked to those values by an `ATRS` attribute attachment. By contrast,
`drawing_sheet_feature` (`kind == "drawing_sheet"`) describes the paper size
and orientation; `model["sheet"]` is the resolved paper/container structure,
not the title-block metadata.

The following strings are the exact, machine-readable predefined attribute
names used in an `ATRS` attachment. They are case-sensitive literals and must
not be translated or replaced by the `S-xx` catalogue identifier. All eleven
attributes have the predefined SXF type `STR`.

| Catalogue ID | Exact `attribute_name` | SFC field / Python key | Meaning |
| --- | --- | --- | --- |
| `S-05` | `表題_事業名` | `P_Name` / `project_name` | Project name |
| `S-06` | `表題_工事名` | `C_Name` / `construction_name` | Construction name |
| `S-07` | `表題_契約区分` | `C_type` / `contract_type` | Contract type |
| `S-08` | `表題_図面番号` | `D_number` / `drawing_number` (before `$$`) | Drawing number |
| `S-09` | `表題_図面総数` | `D_number` / `drawing_number` (after `$$`) | Total drawing count |
| `S-10` | `表題_図面種別` | `D_type` / `drawing_type` | Drawing type |
| `S-11` | `表題_尺度` | `D_Scale` / `drawing_scale` | Scale |
| `S-12` | `表題_図面名` | `D_title` / `drawing_name` | Drawing name |
| `S-13` | `表題_年月日` | `D_Year`, `D_Month`, `D_Day` / `drawing_year`, `drawing_month`, `drawing_day` | Drawing date |
| `S-14` | `表題_会社名` | `C_Contractor` / `contractor_name` | Contractor name |
| `S-15` | `表題_事務所名` | `C_Owner` / `owner_name` | Owner/commissioning organization name |

An explicit-type attachment name has this form:

```text
$$ATRS$$<figure-id>$$<attribute-name>$$STR
```

For example, `$$ATRS$$9$$表題_事業名$$STR` is exposed at
`model["attribute_attachments"][...]["attribute"]` as:

```python
{
    "mechanism": "ATRS",
    "figure_id": "9",
    "attribute_name": "表題_事業名",
    "attribute_type": "STR",
    "unit": None,
}
```

Because `ATRS` applies to a text feature, that feature's displayed text is the
attribute value; there is no separate `attribute_value` key. The type and unit
may be omitted from the encoded name, in which case their parsed values are
`None`.

For a title-block value drawn on multiple lines, use the unsuffixed predefined
name for a single line. For multiple lines, the SXF specification's published
form appends an ASCII space and a 1-based ASCII line number, for example
`表題_工事名 1` and `表題_工事名 2`. `ezsxf` preserves this suffix verbatim and
does not fold the line-specific names back to the base name.

There are two compound-field rules to keep separate from the `ATRS` names:

- `D_number` / `drawing_number` stores `<drawing-number>$$<total-count>` when a
  total count is present, while title-block text uses separate
  `表題_図面番号` and `表題_図面総数` attachments.
- `表題_年月日` is one text attribute, while `drawing_attribute_feature`
  exposes its date as the three integer keys `drawing_year`, `drawing_month`,
  and `drawing_day`.

Convert the original input or an already parsed result to DXF:

```python
ezsxf.to_dxf("./data/D0LS004ZSFC/D0LS004Z.SFC", "drawing.dxf")

parsed = ezsxf.parse_sfc("./data/D0LS004ZSFC/D0LS004Z.SFC")
dxf_text = ezsxf.to_dxf(parsed)
```

Draw with matplotlib and save through the returned `Axes`:

```python
ax = ezsxf.plot("./data/D0LS004ZSFC/D0LS004Z.SFC")
ax.figure.savefig("drawing.png", dpi=200, bbox_inches="tight")
```

Both backends share the same hierarchy, placement, layer, color, line type,
line width, text, dimension, and hatch conversion. Curves are converted to
polylines; use `curve_segments` to control the approximation resolution.

Converters that need true curves can read `PathPrimitive.curve`
(`CurveGeometry`): for circle, arc, ellipse and elliptical-arc sources it holds
the exact curve behind the sampled `points`, already transformed through
compound-figure placements. The curve is `center + axis_u*cos(t) + axis_v*sin(t)`
for `t` from `start_param` to `end_param`; `axis_u`/`axis_v` are conjugate
semi-diameters, so a circle placed with unequal X/Y ratios is reported as the
ellipse it becomes.

Drawing conversion currently targets SFC input. Externally defined symbols are
shown as insertion markers, while externally defined and tiled hatch patterns
retain only boundaries marked visible by the SXF data.

## CLI

```bash
# smoke test
python -m ezsxf

# parse to JSON
python -m ezsxf parse sfc ./data/D0LS004ZSFC/D0LS004Z.SFC --pretty
python -m ezsxf parse p21 ./data/D0LS004ZP21/D0LS004Z.P21 --lenient

# convert SFC to DXF
python -m ezsxf to-dxf ./data/D0LS004ZSFC/D0LS004Z.SFC drawing.dxf

# validate and resave SFC (input and output can be the same file)
python -m ezsxf resave-sfc drawing.sfc copy.sfc
# preserve external references; arrange SAF/image files separately
python -m ezsxf resave-sfc drawing.sfc copy.sfc --allow-external-references

# deliver SFC/SAF/images together into a new directory
python -m ezsxf bundle-sfc drawing.sfc delivery --file-name renamed.sfc

# save or interactively display a matplotlib drawing
python -m ezsxf plot ./data/D0LS004ZSFC/D0LS004Z.SFC drawing.png --dpi 200
python -m ezsxf plot ./data/D0LS004ZSFC/D0LS004Z.SFC
```

## Development

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test

# Python-level tests
python -m unittest discover -s tests -p 'test_*.py' -v
```

## Repository Layout

- `src/*.rs`: Rust parser, resolved model, and PyO3 bindings
- `src/ezsxf/`: Python API, CLI, DXF writer, matplotlib backend, and stubs
- `data/`: SXF sample datasets used for validation

## License

MIT License. See [LICENSE](./LICENSE).
