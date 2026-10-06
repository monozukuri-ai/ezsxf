# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog, and this project follows Semantic Versioning.

## [Unreleased]

### Added
- Strict SFC resaving with atomic replacement, model round-trip validation,
  external dependency bundle delivery, and basic-element creation/edit APIs.
- SAF attribute creation, editing and removal; supported 3.0/3.1 XML validation;
  TIFF/JPEG image creation and placement updates. New SXF 3.1 images use ATRU;
  existing SAF/ATRF images retain their mechanism.
- Native Windows MSVC CI and filesystem save checks, including Japanese paths,
  locked/read-only destinations, SAF/image delivery and failed-save cleanup.
- Dedicated Windows Jw_cad save/overwrite/reopen qualification with pinned
  official installers and recorded screenshots/output hashes.
- `PathPrimitive.curve` (`CurveGeometry`): the exact circle/arc/ellipse behind a sampled path, already transformed through compound-figure placements (conjugate semi-diameters, so unequal X/Y ratios stay exact). Set for SFC `circle`/`arc`/`ellipse`/`ellipse_arc` features and P21 `CIRCLE`/`ELLIPSE`/circular `TRIMMED_CURVE`; lets converters emit true ARC/CIRCLE/ELLIPSE entities instead of polylines
- Core P21/SFC parsing with strict and lenient modes
- Typed SFC feature extraction and Python dictionary output
- CLI entrypoint (`python -m ezsxf`, `ezsxf parse ...`)
- Real sample-data compatibility tests for paired P21/SFC datasets
- Python API smoke/integration tests (`tests/test_python_api.py`)

### Changed
- Improved P21 parse performance by avoiding expensive keyword lookahead error paths
- Expanded SFC string handling for backslash-quoted inputs used in real datasets
