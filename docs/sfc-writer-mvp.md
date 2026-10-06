# Basic SFC writer contract

The writer MVP is a standalone SFC document containing a free-size sheet,
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
doc.add_text("日本語", (10, 30), height=3.5, width=30, layer=layer)
doc.update_element(line, end_x=120.125)
doc.save("drawing.sfc")
assert ezsxf.parse_sfc("drawing.sfc", strict=True) == doc.to_dict()
```

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

## Geometry, numbers and strings

- Coordinates, radii and text box sizes are sheet millimetres. Positive X points
  right; positive Y points up. Widths and dash patterns are paper millimetres.
- Angles are degrees, with zero to the right. Arc `direction=0` is
  counterclockwise, `direction=1` clockwise; start/end angles are in `[0, 360)`
  and must differ. A full circle uses `add_circle`.
- Text `width` is the **complete box width**, not one character's width.
  `height` is its height. Default `base_point=1` means lower left; `direction=1`
  means horizontal writing. See the method type stubs for other parameters.
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

## State, saving and qualification

Edits validate a candidate in Rust before committing. A failed edit keeps the
document unchanged. `to_dict()` returns an independent snapshot; a modified
snapshot must remain consistent with its typed/model caches to be writable.
`save(path)` validates, encodes and atomically replaces a regular destination;
it preserves an existing file after validation failure. Symlink destinations
are rejected. It does not provide power-loss durability.

The MVP output is standalone: no SAF, images or external references are needed.
Use the existing bundle APIs for those dependencies; they have a separate
acceptance scope. P21 output, complete CAD editing support and lossless re-export
through arbitrary third-party CAD are outside this basic contract.

Run against an installed wheel or a package built from its source distribution:

```sh
python scripts/verify_sfc_mvp.py --output new-verification-directory
python -m unittest discover -s tests -p 'test_*.py' -v
```

The first command checks five primitives with predefined/custom styles, edited
geometry, strict structural equality, deterministic output with a fixed
timestamp, CP932 Japanese/backslashes, new/existing Japanese paths, failure
rollback and installed type information. It produces owned CAD inputs and hashes.
It does **not** perform a CAD visual check. Platform/package and external CAD
evidence are described in [writer validation](sfc-writer-validation.md).

Reference: SXF Ver.3.1 Feature Specification, second edition, printed pp.12–17
(style definitions/codes), pp.24 and 28 (arcs/text). The SFC encoding specification
defines numeric spelling and quoting; bundled reference PDFs are read-only and
are not distributed as package contents.
