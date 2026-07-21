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
