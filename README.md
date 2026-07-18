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
- `resources/`: SXF specification PDFs (reference only)

## License

MIT License. See [LICENSE](./LICENSE).
