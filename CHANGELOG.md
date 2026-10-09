# Changelog

## [Unreleased]

- P21 output for geodetic partial drawings (kind 2), retaining the SFC
  coordinates and placement transform with `$$SXF_FG_` identifiers.
- Common P21/SFC sheet and subfigure metadata in `typed_features` and `model`,
  including decoded Unicode names, fractional FREE paper sizes, nested/shared
  placements and attribute wrappers. Original STEP records remain intact.

- P21 drawing conversion now renders trimmed ellipses and retains exact
  elliptical-arc geometry, local parameter trims, sense and nonuniform placements.

- Structured `extend` batches for fills, hatches, composite boundaries, nested
  parts/groups and partial drawings, plus direct insertion into a definition.
- Zero-based input indices for batch errors and atomic `on_invalid="skip"`
  returning IDs and rejection reasons. Placement scale failures identify the
  positive-scale requirement directly.
- Native P21 point markers (all seven kinds) and cubic Bezier spline segments;
  explicit standalone `unsupported="drop"` with a source-entity report.
- `new_sfc(target="p21")`, `validate_p21()` and source IDs in P21 geometry
  failures. Zero-area fill/hatch additions are rejected for either target.
- Shared identical P21 Cartesian points and public `ezsxf.build_drawing`.
- Reproducible 50,000-feature structured/20,000-input rejection workloads and
  paired marker/spline CAD controls. Native CAD checks for these changes pending.

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog, and this project follows Semantic Versioning.

## [Unreleased]

### Added
- AP202 output for five dimension kinds, leaders/balloons, composite boundaries,
  four fill/hatch styles, ellipses and elliptic arcs, drawing title metadata and
  ATRF/ATRU/ATRS attribute groups.
- Complete P21/SAF/image delivery with `save_p21_bundle`/`write_p21_bundle`, and
  deterministic Deflate P2Z output with `save_p2z`, `to_p2z_bytes` and `write_p2z`.
  CLI commands `bundle-p21` and `to-p2z` validate dependencies before saving.
- Rust source builds now require Rust 1.88 or later for the ZIP dependency.

- Atomic `SfcDocument.extend` for one-pass batch validation and `create_part`
  for shared definitions with only explicit placements.
- Standard A0..A4/FREE paper codes and orientations, with integral-float paper
  dimensions and explicit rejection of fractional SFC millimetres.
- Horizontal CP932 monospaced text-box estimates when width is omitted.
- AP202 P21 generation for basic curves/text, styles, mathematical partial
  drawings, groups and shared parts; Python save/bytes APIs and `to-p21` CLI.
  Unsupported features fail before replacing the destination.
- Reproducible batch timings and Windows scale-review inputs.

### Fixed
- P21 drawing conversion now retains typed font identifiers and vertical-text
  transforms, and honours clockwise/major circular arcs and hidden layers.

## [0.2.0] - 2026-10-06

### Added
- Style factories for predefined/RGB colours, predefined/custom line types and
  paper-mm line widths, with definition reuse, stable codes and transactional
  validation of pattern lengths, numeric precision and specification limits.
- Documented basic writer contract, installed-package MVP verification inputs,
  wheel/source payload checks and multi-platform release qualification.
- Vendor-free CAD re-export inputs and a read-only comparison gate for basic
  geometry, group hierarchy/reuse, SAF bindings/values and TIFF/JPEG bytes.
- Named-field creation/editing of ellipses, ellipse arcs, splines, clothoids,
  point markers, dimensions and leaders; ordinary group/part operations and
  composite boundaries with solid fills and user-defined hatches.
- Native Japanese-system-locale qualification controls and custom CAD case
  manifests, including Windows PowerShell 5.1 script compatibility.
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
