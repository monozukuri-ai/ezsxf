# Contributing

## Setup

```bash
python -m venv .venv
source .venv/bin/activate
pip install maturin
maturin develop
```

## Validation Checklist

Run these before opening a PR:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
python -m unittest discover -s tests -p 'test_*.py' -v
```

## Development Notes

- Keep parsing logic in `src/lib.rs`.
- Keep Python wrappers thin (`src/ezsxf`).
- Update `src/ezsxf/_core.pyi` when changing exported Rust functions.
- Add regression tests for parser fixes and new SXF feature handling.

## Pull Requests

- Keep PRs focused to one logical change.
- Use descriptive commit messages (e.g. `feat(parser): support ...`).
- Include test evidence in the PR description.
- Follow the project code of conduct in [CODE_OF_CONDUCT.md](./CODE_OF_CONDUCT.md).
