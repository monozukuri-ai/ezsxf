# Saving existing SFC drawings

```python
import ezsxf

parsed = ezsxf.parse_sfc("drawing.sfc", strict=True)
sfc_bytes = ezsxf.serialize_sfc(parsed)
ezsxf.write_sfc(parsed, "copy.sfc")
```

This API resaves an existing complete SFC parse result. It preserves entity IDs,
record order, parameter values, the typed features, hierarchy, style codes and
attribute attachments. Output uses Shift-JIS/CP932 and CRLF line endings.
Comments, whitespace, original encoding and quote spelling are not preserved.
The header is retained, including `FILE_NAME` and the original timestamp, even
when writing to a different path. New drawing construction and P21 output are
not supported by these functions.

`entities` and `header.entities` are the serialization source. Before returning
bytes or touching the destination, the writer checks field syntax and precision,
reparses the output without warnings and verifies the derived `typed_features`,
`model` and header aliases. It rejects inconsistent dictionary edits, skipped
features, invalid references/order, unencodable or changed characters, and
excess precision (lengths beyond six fractional digits or angles/scales beyond
15 digits excluding the sign and decimal point). It does not round values or
silently repair input.
`write_sfc` writes a temporary file beside the destination and replaces the
destination after validation and a successful write. Existing regular-file
permissions are retained; symlink destinations are rejected.

Drawings with external SAF references require explicit opt-in:

```python
ezsxf.write_sfc(parsed, "copy.sfc", allow_external_references=True)
```

This preserves the references only: it does not read, validate, rename or copy
SAF or image files. Arrange the referenced files separately, retaining their
names and directory relationships. In particular, when the SAF name is omitted,
keep the drawing basename consistent with the retained `FILE_NAME` and SAF
basename. Use the default to reject such drawings until those dependencies are
handled. Inline ATRU/ATRS attachments do not require this opt-in.

For new drawings and edits, see [basic SFC creation and editing](sfc-writer-mvp.md).
For delivery with dependencies, see [SAF and image bundles](sfc-bundles.md).
For CAD-specific limits, see [compatibility](compatibility.md).

Representation follows SXF Ver.3.1 SFC specification §§1-1-1 and 1-1-2
(body pp.2–6); ordering and relationships follow §1-3 (body pp.99–103).
