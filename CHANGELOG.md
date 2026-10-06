# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog, and this project follows Semantic Versioning.

## [Unreleased]

### Added
- `PathPrimitive.curve` (`CurveGeometry`): the exact circle/arc/ellipse behind a sampled path, already transformed through compound-figure placements (conjugate semi-diameters, so unequal X/Y ratios stay exact). Set for SFC `circle`/`arc`/`ellipse`/`ellipse_arc` features and P21 `CIRCLE`/`ELLIPSE`/circular `TRIMMED_CURVE`; lets converters emit true ARC/CIRCLE/ELLIPSE entities instead of polylines
- Core P21/SFC parsing with strict and lenient modes
- Typed SFC feature extraction and Python dictionary output
- CLI entrypoint (`python -m ezsxf`, `ezsxf parse ...`)
- Real sample-data compatibility tests for paired P21/SFC datasets
- Python API smoke/integration tests (`tests/test_python_api.py`)

### Changed
- Improved P21 parse performance by avoiding expensive keyword lookahead error paths
- Expanded SFC string handling for backslash-quoted inputs used in real datasets
