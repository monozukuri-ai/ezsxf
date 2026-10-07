# Compatibility and limits

ezsxf validates its SFC output by strict reparse and model comparison. This
establishes preservation within the supported parser/editor contract. CAD
import, display and re-export are separate operations and may change the data.
The package does not claim OCF certification or complete AP202 conformance.

## Strings and locales

SFC output must be losslessly representable in CP932. Unsupported characters
raise an error rather than being replaced. Font installation and glyph shape
depend on the CAD application.

Standard SFC quoting writes a semantic backslash as two backslashes. Some CAD
applications display both characters, or display a yen glyph. ezsxf strict
reload returns the original single backslash. The optional
`literal_backslashes=True` mode emits one byte where unambiguous; it rejects
unsafe quote/backslash combinations. Its CAD behavior must be checked for the
target application; standard quoting remains the default.

Recorded Japanese Windows CAD results used both Japanese system and user
regional cultures with ANSI code page 932. An external Japanese application
manifest alone did not establish correct Japanese save/reopen behavior.

## Recorded CAD observations

These are bounded observations from the 2026-10-06 and 2026-10-07 qualification inputs, not
promises for arbitrary files or newer application releases. Viewer display
does not establish authoring/export support.

| Application and profile | Observed scope and limit |
| --- | --- |
| Jw_cad 10.03.6, native Windows with Japanese system/user cultures | Tested basic Japanese drawings retained text and resolved geometry/styles through save, overwrite and reopen. The tested group/part SFC exports failed with `30002: SFIG_LOCATE`. |
| Jw_cad 10.03.6, Wine with Japanese code page and font substitutes | The basic writer input displayed five primitive kinds, predefined/custom styles and Japanese text. Standard backslashes displayed doubled yen glyphs; the literal variant displayed one. This is visual import evidence for that input. |
| Jw_cad 8.25a, native Japanese Windows profile | Tested group hierarchy, shared-part reuse, geometry and placement survived export, but group names and sheet metadata changed. SAF/image attachment names were rewritten, losing attribute and raster bindings. Complete source-to-export equality failed. |
| Jw_cad 8.25a, 2026-10-07 native Windows, ja-JP system/ANSI 932 and en-US user culture | Twelve generated SFC/P21 inputs retained basic geometry, all nine horizontal text anchors, shared parts and 1/1–1/100 partial-drawing placements through JWW save/reopen. Six SFC/P21 pairs matched independently. Layer-group scale stayed 1; the partial-drawing ratios remained on blocks. This qualifies the listed imports/JWW storage, not native SFC re-export or OCF certification. |
| Jw_cad 8.25a, expanded P21 writer on the same native Windows profile | The tested linear/angular/radius/diameter dimensions, leader and balloon matched SFC geometry, text and terminators through JWW save/reopen. Arc-length dimensions were omitted in both formats. |
| DynaCAD Viewer 9.0, tested Japanese Windows/Wine profiles | Tested SAF values and actual TIFF/JPEG image patterns displayed, including edited placements. This qualifies viewing of those inputs, not CAD re-export. |
| DynaCAD Viewer 9.0, expanded P21 writer on native Windows, 2026-10-07 | All five tested P21 dimensions/leaders and solid/two-pattern fills with holes displayed. SAF's Japanese grouped values matched the source SFC attribute dialog. P21/P2Z-extracted TIFF/JPEG viewports matched SFC pixels; native .NET extraction retained all member bytes. Direct P2Z opening is not qualified. |
| VoiCeFREE 3.5.5.3, tested Japanese Wine profile | Japanese text displayed; the tested ATRU image cases showed empty frame outlines. SAF/raster-preserving re-export is not established. |

The basic writer's native Windows build/filesystem saving is verified separately
from the CAD observations. A fresh native Windows CAD review of the 0.2.0 custom
style input has not been completed. [Basic writing](sfc-writer-mvp.md),
[complex editing](sfc-editing.md) and [SAF/image bundles](sfc-bundles.md) describe
the library's own guarantees. See [P21 writing](p21-writing.md) for the later
bounded P21 and native-scale qualification.

## Checking a target CAD's re-export

Use a representative drawing with its original SAF and image files present.
Open it in the target CAD, export into a new directory, close and reopen the
exported drawing, and inspect groups, attribute values and actual raster pixels.
Export again into another new directory. Record the CAD version, locale,
settings and warnings; do not repair the export before evaluating it.

The repository includes a bounded, read-only comparator:

```bash
python scripts/verify_sfc_reexport.py source/drawing.sfc exported/drawing.sfc --report new-report.json
```

The report path must be new. Exit status 1 indicates changed/lost data or an
unsupported comparison. Only `passed: true` satisfies the complete data gate.
Run it for both source/first export and first/second export.

The comparator checks basic geometry, group hierarchy/kind/names, shared-part
reuse, resolved styles, SAF metadata/values/bindings and referenced TIFF/JPEG
bytes. Entity IDs, code-table indices and dependency basenames may change if
their resolved content and bindings are retained. Header timestamps/application
metadata and sheet title are excluded; other sheet parameters remain checked.
Numeric tolerance is absolute/relative 1e-9.

Composite curves, hatches and other unsupported elements fail this comparison.
Image recompression fails exact byte comparison and requires a separate decoded
pixel/visual review. Custom schemas and vendor references also need independent
review. Comparing local copies alone does not prove that a CAD opened, displayed
or re-exported them.
