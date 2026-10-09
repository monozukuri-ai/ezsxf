# Reading P21/SFC and parse results

```python
import ezsxf

parsed = ezsxf.parse_sfc("drawing.sfc", strict=True)
print(parsed["format"])
print(len(parsed["typed_features"]))
print(parsed["model"]["sheet"]["component_ids"])

p21 = ezsxf.parse_p21("drawing.p21", strict=True)
print(len(p21["entities"]))
```

Both parsers accept a path string, file contents as a string, or bytes. For a
`pathlib.Path`, pass `str(path)` or `path.read_bytes()`. SFC input is decoded as
UTF-8 or Windows Shift-JIS/CP932. Strict decoding rejects invalid byte sequences;
lenient recovery can report warnings instead.

## Result structure

| Key | Meaning |
| --- | --- |
| `format` | `"sfc"` or `"p21"`. |
| `header` | File description, filename/schema metadata and their original records. |
| `entities` | Ordered source records with entity IDs and parameters. |
| `typed_features` | Supported SFC records, or supported P21 sheet/subfigure metadata, decoded into named fields. |
| `model` | Resolved sheet and figure structure with common SFC/P21 keys. SFC also resolves style tables and composite curves. `None` when P21 has no recognized structural features. |
| `warnings` | Recovery, unsupported or incomplete-feature diagnostics. |

The SFC reader covers all 34 feature types in SXF Ver.3.1. P21 reading retains
the generic Part 21 entity representation and resolves the structural metadata
described below. P21 geometry continues to use the STEP graph for drawing
conversion; its individual geometric records are not mapped to typed SFC features.

## Common P21 sheet and subfigure metadata

`parse_p21()` returns `drawing_sheet`, `sfig_org` and `sfig_locate` features with
the same field names as SFC. `model.sheet`, `model.sfig_definitions` and
`model.sfig_references` link them by the original P21 entity IDs. Typed feature
`keyword` values use the corresponding SFC feature names; `entities` retains
the original AP202 keywords, parameters and STEP string escapes.

```python
parsed = ezsxf.parse_p21("drawing.p21")
by_id = {item["id"]: item for item in parsed["typed_features"]}
if parsed["model"] and parsed["model"]["sheet"]:
    sheet = by_id[parsed["model"]["sheet"]["entity_id"]]
    print(sheet["name"], sheet["sheet_type"], sheet["orientation"])
    for ref in parsed["model"]["sfig_references"]:
        placement = by_id[ref["placement_id"]]
        print(placement["name"], placement["position"], placement["ratio_x"])
```

Paper codes are `0..4` for A0..A4 and `9` for FREE. Standard sheet names such as
`A3_horizontal` supply paper/orientation metadata. `PLANAR_BOX`, directly on the
sheet or through `PRESENTATION_SIZE`, supplies dimensions in the SXF millimetre
convention; FREE dimensions retain fractional values. Without a standard name,
paper and orientation are inferred from the box dimensions. Drawing titles are
resolved through the sheet's drawing revision. Typed names and attribute values
decode STEP `X2`/`X4` Unicode escapes.

Subfigure prefixes `$$SXF_FM_`, `$$SXF_FG_`, `$$SXF_G_` and `$$SXF_P_` map to
kinds 1, 2, 3 and 4. Symbol maps resolve source origins/axes and target positions,
rotation and X/Y scale. Placements inside definitions and multiple part instances
retain separate references. Component IDs exclude paper boxes and placement axes;
they include callout children and styled targets so membership traversal can find
the renderer's source IDs. ATRF/ATRU/ATRS wrappers populate `attribute_attachments`.
P21 code tables, composite-curve/hatch metadata and external SAF filename resolution
are outside this structural extraction and remain empty/unresolved.

Invalid or sheared mappings are omitted from the common placement model with a
warning; the original graph remains available for drawing conversion. Generic
unprefixed AP202 subfigures are retained only in `entities`. For multiple sheets,
the common model and drawing conversion use the first sheet and report a warning.

## Strict and lenient modes

Strict parsing validates required structure and references. Some incomplete
feature records can still be skipped with warnings, so inspect `warnings` even
when `strict=True`. Lenient mode enables additional recovery of damaged input:

```python
recovered = ezsxf.parse_sfc("damaged.sfc", strict=False)
for warning in recovered["warnings"]:
    print(warning)
```

Recovery is intended for inspection. [SFC saving](sfc-writing.md) requires a
complete, consistent parse without warnings; it does not turn a partial parse
into a complete drawing.

## Editing and drawing conversion

Use `ezsxf.edit_sfc(parsed)` to edit the original SFC records. `to_dict()` returns
an independent parser-compatible snapshot. Direct dictionary edits must leave
the source records and derived typed/model views consistent to be writable.

[Drawing conversion](conversion.md) produces flattened rendering primitives.
That representation expands groups and dimensions and is separate from the
editable SFC model. Use the SFC editor for structural preservation, and inspect
`Drawing.warnings` when evaluating conversion coverage.
