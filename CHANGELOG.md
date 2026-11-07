# Changelog

## [Unreleased]

- SFC and P21 output is written in pieces of about 4 MB and joined once,
  instead of one buffer that doubled and copied itself as it grew. When a large
  drawing had left free space in the heap, the buffer grew there and every old
  copy stayed resident. On Linux each piece's pages are given back as soon as it
  is copied into the result, and so are those of the result as it is handed to
  Python. For a committed drawing of 670,698 entities, returning its 291 MB P21
  peaked 509 MB above the document instead of 843 MB, and its 99 MB SFC 121 MB
  instead of 255 MB. Output bytes are unchanged.

- A typed feature takes 168 bytes instead of 488. `TypedFeature` boxes the
  variants larger than an arc (text, the five dimensions, labels, balloons,
  hatching and tiles, ellipse arcs, clothoids, placements, external symbols and
  hatches, and the drawing attribute), whose size every line and polyline paid
  for as well. A committed drawing of 670,698 entities (99 MB of SFC) holds
  736 MB instead of 941 MB. Rust code matching those variants now gets a `Box`.
- `to_bytes`, `to_p21_bytes`, `to_p2z_bytes`, `serialize_sfc` and
  `serialize_p21` hand their output to Python a piece at a time, freeing each
  piece of the native buffer once copied, rather than copying it whole while
  both are held.

- `extend` turns each leaf into its record as it reads the batch, instead of
  first holding every leaf as a field map with lists of values, and moves the
  records into the document rather than copying them. A commit no longer holds
  a second copy of the records: its reparse compares each entity with the one
  committed and keeps that, and the bytes go once decoded. The parser reads its
  text in place rather than as a `Vec<char>` (four bytes a character), the UTF-8
  check of an input no longer copies it, and SFC output is encoded to Shift-JIS
  entity by entity rather than as one string plus a decoded copy for the check.
  Converting a DWG whose partial drawing has 172,695 elements and 1.8 million
  vertices to SFC went from 2.33 GB to 1.24 GB peak. Output bytes and error
  messages are unchanged.
- The `points` / `vertices` of a leaf may be one `array('d')` of
  `x0, y0, x1, y1, ...`, holding a vertex in 16 bytes instead of a tuple of two
  floats (104 bytes) until `extend` reads it; the drawing above then peaks at
  1.05 GB.

- Bundles no longer clone and reparse a large drawing. `save_bundle` emits the
  committed entities under the delivered header (a clone plus a reparse of the
  emitted SFC cost several times the drawing), and `to_p2z_bytes` /
  `save_p21_bundle` build AP202 from the document itself when there is no SAF.
  A drawing with a SAF keeps the renamed-SFC route, since its ATRF references
  take the delivered name.
- P21 output is written record by record, each checked against its own parse,
  instead of building the whole AP202 graph and parsing the finished text back as
  one document. The SFC is no longer re-encoded and reparsed first either. A 96
  sheet drawing set (130 MB of P21) went from 4.3 GB to under 2 GB peak; the output
  bytes are unchanged.
- `SfcDocument.to_bytes` no longer reparses a document it already validated, and
  batches no longer clone `typed_features` and `model` or keep their inputs while
  validating. A 256,000 entity drawing went from 3.5 GB to 1.9 GB peak.

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
