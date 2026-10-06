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
| `typed_features` | Supported SFC records decoded into named fields. |
| `model` | Resolved SFC sheet, code tables, figures, composite curves and attribute attachments; `None` for generic P21 data. |
| `warnings` | Recovery, unsupported or incomplete-feature diagnostics. |

The SFC reader covers all 34 feature types in SXF Ver.3.1. P21 reading exposes
the generic Part 21 entity representation; it does not build the same typed SFC
model or implement every AP202 semantic rule.

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
