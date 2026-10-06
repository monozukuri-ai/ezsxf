# Complex SFC element editing

The editor works on the original SFC records and ownership model. It validates
each complete candidate in Rust before replacing the document state. Failed
operations retain the original SFC/SAF state. IDs and style-table codes remain
stable; grouping deliberately changes record order and container membership.
This API does not imply preservation by an external CAD application.

## Geometry and dimensions

```python
import ezsxf

doc = ezsxf.new_sfc("edited.sfc")
ellipse = doc.add_feature("ellipse", center_x=50, center_y=40,
                          radius_x=20, radius_y=10, angle=30)
doc.update_element(ellipse, radius_y=12)
spline = doc.add_feature("spline", points=[(0, 0), (10, 20), (30, 20), (40, 0)])
dimension = doc.add_feature("linear_dimension", start_x=0, start_y=30,
                            end_x=40, end_y=30, text="40", text_x=15, text_y=35,
                            arrow1_code=1, arrow1_x=0, arrow1_y=30,
                            arrow2_code=1, arrow2_x=40, arrow2_y=30)
doc.update_element(dimension, text="40 mm", text_width=20)
doc.save("edited.sfc")
```

`add_feature(kind, **fields)` returns an entity ID. It accepts the following
named feature kinds, or their corresponding SXF `*_feature` keywords. Unknown
fields and kinds are rejected. `update_element` accepts the same fields for
existing supported geometry, including elements in groups/parts and composite
curves. Coordinates remain local to their owning definition; changing a shared
part's child affects every placement of that part.

| Kind | Geometry fields |
| --- | --- |
| `point_marker` | `x`, `y`, `marker`, `angle`, `scale` |
| `ellipse` | `center_x`, `center_y`, `radius_x`, `radius_y`, `angle` |
| `ellipse_arc` | ellipse fields, `direction`, `start_angle`, `end_angle` |
| `spline` | `points`, `open_close` (0 closed, 1 open) |
| `clothoid` | `base_x`, `base_y`, `parameter`, `direction`, `angle`, `start_length`, `end_length` |
| `linear_dimension`, `radius_dimension`, `diameter_dimension` | `start_x`, `start_y`, `end_x`, `end_y` |
| `angular_dimension`, `curve_dimension` | `center_x`, `center_y`, `radius`, `start_angle`, `end_angle` |
| `label` | `vertices`, `arrow_code`, `arrow_scale` |
| `balloon` | label fields, `center_x`, `center_y`, `radius` |

All accept `layer`/`color`; all except point markers also accept `line_type` and
`line_width`. Numeric fields require finite integers/floats, not numeric strings
or booleans. Length precision is limited to six fractional digits without
implicit rounding. Angles are degrees. Spline points are SXF control points,
not points interpolated by a newly chosen spline algorithm.

Dimensions and leaders also accept `text_present`, `font`, `text`, `text_x`,
`text_y`, `text_height`, `text_width`, `text_spacing`, `text_angle`, `text_slant`,
`text_base_point`, and `text_direction`. `text_width` is the complete text-box
width. A supplied `text` enables its display on creation; updating `text` retains
the existing `text_present` flag unless explicitly changed.

Dimension arrows use `arrow1_code`, `arrow1_direction`, `arrow1_x`, `arrow1_y`,
`arrow1_scale`, and the corresponding `arrow2_*` fields. Radius dimensions have
only arrow 1. Linear/angular/curve dimensions also accept `extension1_present`,
`extension1_base_x`, `extension1_base_y`, `extension1_start_x`,
`extension1_start_y`, `extension1_end_x`, `extension1_end_y`, and the matching
`extension2_*` fields. Arrow/extension positions are explicit: the editor does
not automatically compute dimension layouts or measure text.

Defaults are style/font code 1, radius/ellipse radii/clothoid parameter 10,
text height/width 3.5, marker/leader-arrow code 1 and scale 1. Text is absent
unless supplied; dimension arrows/extensions are absent by default. Other
numeric values default to zero, except `open_close`, `text_base_point` and
`text_direction`, which default to 1. Specify the intended geometry and text
placement explicitly. Coordinate aggregates are edited through `points` or
`vertices`, rather than count/array fields.

## Groups, partial drawings and reusable parts

```python
line = doc.add_line((0, 0), (20, 10))
group = doc.group_elements("Assembly", [line, ellipse])
doc.rename_group(group, "Revised assembly")
extra = doc.add_circle((30, 20), 5)
doc.add_to_group(group, [extra])
doc.update_element(line, end_x=25)  # definition coordinates
doc.ungroup(group)                # retains each child's ID

part = doc.group_elements("Symbol", [line, ellipse], kind=4, layer=1)
copy = doc.place_part(part, position=(80, 40), angle=15, scale=(2, 1))
doc.update_element(copy, x=90, angle=30)
```

`group_elements` consumes nonempty, unique component IDs directly on the sheet,
preserves their original relative order and returns the placement ID. Their
definition is emitted before its references without capturing other geometry.
Existing group placements can be grouped to create nested groups. Reserved
attribute-wrapper names/placements cannot be regrouped by these methods.

Kinds are 1 mathematical partial drawing, 2 geodetic partial drawing, 3 drawing
group (default), and 4 drawing part. Kind 3 must retain the identity transform:
position `(0,0)`, angle 0, scale `(1,1)`. Parts can have additional placements
using `place_part`; the other kinds permit only one. `rename_group` updates the
definition and every placement together. `add_to_group` moves sheet components
into an existing definition; illegal parent relationships and forward references
are rejected. It does not apply a coordinate transformation to moved geometry.

`ungroup` unwraps only identity drawing groups, including nested ones. It does
not flatten transformed parts or partial drawings. Deleting a part placement
with `remove_element` succeeds when other placements keep its definition in
use. Deleting its final placement is rejected rather than silently deleting the
definition and contents. Code-table declarations and sheet metadata remain
outside `update_element`/`remove_element`.

## Fill and user-defined hatch boundaries

```python
edge = doc.add_polyline([(0, 0), (40, 0), (40, 30), (0, 30), (0, 0)])
outer = doc.add_composite_curve([edge])
hole_edge = doc.add_polyline([(10, 10), (20, 10), (20, 20), (10, 20), (10, 10)])
hole = doc.add_composite_curve([hole_edge])
fill = doc.add_fill(outer, holes=[hole], color=2)
hatch = doc.add_hatch(outer, [[1, 1, 1, 0, 0, 3, 45]], holes=[hole])
doc.update_hatch_patterns(hatch, [[1, 1, 1, 0, 0, 4, 30]])
doc.update_hatch_boundaries(fill, outer)
doc.update_hatch_boundaries(hatch, outer)
doc.release_composite_curve(hole)  # valid now that no hatch references it
```

`add_composite_curve` consumes sheet arc/ellipse-arc/polyline/spline IDs and
returns the boundary's **entity ID**. Fill/hatch APIs resolve these IDs to the
order-dependent SXF curve codes internally. Boundary codes are appended without
renumbering earlier boundaries. Holes and the outer boundary must be distinct.
Use closed contours with the intended orientation; the API does not perform
geometric self-intersection, containment or overlap analysis.

Each hatch pattern contains `[color, line_type, line_width, x, y, spacing, angle]`.
The first three fields must be integers; one to four patterns are supported.
`update_hatch_patterns` retains references to the existing boundaries.
`update_element` edits a fill's layer/color and a hatch's layer. Use
`update_hatch_boundaries` to replace boundary references.

`release_composite_curve` requires an unreferenced boundary and returns its
children to the sheet, retaining IDs. Direct deletion of a composite definition
is rejected because it would change ownership or renumber later curve codes.
Release updates later hatch codes by their resolved boundary IDs, preserving
their targets when the declaration order changes. Predefined/tile hatch creation and geometric
validation of regions remain outside this API; existing such records are
preserved by resaving.

SAF attachment/image operations retain their documented direct-basic-element
scope. Grouped attribute authoring, automatic dimension layout, arbitrary
container reparenting and P21 generation are not provided.
