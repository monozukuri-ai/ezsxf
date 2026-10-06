# SAF attributes, images and bundle delivery

## Saving existing bundles

```python
import ezsxf

report = ezsxf.write_sfc_bundle(
    "source/drawing.sfc", "delivery", file_name="renamed.sfc",
    extra_files=["custom-attachment.bin"],  # optional vendor-specific dependencies
)
print(report["files"])
```

The destination must be a **new directory**. The operation validates the drawing,
SAF root metadata and matching ATRF/Figure IDs, gathers local image filenames
from SAF `Attr` names `画像`/`ファイル名` and inline ATRU attachments, then copies
dependencies unchanged. It also copies a declared local SYSTEM DTD. Missing,
ambiguous, unsafe or symlink dependencies fail before publishing the directory.
Images and copied DTDs are opaque bytes; this is not image decoding or full SAF
DTD/schema validation. Unknown vendor attachment references must be supplied
through `extra_files`.

An explicit `file_name` updates the header, ATRF definitions/placements and SAF
`sxfFile`, and renames the SAF to the new drawing stem. Other XML spelling and
opaque attributes are retained; UTF-8 (including BOM) and Shift-JIS/CP932 SAFs
are supported. Without a rename, drawing/SAF filenames and records are retained
(the header filename is aligned with the physical source basename). Dependencies
must be regular files in the source directory; path references, multiple SAF
files, PUBLIC/internal DTD declarations and existing destinations are rejected.
Validated files are staged next to the destination and published together using
an exclusive directory rename on Linux, macOS or Windows. This prevents partial
deliveries after ordinary errors; it does not promise power-loss durability.

## Creating and editing attributes and images

```python
doc = ezsxf.new_sfc("drawing.sfc")
circle = doc.add_circle((50, 50), 10)
doc.set_attribute(circle, "Material", "Steel")
doc.set_attribute(circle, "Height", "12.5", attribute_type="LEN", unit="m")
image = doc.add_image("scan.tif", (80, 40), 100, 60, angle=15)
doc.save_bundle("delivery", file_name="drawing.sfc")

edited = ezsxf.edit_sfc_bundle("delivery/drawing.sfc")
edited.set_attribute(circle, "Material", "Concrete")
edited.remove_attribute(circle, "Height")
edited.update_image(image, (90, 45), 100, 60, image="replacement.tif")
edited.save_bundle("revised", file_name="revised.sfc")
```

`set_attribute` creates an ATRF wrapper or updates its loaded SAF. It returns
the stable figure identifier and keeps the target entity ID. Attribute identity
is its set metadata (`set_name`, `set_version`, `designed_by`), `group` path
(at most two levels), and `name`. Repeated names at that address are ambiguous
and cannot be updated by name. `get_attributes` returns the values and metadata.
Removing the final attribute unwraps the element; `remove_attachment` removes
all its attributes, and `remove_element` removes the wrapper and target together.
Elements referenced by a SAF `ターゲット` attribute cannot be detached.

Use `edit_sfc_bundle` to load an existing SFC, SAF and file dependencies before
editing SAF data. This API validates the supported SAF grammar, rejects unknown
XML fields, and upgrades legacy 3.0 inline `AttributeSet` data to 3.1 root
definitions and `AttrSetRef` children. Authored SAFs use UTF-8 and normalize XML
formatting, comments and encoding. `write_sfc_bundle` remains the byte-preserving
route for unedited vendor XML. `saf_bytes` returns the authored SAF (or `None`);
`validate_saf(bytes)` checks the 3.1 DTD structure, required fields, unique set
metadata/IDs, figure IDs, set references and group depth. External DTD contents
and custom vendor schemas are not evaluated; type/unit/value interpretation
remains the caller's responsibility.

`add_dependency(path, file_name=...)` supplies files referenced by `ファイル名`
attributes. `save_bundle` checks dependencies and collisions before publishing
a new directory; SFC header, ATRF names and SAF `sxfFile` are renamed together.
Unused dependencies are omitted. Loading captures dependency bytes, so later
changes to source files do not affect the edited document.

`add_image` and `update_image` accept TIFF/JPEG files and generate the standard
image attribute and a clockwise closed rectangle. New images use the SXF 3.1
common-set ATRU mechanism and need no SAF of their own. Existing SXF 3.0
SAF/ATRF images retain their mechanism when edited. Inline image metadata is
available in `to_dict()["model"]["attribute_attachments"]`;
`get_attributes` reads SAF attributes.
The anchor is the lower-left corner; dimensions are sheet millimetres and angles
are degrees. Computed corners are rounded to the SFC six-decimal coordinate
limit. TIFF metadata must describe one page of G4 strips, monochrome 1-bit
pixels, normal orientation and dimensions no greater than 13,000 pixels.
JPEG frame/segment headers are checked. Pixel decoding and image conversion are
not performed, and supported headers alone do not establish valid pixel data.
Image geometry updates must retain a clockwise rectangle.

`set_single_attribute` authors ATRU names and `set_text_attribute` authors ATRS
names (text targets only); use prescribed common attribute names/types for these
mechanisms. Arbitrary attribute sets should use SAF. All attachment APIs
currently target the same five basic element types; grouped elements are not
editable. Failed operations keep both SFC and SAF state unchanged.

See [compatibility](compatibility.md) before relying on third-party CAD re-export.
