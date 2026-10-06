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
| CAD SAF/image check | The original D0PL001Z SFC/SAF/TIFF bundle and the ezsxf bundle produce equal imported JWW models. The model retains the `$$ATRF$$1` block name but contains neither the TIFF filename nor a bitmap placement. This does not qualify SAF attribute or raster display; SFC export fails with the same compound-reference error. |
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

The native CAD check is recorded in
[GitHub Actions](https://github.com/monozukuri-ai/ezsxf/actions/runs/37399497412)
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
