# SFC writer qualification

Updated 2026-10-06. Model round trips, third-party CAD ingestion and native
Windows filesystem behavior are separate acceptance checks.

| Check | Evidence and boundary |
| --- | --- |
| Complete SFC model round trip | Authored fixture covers all 34 feature types, nested figures, hatch holes, style tables and ATRF/ATRU/ATRS. Writer rejects warnings, omitted entities, inconsistent caches and excess precision. |
| Real-file model round trip | Linux and native Windows: 20 SFCs, 46,305 entities/features. New saves and overwrites reparse to the original model; Windows output bytes match Linux output. Larger sample corpus is optional and not shipped in the package. |
| External CAD ingestion | A fresh Jw_cad 10.02.1 / Wine 9.0 run opened original and ezsxf-resaved D0LS004Z drawings. JWW entities, settings, blocks, names, counts and diagnostics agree. The remaining header differences are in unused SXF line-type slot 30. |
| All-feature CAD fixture | The corrected 52-record / 34-feature fixture and its ezsxf serialization now produce equal JWW entities, settings, blocks, names, counts and diagnostics. This establishes equal CAD ingestion, including the difficult text, within Jw_cad's import behavior; it does not establish preservation of every SXF feature's meaning. |
| CAD-to-SFC save/reopen | Jw_cad 10.03.6 on native Windows and 10.02.1 on Wine passed basic-element Save As, close/reopen, confirmed same-name replacement and a second close/reopen. Native Windows also passed a Japanese/special-character data case. First-save/replacement SFC entities, typed features and structure agree exactly without reader warnings; independent JWW geometry/style snapshots agree. Complex/attribute-figure exports fail on native Windows too. Text/raster display is qualified separately. |
| SAF bundle and authoring | Tests cover attribute/group creation, update/removal, matching figure/set IDs, required XML fields, legacy 3.0 normalization, renamed physical/header/SAF references, unchanged dependencies, failed-edit rollback and exclusive directory publication. Owned 3.1 SAFs also pass the bundled DTD through xmllint. DTD contents, custom schemas, type/unit/value domains and external CAD attachment display remain separate. |
| CAD SAF/image check | DynaCAD Viewer 9.0 on Wine displays owned SAF attribute values and actual ATRU TIFF/JPEG pixels, including edited positions, dimensions and rotations. This is bounded visual evidence. The legacy D0PL001Z bundle still raises a SAF-related warning. Jw_cad and VoiCeFREE display empty ATRU outlines in the owned image case. |
| Real-file bundle | On Linux and native Windows, 19 of 20 local drawings delivered successfully, including 6 with actual SAF/image dependencies. All bundle file hashes match between platforms; 7 image dependencies were independently compared byte-for-byte with their sources. D4GV001Z was rejected because D4GV0011.TIF is missing from its SFC directory; no output directory was published for that case. |
| Native Windows MSVC build and saving | GitHub Windows Server 2025 build 26100, native `x86_64-pc-windows-msvc`, CPython 3.13.15: 52 Rust tests, 44 Python tests passed (3 skipped), all 8 platform save checks. Native build, strict clippy, fmt, package, installed-wheel import/CLI and SAF/image authoring checks passed. The earlier Windows 11/cloud run remains separate cross-compiled/corpus evidence. |
| Real SAF/image editing | Linux: all 6 available real SAF bundles load, normalize and save; only explicit SAF filename spellings change in the SFC model. Moving/resizing/rotating all 7 image placements and reloading preserves their edited geometry and attributes, while retaining the original image bytes. Missing D4GV0011.TIF is rejected. This is data/API evidence, not a third-party raster display pass. |

The CAD run used Jw_win.exe SHA-256
`95e6b11c4ee014e0079f288429ae2c6e5eed41141a963e4b8770d2e8ead87acf`.
Private runtime copies, screenshots, logs and comparison JSON stay under
`.local/internal/`; no vendor binaries or third-party drawings are distributed.
Jw_cad can decompose dimensions, hatches and other complex structures when
importing; equality of its imported model does not prove preservation of SXF
editable structure or OCF conformance. Japanese text values and layer names
passed the new Wine data checks. Some Wine menu/error captions still render as
boxes; this run does not qualify native Windows CAD display.

## Display qualification on 2026-10-06

The later compatibility follow-up below supersedes the unresolved SAF warning
and the manifest-only save result in this original qualification table.

`scripts/prepare_sfc_display.py` creates owned inputs and a separate
`expectations.json`: five text rows, a circle with Japanese SAF properties, a
Group 4 TIFF with an L and checkerboard, and a JPEG with red/green above
blue/yellow. A second bundle moves, resizes and rotates the images by +15 and
-15 degrees. Pillow is needed only for preparing these verification inputs.
Each SFC passes strict parsing and model serialization/reparse before review.
Input hashes remain unchanged after the viewer runs.

| Application and environment | Observed result |
| --- | --- |
| DynaCAD Viewer 9.0, Wine 9.0, task-owned Japanese prefix with IPA/Noto font substitutes | The SAF inspector displays group `設計`, `材料 = 鉄 & 鋼`, and Japanese/special-character notes. Both ATRU images show their actual patterns and orientation. Edited placements preserve patterns, dimensions and opposite rotations. The black CAD background displays TIFF foreground ink in white. Five text rows, including CP932 characters `ソ 予 表 申 能`, display correctly. A semantic single SFC backslash displays as two yen signs; the SAF note displays one yen sign. Some menu captions remain boxes, so this is not qualification of the whole localized UI or native Windows. |
| VoiCeFREE 3.5.5.3, same Wine prefix | Five Japanese/ASCII rows display, including the CP932 characters above. SFC backslashes display as doubled yen signs. ATRU images show empty outline rectangles; neither image's expected pattern is present. SAF inspector behavior is not qualified. |
| Jw_cad 8.25a, native Windows Server 2025 build 26100, external `ja-JP` manifest experiment | Japanese glyphs and all five corrected text rows display. Basic/quoted Save As, replacement and reopen retain equal strict SFC models and independent JWW geometry/styles; the quoted canvas is unchanged pixel-for-pixel. Backslashes remain doubled, with backslash or yen glyphs depending on the font. The five-row save workflow times out, so it is not a five-row save/reopen pass. |
| Jw_cad 10.03.6, same native Windows manifest experiment | All five corrected rows import, but Japanese displays as mojibake. Save/reopen changes Japanese contents, font names and widths: 9 resolved JWW differences in the five-row case. The combined Japanese/backslash/apostrophe output also fails strict SFC parsing. Basic/quoted snapshots have 3 differences each. Raster patterns are absent and SAF/image exports fail. These are compatibility failures even though the workflow's minimum basic-operation gate passes. |

The initial text card created duplicate font definitions. DynaCAD reported
`TEXT_FONT 20024` and `TEXT 30007`, and Jw_cad imported only one of five rows.
`SfcDocument.add_font()` now returns an existing code for an exact-name match,
including the default Japanese font; it appends only new names. It preserves
existing codes and leaves imported documents unchanged on a reused name.
The Rust regression covers repeat calls without mutation and subsequent new
font allocation. Corrected files contain two font definitions and all five rows
display in the three readable application/environment combinations above.

The SFC specification, appendix SFC second edition, section 1-1-3(3), printed
page 3 / PDF page 9, requires double-backslash spelling inside strings and
explains font-dependent backslash/yen glyphs. The writer keeps that spelling.
The repeated glyphs above are recorded as a display limitation; no alternate
escaping or silent text transformation is introduced to accommodate a viewer.

The native CAD evidence is in
[the corrected-font run](https://github.com/monozukuri-ai/ezsxf/actions/runs/37404988926)
at `2f1eb914732de81ebe4b5cfea84dcc99a89c1918`. Independent Linux review
verified 30 output hashes for 10.03.6 and 23 for 8.25a. Invalid SFC outputs are
retained as failures; leniently recovered models cannot qualify equality.
[Earlier manifest/card evidence](https://github.com/monozukuri-ai/ezsxf/actions/runs/37401844326)
at `61faa6d` retains the duplicate-font failure. The 8.25a corrected-font job
needed one retry after official-installer connection timeouts.

The operating system remains en-US/ACP 1252. The experiment adds an external
manifest to the private runtime, keeps the vendor executable hash unchanged,
and restores the ephemeral runner's `PreferExternalManifest` registry value.
A separate x86 probe reports ACP 932; it does not measure the CAD's own ACP.
The actual CAD observations above determine acceptance. The workflow is manual
and defaults to the unchanged system locale; `ja-JP` is an explicit experiment,
not a generally qualified fix for Japanese Windows behavior.

[Core CI](https://github.com/monozukuri-ai/ezsxf/actions/runs/37404958691) at
`2f1eb91` passed Linux and native MSVC checks: 53 Rust tests, 44 Python tests
on Windows with 3 skipped, fmt, strict clippy, wheel/import/CLI and the platform
save checks. Locally, all 47 Python tests passed after rebuilding the extension.
The newer font fix does not change the earlier filesystem/corpus evidence.

DynaCAD was supplied by the user and both viewer licenses were accepted by the
user before their installers' agreement actions. VoiCeFREE needed MFC42 from
the official Microsoft VC6 redistributable; its download hash matched the local
Winetricks pin. Autodesk's official viewer ZIP returned HTTP 403 on both Linux
and a native Windows runner. No registration data was submitted by automation.
Vendor installers, dependencies, license text, supplied drawings, screenshots,
UI actions and independent results stay private under
`.local/internal/display-review-20261006/`. They are excluded from distributions.

Remaining display coverage includes legacy D0PL001Z SAF/TIFF ingestion (both
the original and the normalized edited bundle showed a SAF-related warning;
the original also warned with an absolute filename on a short C-drive path), larger
real-file coverage, custom SAF schemas/types, native Windows DynaCAD display,
and a reliable CAD save/reopen result for the five-row character case.

## Recorded Wine CAD run

The 2026-10-05/06 check used a task-owned Wine prefix outside the repository,
an isolated Xvfb display and xdotool. Original drawings and supplied dependencies
were staged privately. Each case records the application hashes, input hashes,
UI titles, screenshots, output hashes and exit codes. The source SFC hash must
remain unchanged. Native JWW snapshots were read independently with ezjww;
counts alone were not used as a compatibility pass.

For basic drawings, the CAD imported lines, a circle, an arc, a polyline and
Japanese text. It expanded the polyline into lines, renumbered populated layers,
dropped an unused layer, expanded predefined style tables, added its hidden
auxiliary layer and changed the sheet name to `sheet`. Resolving layer names,
visibility, scale, color and line-type definitions before comparing JWW models
gave equal geometry, text and styles (absolute numeric tolerance 1e-6, relative
1e-9). Raw differences remain recorded separately. These transformations do
not occur in an ezsxf-only resave.

The text-only case includes an apostrophe, a closing parenthesis, a comma and a
literal backslash. Its SFC text value remains unchanged across CAD Save As and
replacement. Jw_cad's JWW snapshot retains the SFC escaped double backslash;
therefore equal imported snapshots alone do not prove a literal-backslash
display matches the original semantic string.

Wine's default Direct2D rendering omitted circles/arcs on screen even though
they remained in both native file formats. Disabling `Direct2d` in the private
Jw_cad `View` settings selected GDI and made the circle, arc, polyline, line and
Japanese text visible. This renderer change is confined to the verification
prefix; screenshots and data checks record the rendering mode separately.
Jw_cad restored Direct2D during the save workflow, so the final display check
explicitly selected GDI again before opening the saved SFC. In that check the
source and replacement/reopened drawing canvas `(80, 60, 1175, 835)` matched
pixel-for-pixel: zero changed pixels. Menu/tool/status regions are excluded.

The successful replacement uses **SFC Save As with the existing basename** and
the overwrite confirmation. Jw_cad disables its normal overwrite command for
an imported SFC. Verification waits for the confirmation to close, the output
mtime to change, the drawing title to match and the file hash to stabilize;
then it closes the CAD and reopens the file in a new process. Generated SFCs
were reparsed strictly. Entity IDs and style-table codes can change relative to
the source, so source-to-CAD comparison and first-save-to-replacement comparison
are separate checks.

The compound-file failure was reproduced from the original SFC as well as the
ezsxf-resaved input, and also through an intermediate JWW. The real-file error
names `CCURVE_ORG001` / `SFIG_LOCATE`. This does not isolate the cause to Wine,
Jw_cad or its bundled SXF library. An error can leave a truncated SFC, which is
rejected by the strict reader and retained only as failed-run evidence. An
unmodified source must be tested in native Windows or another SXF writer before
qualifying compound CAD round trips.

The supplied D0PL001Z image is referenced by the SAF attribute `画像`, rather
than a standalone SFC symbol feature. Both original and resaved bundles were
tested with their actual SAF and TIFF present. Their equal JWW snapshots retain
the attribute figure's block name but do not contain the image reference. No
passed image/attribute round trip is inferred from geometry ingestion or from
byte-for-byte bundle copying.

Private evidence is retained in
`.local/internal/cad-roundtrip-20261005-b/`: per-case `result.json`, screenshots,
native SFC/JWW files, independent reader snapshots and `analysis.json`. Vendor
libraries, supplied drawings and runtime files remain excluded from distribution.

## Recorded native Windows run

The 2026-10-06 native MSVC source build passed in
[GitHub Actions](https://github.com/monozukuri-ai/ezsxf/actions/runs/37397528538)
at commit `af2f99783f46b53e764722272e7887c360bc668b`. The runner asserted the
MSVC host triple, built and installed its own wheel, ran 52 Rust and 47 Python
tests (44 passed, 3 skipped), and passed 8 filesystem checks, including authored
SAF/ATRU image creation, editing, reload and collision protection. Linux
independently verified all 15 artifact hashes from each platform; every file in
the fixed-timestamp authored and revised bundles matches between Windows and
Linux. Results are retained privately under `.local/internal/stage123-ci-atru/`.

New image authoring uses ATRU, as required for SXF 3.1 common image attributes.
Legacy SXF 3.0 SAF/ATRF images remain editable without changing their attachment
mechanism. The clockwise, closed five-point rectangle follows the common
attribute specification, section 3-4, pages 9-10. See also the
[OCF common implementation rules, item 11, page 9](https://ocf.or.jp/pdf/kiyaku201104b.pdf).
This format/API qualification does not certify pixel decoding or CAD raster
display.

### Native Windows CAD scope

The successful Jw_cad 10.03.6 native CAD job is recorded in
[GitHub Actions](https://github.com/monozukuri-ai/ezsxf/actions/runs/37399497412/job/112063262244)
at commit `98bcbcbc25f7d8ab2a7f1e2a0dcfec9a71b7be30`, with private independent
analysis under `.local/internal/stage123-native-final/`.
Dedicated PowerShell/Win32 UI automation on an ephemeral Windows runner opened
Jw_cad 10.03.6, saved owned basic and quoted-text inputs to JWW and SFC, closed
and reopened the SFC, confirmed same-name replacement, and reopened again.
The official installer and application hashes were checked. Each source stays
unchanged, each output hash is recorded, and SFC first-save/replacement models
agree strictly. Independent ezjww snapshots have zero resolved geometry/style
differences between the initial import and final reopen. SFC values retain
Japanese text, apostrophe, parenthesis, comma and a literal backslash.

The native Windows run reproduces `30002: SFIG_LOCATE` for both the all-feature
compound fixture and the owned SAF/ATRU image drawing. These failed/empty
exports are retained as failure evidence and never counted as qualified saves.
The same failure on native Windows rules out a Wine-only explanation; it does
not identify the responsible component within Jw_cad or its SXF library.

Jw_cad 8.25a can save/reopen the compound fixture under Wine, but the generated
SFC gives drawing-group placements nonzero positions and prefixes attribute
figure names. Strict reading rejects it; independent JWW comparisons find
changes in names, styles and structures. Saving successfully is not accepted as
compound or attribute preservation.

In the same native Windows workflow, the 8.25a comparison job failed: all four
cases wrote and replaced SFCs, but the final JWW snapshot command opened an
error dialog naming `????.jww` and never completed. Its operation result stays
failed. Independent review verified all 20 retained file hashes. Basic/quoted
first-save and replacement SFC models agree strictly; compound outputs contain
invalid CP932 sequences and drawing-group transforms, while attribute outputs
change between saves. This comparison does not qualify the older CAD, and the
overall CAD workflow remains failed despite the successful 10.03.6 job.

The runner uses en-US and ANSI code page 1252. Screenshots show Japanese
mojibake, and the readable Arial case displays two backslashes for a semantic
single-backslash input. Independent JWW fields also retain extra backslashes.
Matching imported/reopened snapshots and intact SFC semantic strings therefore
do not qualify Japanese or literal-backslash display in that CAD/environment.
SAF values and TIFF/JPEG display still require
a compatible viewer. The official Autodesk viewer ZIP returned HTTP 403 in
this environment; DynaCAD/Bigvan/V-nas downloads require registration. No
registration or personal information was submitted. The existing Azure VM is
not reconnected: the authenticated account lacked its subscription and the
account-switch device code expired.

The current automation is in `scripts/verify_sfc_cad_windows.ps1` and
`.github/workflows/cad-windows.yml`. It operates only on its dedicated CAD
process, obtains menus/control IDs from the actual application, retains UI
screenshots and native outputs, and separately records operation success and
independent review requirements. Third-party application downloads and vendor
binaries are absent from package payloads.

New image creation also accepted both real 4000x3000 JPEG dependencies and
preserved their bytes through creation/save/reload. The supplied D0PL0011.TIF
is 13087 pixels wide and is rejected for new authoring by the documented
13000-pixel TIFF limit. Existing-bundle copying and placement-only editing
preserve that legacy file; they do not revalidate or convert its pixels.

### Earlier Windows cloud filesystem/corpus run

The 2026-10-05 run installed the Windows wheel into a private, directory-local
Python runtime. Its installed extension hash matched the wheel. New saves,
overwrites and Japanese paths passed, as did preservation of an original file
after validation failure, a denied `FILE_SHARE_DELETE` handle or a read-only
destination. Failed replacements left no temporary file. Bundle rename,
destination collision protection and basic-element creation/editing also passed.

A Rust regression exposed that `std::fs::rename` could replace an existing empty
directory on this Windows environment. Bundle publication now calls
[`MoveFileExW`](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw)
with zero flags, excluding replacement and cross-volume copying. The same
regression passed on Windows after this change, including staging cleanup.
The Linux and Windows-target strict clippy checks also passed.

Python discovered 41 tests: 38 passed, two POSIX-only tests and one optional
matplotlib test were skipped. The cross-compiled Rust runner's optional corpus
tests use a compile-time source path unavailable on the VM; the separate native
Python corpus run actually processed all 20 drawings and their dependencies.

The result ZIP was recovered and its SHA-256 checked. Linux independently
verified 62 recorded artifact hashes, reparsed all Windows-resaved SFCs, compared
them with the original models and Linux serialization, and checked bundle IDs,
sheet structure and unchanged image bytes. The acceptance report and detailed
logs remain private under `.local/internal/windows-cloud/`, including
`independent-save-verification.json`. Temporary Run Commands and private Blob
containers were removed, with absence checked. No CAD UI save/reopen was run in that earlier cloud check.

## Native Windows procedure

Use Windows with Python >=3.9, stable Rust, and the MSVC C++ build prerequisites
available. Extract the source verification bundle or use this source tree.
From PowerShell in its root:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify_sfc_windows.ps1
```

The script creates `.venv-windows`, installs maturin, runs fmt/clippy/Rust tests,
builds the extension, runs all Python tests, and then executes platform checks.
It writes results to a **new** `windows-verification` directory. Failed commands
stop the script. Use `-Python C:\path\to\python.exe` if needed, or `-Output`
with a new directory name for repeat runs.

`verification.json` records the executing OS, Python/package versions, checks
and artifact hashes. Native Windows checks cover new/existing-file replacement
at a Japanese path, failed-validation preservation, bundle rename/collision
protection, basic element edits, and failure cleanup when a file handle denies
`FILE_SHARE_DELETE`, plus read-only-target preservation and temporary cleanup.
This is filesystem verification, not a CAD UI test.

For independent CAD review, open `windows-verification/created.sfc` and
`windows-verification/日本語.sfc` in the installed CAD. Inspect geometry,
Japanese text, layers and complex-feature conversions. Save `created.sfc` as
`cad_saved.sfc` in that CAD, close and reopen it. Check both the newly created
file and a subsequent overwrite. For SAF/image display, use a CAD that supports
the relevant raster/attribute mechanism. `authored/属性画像.sfc` and
`revised/改訂.sfc` contain edited SAF attributes and an actual ATRU TIFF
placement. The separate unedited-copy bundle uses an owned 1x1 BMP dependency
and establishes dependency copying only.

The manually saved SFC can then be inspected without UI automation:

```powershell
.\.venv-windows\Scripts\python.exe scripts/verify_sfc_platform.py --output windows-cad-inspection --cad-saved cad_saved.sfc
```

This records strict-reader warnings and imported feature counts. Counts alone
do not establish CAD geometry/style equality; retain CAD version, screenshots,
saved file and any observed transformations with the result JSON.

## Local checks

```bash
cargo test --offline
cargo fmt --check
cargo clippy --offline --all-targets --all-features -- -D warnings
maturin develop --offline
python -m unittest discover -s tests -p 'test_*.py' -v
python scripts/verify_sfc_platform.py --output platform-verification
```

The platform script labels the native Windows locked-file check `not-run` on
Linux. CI uploads the platform result separately for Ubuntu and Windows. SAF
attribute and TIFF/JPEG placement authoring are supported within the documented
grammar/basic-element scope. No P21 writer, complex-element editor, complete
CAD interchange guarantee or OCF certificate is provided by these checks.

## Compatibility follow-up on 2026-10-06

`literal_backslashes=True` is an explicit, nonstandard display compatibility
option on `serialize_sfc()`, `write_sfc()`, and `SfcDocument.to_bytes()`,
`save()` and `save_bundle()`. It writes an isolated backslash once inside
semantic SFC strings. The standard default still doubles it as prescribed by
appendix SFC §1-1-3(3), printed page 3. Both spellings undergo strict reparse
and equality checks before publication. Consecutive backslashes, a final
backslash or a backslash immediately followed by an apostrophe are ambiguous
with this spelling and raise an error without replacing a file or publishing
a bundle. Header escaping, CP932 double-byte characters and SAF XML escaping
are unaffected. A yen glyph in a substituted Japanese font remains possible.
This option is for reviewed CAD display workflows, not electronic-delivery
conformance. `write_sfc_bundle()` preserves existing sidecars; use
`edit_sfc_bundle(...).save_bundle(...)` for SAF normalization and this option.

```python
document.save("cad-display.sfc", literal_backslashes=True)
document = ezsxf.edit_sfc_bundle("source/D0PL001Z.SFC")
document.save_bundle("reviewed-bundle")
```

DynaCAD Viewer 9.0 and VoiCeFREE 3.5.5.3 / Wine display a single glyph for each
isolated backslash and each separator in `C:\temp\new.sfc` in the API-generated
five-row input; the standard comparison displays doubled glyphs. Japanese,
CP932 characters `ソ 予 表 申 能` and the mixed apostrophe/parenthesis/comma
row remain intact. The owned generator emits `text-literal.sfc` for native
checks as well.

The legacy warning text was read through public Windows UI APIs:
SAF could not be read, or its structure was invalid. In the normalized
D0PL001Z bundle, adding only `type="STR"` to the `画像` attribute removes this
warning and opens the drawing. The SAF writer now emits the predefined STR
type when omitted for `画像` and `ファイル名` (attribute mechanism specification
§1 and table 8, S-02/S-16, printed pages 1/17). Explicit types and other omitted
types remain unchanged, including the numeric `ターゲット` and `等高線`.
The 3.0-to-3.1 upgrade retains the attribute-set metadata, figure IDs, values,
dependency names and unpadded legacy date. This is a Wine result; native
Windows DynaCAD remains unqualified.

[The native default-locale control](https://github.com/monozukuri-ai/ezsxf/actions/runs/37421483048)
at `21af321` retains all five SFC text values, fonts and placements through
save, overwrite and reopen in Jw_cad 10.03.6. Strict parsing and independent
resolved JWW comparison pass; the latter has zero differences. Its en-US
1252 canvas/JWW still contains Japanese mojibake, so stable saved SFC data
does not qualify Japanese display. The 8.25a control fails its basic-operation
gate under the non-Japanese system locale; it is retained as failure evidence.

Jw_cad's [official change log](https://www.jwcad.net/versioninfo.htm) records
the Unicode UTF-16 migration in 10.01. The external `ja-JP` manifest experiment
is now restricted to 8.25a; the standalone verification script rejects it for
10.03.6 before launching CAD. The native workflow checks strict saved text,
resolved font names and placement independently of the operation gate, and
requires those checks for 10.03.6. Earlier manifest-induced changes remain
negative evidence rather than a claim about every Japanese Windows system.
