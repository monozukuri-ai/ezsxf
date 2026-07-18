# ezsxf

A fast SXF parser for Python, implemented in Rust with PyO3.

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
- Python API + CLI (`ezsxf` / `python -m ezsxf`)

## Installation

### From source (recommended currently)

```bash
python -m venv .venv
source .venv/bin/activate
pip install maturin
maturin develop
```

## Quick Start

```python
import ezsxf

result = ezsxf.parse_sfc("./data/D0LS004ZSFC/D0LS004Z.SFC", strict=True)
print(result["format"])          # "sfc"
print(len(result["typed_features"]))
print(result["model"]["sheet"]["component_ids"])
```

## CLI

```bash
# smoke test
python -m ezsxf

# parse to JSON
python -m ezsxf parse sfc ./data/D0LS004ZSFC/D0LS004Z.SFC --pretty
python -m ezsxf parse p21 ./data/D0LS004ZP21/D0LS004Z.P21 --lenient
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

- `src/lib.rs`: Rust parser and PyO3 bindings
- `src/ezsxf/`: Python package (`__init__`, CLI entrypoint, stubs)
- `data/`: SXF sample datasets used for validation
- `resources/`: SXF specification PDFs (reference only)

## License

MIT License. See [LICENSE](./LICENSE).
