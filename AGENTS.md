# Repository Guidelines

## Project Structure & Module Organization
- `src/lib.rs`: Rust implementation of the PyO3 extension module (`_core`).
- `src/ezsxf/__init__.py`: Python-facing API and CLI entrypoint glue.
- `src/ezsxf/_core.pyi`: Type stubs for functions exposed from Rust.
- `resources/`: SXF specification PDFs used as reference material.
- `Cargo.toml` and `pyproject.toml`: crate/package metadata and `maturin` build settings.

Keep core parsing logic in Rust and expose a small, stable Python API from `src/ezsxf`.

## Build, Test, and Development Commands
- `maturin develop`: Build and install the extension into the active virtual environment.
- `cargo build`: Compile the Rust crate quickly during development.
- `cargo test`: Run Rust tests (unit + integration).
- `cargo fmt --check`: Verify Rust formatting.
- `cargo clippy --all-targets --all-features -- -D warnings`: Run strict Rust lint checks.
- `python -c "from ezsxf import hello_from_bin; print(hello_from_bin())"`: Smoke test Python import after build.

## Coding Style & Naming Conventions
- Rust: use `rustfmt` defaults (4-space indentation), `snake_case` for functions/modules, `CamelCase` for types.
- Python: follow PEP 8 naming (`snake_case`), keep exports explicit in `__init__.py`.
- Keep the PyO3 boundary thin: business/parsing logic in Rust, minimal wrapper code in Python.
- When adding exported Rust functions, update `_core.pyi` in the same change.

## Testing Guidelines
- Prefer Rust tests first; add unit tests near code (`#[cfg(test)]`) and integration tests under `tests/`.
- Add regression tests for each parser fix or new SXF element handled.
- Minimum local validation before PR: `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets --all-features -- -D warnings`.
- If Python wrappers expand, add `pytest` tests under `tests/`.

## Commit & Pull Request Guidelines
- Current history is minimal (`first commit`), so use clear, imperative commit subjects.
- Recommended pattern: `type(scope): summary` (example: `feat(parser): add STEP AP202 header reader`).
- Keep each PR focused to one logical change.
- PR descriptions should include: purpose, related SXF spec reference (document/page), and test evidence (commands run + results).

## Security & Configuration Tips
- Treat files in `resources/` as read-only references; do not modify or rename source PDFs casually.
- Do not commit generated binaries, local virtualenvs, or temporary conversion outputs.
