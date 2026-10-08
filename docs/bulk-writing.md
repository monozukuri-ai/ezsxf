# Bulk authoring and P21 preflight

`SfcDocument.extend` accepts leaf features and structured dictionaries in one
atomic transaction. It validates each input fragment, clones the document once,
then strictly validates the complete candidate once. Create style codes before
calling it. Final serialization still validates the complete document.

```python
import ezsxf

doc = ezsxf.new_sfc(paper="A1", target="p21")
ids = doc.extend([
    {"kind": "fill", "color": 2,
     "outer": [(0, 0), (40, 0), (40, 30), (0, 30)],
     "holes": [[(10, 10), (20, 10), (20, 20), (10, 20)]]},
    {"kind": "hatch", "outer": [(50, 0), (90, 0), (90, 30), (50, 30)],
     "patterns": [[1, 1, 1, 0, 0, 3, 45]]},
    {"kind": "part", "name": "bolt",
     "elements": [{"kind": "circle", "center": (0, 0), "radius": 3}],
     "placements": [{"position": (100, 20)}, {"position": (120, 20)}]},
    {"kind": "partial_drawing", "name": "detail", "scale": (0.02, 0.02),
     "elements": [{"kind": "line", "start": (0, 0), "end": (500, 0)}]},
])
# ids[0:2]: fill/hatch IDs; ids[2]: two placement IDs; ids[3]: placement ID.
doc.extend([{"kind": "text", "text": "detail", "anchor": (0, 50)}], into=ids[3])
assert doc.validate_p21() == []
data = doc.to_p21_bytes()
drawing = ezsxf.build_drawing(data)
```

## Structured dictionaries

| Kind | Fields and return value |
| --- | --- |
| `fill` | `outer`, optional `holes`, `layer=1`, `color=1`; returns fill ID |
| `hatch` | `outer`, `patterns`, optional `holes`, `layer=1`; returns hatch ID |
| `composite_curve` | `elements` (boundary curves), `visible=False`, `color=1`, `line_type=1`, `line_width=1`; returns boundary ID |
| `part` | `name`, `elements`, nonempty `placements`; returns placement IDs |
| `partial_drawing` | `name`, `elements`, optional placement fields; returns placement ID |
| `group` | `name`, `elements`, identity placement fields; returns placement ID |
| `placement` / `part_placement` | `name` of an already defined part, optional placement fields; returns placement ID |

A fill/hatch boundary is a coordinate polygon or a list of `polyline`, `arc`,
`ellipse_arc` or `spline` leaf dictionaries. Coordinate polygons are closed
automatically. Curve lists must already form a connected, directed closed loop.
Holes remain separate from the outer boundary. Empty interiors, including
collinear zero-area boundaries, are rejected with the candidate fill/hatch ID.
This also applies to individual `add_fill` and `add_hatch` calls. Complete
self-intersection, hole containment and winding certification is not performed.
`composite_curve` creates a reusable boundary definition; use its returned ID
with `add_fill`/`add_hatch`. Its visibility controls the boundary when used by a
fill, rather than creating a separate sheet placement.

Placement fields are `position=(0,0)`, `angle=0`, `scale=(1,1)` and `layer=1`.
Every entry in a part's `placements` requires an explicit `position`; no extra
origin instance is created. Positive unequal X/Y scales are supported. Zero,
negative and nonfinite scales fail directly; mirror the definition geometry
before creating an SXF placement. Groups require the identity transform.

Nested parts/groups and references to earlier part names are allowed, up to
32 levels. Names must be unique; recursive and forward references are rejected.
Partial drawings are top-level only. A failed outer input discards its entire
fragment, including nested definitions and placements.

`into` accepts an existing ordinary definition ID or its placement ID. It
inserts leaf features and existing part placements directly into that definition,
without first adding them to the sheet. It cannot create new nested definitions
or boundaries; use an inline `partial_drawing`, `group` or `part` for those.
Existing placement references must still follow their definitions in SFC order.

## Invalid inputs

The default `on_invalid="raise"` raises an error prefixed with the zero-based
outer input index, such as `Element 3: Entity #19: circle radius ...`, and leaves
the document unchanged. For a partly valid batch:

```python
ids, rejected = doc.extend([
    {"kind": "line", "start": (0, 0), "end": (10, 0)},
    {"kind": "circle", "radius": -1},
], on_invalid="skip")
assert ids[1] is None
assert rejected[0][0] == 1  # (input index, reason)
```

Skip mode commits all accepted fragments together. Generator failures and final
document validation failures abort the entire transaction. IDs are allocated
only for accepted fragments; callers must use returned IDs. Neither mode exposes
unchecked document state.

## P21 constraints and explicit loss

`new_sfc(target="p21")` rejects unsupported P21 features and P21-only geometry
constraints during insertion/editing. The default `target="sfc"` retains SFC
capabilities. `validate_p21()` returns `(source_entity_id, reason)` pairs without
changing the document. P21 geometry failures include the source ID, including
zero-length lines and fills with no interior point. Packaging, SAF and image
dependencies still require the separate bundle validation at export.

Point markers 1–7 and cubic splines with `3n+1` control points (`n >= 1`) have
native AP202 output. Splines use cubic `BEZIER_CURVE` segments and
`COMPOSITE_CURVE`, retaining control points and open/closed state. They are not
converted to polylines. Drawing previews sample curves just as for SFC.

```python
payload, dropped = doc.to_p21_bytes(unsupported="drop", report=True)
# dropped: [(source_entity_id, reason), ...], in source order
```

The default remains `unsupported="raise"`. Explicit `drop` omits clothoids,
geodetic partial drawings (kind 2), unsupported spline control layouts and
dependent content. A dropped geodetic definition removes its owned geometry
and placements. Broken composite boundaries/fills, attribute wrappers and empty
figures are pruned; the report includes those dependent IDs. Inspect the report
before delivery. Invalid supported geometry still raises an error. The source
document remains unchanged, so it can also produce SFC.

Drop/report options are available on standalone `to_p21_bytes` only. Save,
bundle and P2Z APIs remain strict. Standalone export still rejects external
SAF/images. No approximate mode is provided because the supported cubic layouts
are emitted exactly. See [P21 delivery and CAD limits](p21-writing.md).

## Reproducible acceptance workloads

```bash
maturin develop --release
python scripts/verify_writer_requests_v2.py --output /tmp/ezsxf-bulk-review
```

The script records a 50,000-plus-feature batch with 1,000 fills and 200 shared
parts (50 members and five placements each), and a 20,000-input batch with 50
invalid inputs. It separately measures only `extend`, verifies zero strict
reparse warnings, records point sharing and P21/P2Z sizes, and prepares paired
marker/spline controls for native CAD review. The two-second and 0.4-second
thresholds are reported for the local machine, rather than enforced in CI.
New point/spline mappings have local round-trip coverage; native Windows CAD
verification of these additions remains pending.
