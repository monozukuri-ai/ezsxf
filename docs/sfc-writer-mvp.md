# Basic SFC writer contract

The writer MVP is a standalone SFC document containing a standard or free-size sheet,
layers, style/font definitions, and lines, circles, circular arcs, polylines
and text. Its public Python API is available from `ezsxf` in version 0.2.0.
It has no runtime Python dependencies. Application integration is outside this
contract. Other editing/bundle APIs remain available with their documented limits.

## Definitions and identifiers

```python
import ezsxf

doc = ezsxf.new_sfc("drawing.sfc", name="基本図面", width_mm=297, height_mm=210)
layer = doc.add_layer("構造")
color = doc.add_color((32, 96, 160))
line_type = doc.add_line_type("独自線", pattern=[5, 2, 1, 2])
line_width = doc.add_line_width(0.42)
line = doc.add_line((10, 10), (100, 10), layer=layer, color=color,
                    line_type=line_type, line_width=line_width)
doc.add_circle((50, 50), 10, layer=layer)
doc.add_arc((80, 50), 10, 0, 90, layer=layer)
doc.add_polyline([(10, 70), (30, 80), (50, 70)], layer=layer)
doc.add_text("日本語", (10, 30), height=3.5, width=30, layer=layer)
doc.update_element(line, end_x=120.125)
doc.save("drawing.sfc")
assert ezsxf.parse_sfc("drawing.sfc", strict=True) == doc.to_dict()
```

`new_sfc` creates a free-size sheet in millimetres, layer code 1 and font code 1.
Default styles are black, continuous and 0.13 mm. Its optional `timestamp`
defaults to the local current datetime; specify it for reproducible output.

For standard paper, use `paper="A1"` (A0..A4 or FREE) and
`orientation="landscape"` or `"portrait"`. Integer codes are also accepted:
paper 0..4 or 9; orientation 0 or 1. Defaults remain FREE, landscape, 297×210.
Standard sizes are derived from the paper and orientation; explicitly supplied
dimensions must agree. `width_mm`/`height_mm` accept integers and integral floats
such as `841.0`. FREE dimensions are **integer millimetres in the SFC spec**;
fractional values such as `841.5` raise `ValueError`. Round explicitly if needed.

## Bulk additions

```python
ids = doc.extend([
    {"kind": "line", "start": (0, 0), "end": (100, 0), "layer": layer},
    {"kind": "circle", "center": (50, 50), "radius": 10},
    {"kind": "polyline", "points": [(0, 10), (10, 20), (20, 10)]},
    {"kind": "text", "text": "日本語ABC", "anchor": (0, 30)},
])
```

`extend` consumes an iterable of dictionaries with a required `kind`, returning
entity IDs in input order. It builds all records, clones the drawing once,
inserts once and validates the complete candidate once. A failure anywhere,
including an input generator exception, leaves the document unchanged.
An empty iterable is a no-op. New elements are appended to the sheet, retaining
existing group definitions and style codes. Style definitions should be created
before the batch. Complex leaf kinds accepted by `add_feature` also work.
Inline fills, hatches, shared parts, groups and partial drawings are supported;
see [structured bulk authoring](bulk-writing.md) for their dictionary shapes,
`on_invalid="skip"`, direct definition insertion and P21 preflight.
Fields use `update_element` names from the tables below; basic authoring also
accepts `start`, `end`, `center` and `anchor` coordinate pairs. Conflicting pair
and scalar fields are rejected. `to_bytes` still performs final save validation;
no unchecked document state is exposed. Batch size controls peak memory.

Use `scripts/verify_writer_requests.py` for a release-build timing comparison
against individual additions. JWW speed parity requires a separate comparison
on the same machine, input and validation boundary.

Factories return **SXF codes**, while geometry additions return **entity IDs**.
Always pass the returned codes to geometry methods; a code is not an entity ID.
Definitions precede use. Adding a style preserves existing bindings and IDs.
Equivalent definitions are reused without modifying the document.

| Factory | Input and allocation |
| --- | --- |
| `add_color(name)` | The 16 predefined English names, codes 1–16. ASCII case and surrounding/repeated whitespace are normalized. |
| `add_color((r, g, b))` | Tuple/list of exactly three integer components, each 0–255; up to 240 user colours, codes 17–256. Exact RGB reuse. |
| `add_line_type(name)` | The 15 predefined English names, codes 1–15. Code 16 is reserved. |
| `add_line_type(name, pattern=[...])` | Unique, nonempty custom name and 2/4/6/8 positive lengths, alternating drawn segment and gap, in paper mm; codes 17–32. Exact name/pattern reuse. A changed pattern under the same name fails. |
| `add_line_width(mm)` | Predefined widths 0.13, 0.18, 0.25, 0.35, 0.5, 0.7, 1, 1.4, 2 (codes 1–9). Other widths use codes 11–16; code 10 is reserved. Exact width reuse. |
| `add_font(name)` | Exact-name reuse or append; code 1 initially refers to `ＭＳ ゴシック`. Font installation/rendering is the CAD application's responsibility. |
| `add_layer(name)` | Append a layer; `rename_layer(code, name)` retains its code. |

Named colours and RGB definitions are separate: `(255, 0, 0)` creates/reuses a
custom RGB definition, while `"red"` returns predefined code 2. Custom line names
are exact strings and must not name a predefined type. Booleans and numeric
strings are rejected by the style factories. Filled user tables still permit
reuse and new predefined definitions. Unused definitions are retained; the API
does not prune/renumber styles or claim OCF certification.

Predefined colours: `black`, `red`, `green`, `blue`, `yellow`, `magenta`, `cyan`,
`white`, `deeppink`, `brown`, `orange`, `lightgreen`, `lightblue`, `lavender`,
`lightgray`, `darkgray`. Predefined line types: `continuous`, `dashed`,
`dashed spaced`, `long dashed dotted`, `long dashed double-dotted`,
`long dashed triplicate-dotted`, `dotted`, `chain`, `chain double dash`,
`dashed dotted`, `double-dashed dotted`, `dashed double-dotted`,
`double-dashed double-dotted`, `dashed triplicate-dotted`,
`double-dashed triplicate-dotted`.

## Editing basic elements

`update_element(id, **changes)` retains unspecified fields, the entity ID and
record order. `remove_element(id)` checks references before removing a record.
For the `line` ID returned in the example above:

```python
existing = ezsxf.edit_sfc(ezsxf.parse_sfc("drawing.sfc"))
existing.update_element(line, start_x=5, start_y=10)
existing.remove_element(line)
existing.save("drawing.sfc")
```

For a different drawing, obtain its entity IDs from `typed_features` or `model`.
The supported basic geometry changes are:

| Element | Fields |
| --- | --- |
| Line | `start_x`, `start_y`, `end_x`, `end_y` |
| Circle | `center_x`, `center_y`, `radius` |
| Circular arc | `center_x`, `center_y`, `radius`, `direction`, `start_angle`, `end_angle` |
| Polyline | `points`, containing at least two coordinate pairs |
| Text | `text`, `x`, `y`, `height`, `width`, `spacing`, `angle`, `slant`, `base_point`, `direction` |

All five types accept `layer` and `color`; curves also accept `line_type` and
`line_width`, and text accepts `font`. Use definition codes returned by the
factories. Unknown fields, unsupported edits and invalid numeric values fail.
For children of groups or composite curves, coordinates are local to their
owner; see [complex editing](sfc-editing.md) for references and hierarchy limits.

## Geometry, numbers and strings

- Coordinates, radii and text box sizes are sheet millimetres. Positive X points
  right; positive Y points up. Widths and dash patterns are paper millimetres.
- Angles are degrees, with zero to the right. Arc `direction=0` is
  counterclockwise, `direction=1` clockwise; start/end angles are in `[0, 360)`
  and must differ. A full circle uses `add_circle`.
- Text `width` is the **complete box width**, not one character's width.
  `height` is its height. Default `base_point=1` means lower left; `direction=1`
  means horizontal writing. See the method type stubs for other parameters.
- Omitted `width` (or `None` in `add_text`) estimates a horizontal monospaced box:
  full-width CP932 glyphs use `height`, single-byte glyphs use `height / 2`, and
  `spacing` is added between Unicode characters. `estimate_text_width(text,
  height=3.5, spacing=0.0)` exposes the same rule. Only the generated estimate is
  rounded to six decimal places. Explicit width is preserved. Vertical text
  requires explicit width; proportional fonts need application-specific metrics.
  Updating text or height retains the existing width unless explicitly changed.
- Length fields accept at most six fractional digits; angles/scales at most
  fifteen digits. Values must be finite and within each feature's bounds.
  Excess precision fails without rounding; prepare values explicitly before
  calling the API. Floating point arithmetic can produce excess digits.
- Strings must be representable losslessly in CP932. Unsupported characters,
  including emoji, raise `ValueError`; output never substitutes replacement
  characters. Feature strings are limited to 256 encoded bytes.
- Standard SFC quoting escapes a semantic backslash as two backslashes in the
  file. Strict ezsxf reload returns the original single backslash. Some CAD
  displays show both characters or a yen glyph. `literal_backslashes=True`
  provides a separately tested compatibility mode and rejects ambiguous strings;
  it does not change the standard default or guarantee all CAD rendering.

## State and saving

Edits validate a candidate in Rust before committing. A failed edit keeps the
document unchanged. `to_dict()` returns an independent snapshot; a modified
snapshot must remain consistent with its typed/model caches to be writable.
`to_bytes()` uses the same validated serializer. Existing header metadata is
retained. Both `to_bytes` and `save` require `allow_external_references=True`
to retain external SAF references; that flag does not copy their dependencies.
`save(path)` validates, encodes and atomically replaces a regular destination;
it preserves an existing file after validation failure. Symlink destinations
are rejected. It does not provide power-loss durability.

The MVP output is standalone: no SAF, images or external references are needed.
Use the existing bundle APIs for those dependencies; they have a separate
acceptance scope. [P21 output](p21-writing.md) has a separate supported subset.
Complete CAD editing support and lossless re-export through arbitrary third-party
CAD are outside this basic contract.

For complete save semantics and external dependencies, see [SFC saving](sfc-writing.md)
and [SAF/image bundles](sfc-bundles.md). See [compatibility](compatibility.md) for
CAD display and re-export limits.

The style and geometry definitions follow the SXF Ver.3.1 Feature Specification,
second edition, printed p.10 (paper), pp.12–17 (style codes), pp.24 and 28 (arcs
and text), and Implementation Agreement §1-5 (monospaced text width).
