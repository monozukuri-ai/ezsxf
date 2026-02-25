# ezsxf

A fast SXF parser for Python, implemented in Rust with PyO3.

`ezsxf` supports both SXF formats:
- `P21` (ISO-10303-21 based)
- `SFC` (SXF feature blocks)

## Status

This project is in active development. The core parser, strict/lenient modes, typed SFC feature extraction, and real-world sample validation are implemented.

## Features

- Parse `P21` and `SFC` from file path, text, or bytes
- Strict and lenient parsing modes
- Structured parse output (`header`, `entities`, `typed_features`, `warnings`)
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
