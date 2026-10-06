"""Generate SFC/CAD review files and qualify saving on the executing platform."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import struct
import sys
from collections import Counter
from pathlib import Path

import ezsxf


def verify(output: Path, report: dict) -> None:
    fixture = (
        Path(__file__).resolve().parents[1] / "tests/fixtures/writer_all_features.sfc"
    )
    standalone = ezsxf.parse_sfc(
        fixture.read_text(encoding="utf-8").replace("$$ATRF$$102", "ordinary")
    )
    saved = output / "日本語.sfc"
    ezsxf.write_sfc(standalone, saved)
    ezsxf.write_sfc(standalone, saved)
    assert ezsxf.parse_sfc(str(saved)) == standalone
    report["checks"]["new_and_existing_file_unicode_path"] = True
    before = saved.read_bytes()
    invalid = dict(standalone, warnings=[{"code": "test", "message": "reject"}])
    try:
        ezsxf.write_sfc(invalid, saved)
    except ValueError:
        pass
    else:
        raise AssertionError("Partial parse was saved")
    assert saved.read_bytes() == before
    assert not list(output.glob(".*.ezsxf-*.tmp"))
    report["checks"]["failed_validation_preserves_file"] = True

    source = output / "source"
    source.mkdir()
    (source / "writer-fixture.sfc").write_bytes(fixture.read_bytes())
    (source / "writer-fixture.SAF").write_text(
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<SxfAttributeXML version="3.1" date="2026-10-05" application="test" sxfFile="writer-fixture.sfc">'
        '<Figure id="102"><AttributeSet name="sample" version="1" designedBy="test">'
        '<Attr name="画像">pixel.bmp</Attr></AttributeSet></Figure></SxfAttributeXML>',
        encoding="utf-8",
    )
    # Authored, valid uncompressed 1x1 24-bit BMP; vendor data is not distributed.
    bmp = (
        b"BM"
        + struct.pack("<IHHI", 58, 0, 0, 54)
        + struct.pack("<IiiHHIIiiII", 40, 1, 1, 1, 24, 0, 4, 2835, 2835, 0, 0)
        + b"\x00\x00\xff\x00"
    )
    (source / "pixel.bmp").write_bytes(bmp)
    bundle = output / "bundle"
    result = ezsxf.write_sfc_bundle(
        source / "writer-fixture.sfc", bundle, file_name="bundle.sfc"
    )
    assert result["files"] == ["bundle.SAF", "bundle.sfc", "pixel.bmp"]
    assert (bundle / "pixel.bmp").read_bytes() == bmp
    assert 'sxfFile="bundle.sfc"' in (bundle / "bundle.SAF").read_text(encoding="utf-8")
    before = {p.name: p.read_bytes() for p in bundle.iterdir()}
    try:
        ezsxf.write_sfc_bundle(source / "writer-fixture.sfc", bundle)
    except FileExistsError:
        pass
    else:
        raise AssertionError("Existing bundle was replaced")
    assert {p.name: p.read_bytes() for p in bundle.iterdir()} == before
    empty = output / "existing_empty_directory"
    empty.mkdir()
    try:
        ezsxf.write_sfc_bundle(source / "writer-fixture.sfc", empty)
    except FileExistsError:
        pass
    else:
        raise AssertionError("Existing empty directory was replaced")
    report["checks"]["bundle_rename_and_collision_protection"] = True

    doc = ezsxf.new_sfc("created.sfc", name="CAD review", width_mm=297, height_mm=210)
    doc.add_line((10, 10), (100, 10))
    circle = doc.add_circle((60, 60), 20)
    doc.add_arc((120, 60), 20, 0, 120)
    doc.add_polyline([(10, 100), (40, 120), (70, 100)])
    doc.add_text("SXF 日本語", (10, 150))
    doc.update_element(circle, radius=25)
    doc.save(output / "created.sfc")
    assert ezsxf.parse_sfc(str(output / "created.sfc")) == doc.to_dict()
    report["checks"]["basic_element_creation_and_edit"] = True

    # Exercise authored SAF and raster delivery using a self-contained G4 TIFF.
    image_bytes = bytes.fromhex(
        "49492a001000000026a2fffe00200200090000010300010000000800000001010300"
        "0100000008000000020103000100000001000000030103000100000004000000"
        "0601030001000000010000001101040001000000080000001601030001000000"
        "080000001701040001000000070000001c010300010000000100000000000000"
    )
    image_source = output / "scan.tif"
    image_source.write_bytes(image_bytes)
    authored = ezsxf.new_sfc("authored.sfc", timestamp="2026-10-06T00:00:00")
    target = authored.add_circle((50, 50), 10)
    authored.set_attribute(target, "材料", "鉄 & 鋼", group=["設計"])
    image_id = authored.add_image(image_source, (80, 40), 20, 10, angle=15)
    authored.save_bundle(output / "authored", file_name="属性画像.sfc")
    reloaded = ezsxf.edit_sfc_bundle(output / "authored/属性画像.sfc")
    assert reloaded.get_attributes(target) == authored.get_attributes(target)
    reloaded.set_attribute(target, "材料", "変更", group=["設計"])
    reloaded.update_image(image_id, (85, 45), 30, 15)
    reloaded.save_bundle(output / "revised", file_name="改訂.sfc")
    ezsxf.validate_saf((output / "revised/改訂.SAF").read_bytes())
    assert (output / "revised/scan.tif").read_bytes() == image_bytes
    assert ezsxf.edit_sfc_bundle(output / "revised/改訂.sfc").get_attributes(target)[0]["value"] == "変更"
    report["checks"]["saf_attribute_and_raster_creation_edit_delivery"] = True

    prior = {p.name: p.read_bytes() for p in (output / "revised").iterdir()}
    try:
        reloaded.save_bundle(output / "revised")
    except FileExistsError:
        pass
    else:
        raise AssertionError("Authored bundle replaced an existing directory")
    assert {p.name: p.read_bytes() for p in (output / "revised").iterdir()} == prior
    report["checks"]["authored_bundle_collision_preserves_delivery"] = True

    if os.name == "nt":
        import ctypes
        from ctypes import wintypes

        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.CreateFileW.argtypes = [
            wintypes.LPCWSTR,
            wintypes.DWORD,
            wintypes.DWORD,
            wintypes.LPVOID,
            wintypes.DWORD,
            wintypes.DWORD,
            wintypes.HANDLE,
        ]
        kernel.CreateFileW.restype = wintypes.HANDLE
        kernel.CloseHandle.argtypes = [wintypes.HANDLE]
        kernel.CloseHandle.restype = wintypes.BOOL
        before_file = saved.read_bytes()
        handle = kernel.CreateFileW(
            str(saved.resolve()), 0x80000000, 1, None, 3, 0x80, None
        )
        if handle == wintypes.HANDLE(-1).value:
            raise ctypes.WinError(ctypes.get_last_error())
        try:
            try:
                ezsxf.write_sfc(standalone, saved)
            except OSError:
                pass
            else:
                raise AssertionError(
                    "Replacing a file with no FILE_SHARE_DELETE succeeded"
                )
            assert saved.read_bytes() == before_file
            assert not list(output.glob(".*.ezsxf-*.tmp"))
        finally:
            kernel.CloseHandle(handle)
        report["checks"][
            "windows_locked_file_preserves_original_and_cleans_temporary"
        ] = True
        import stat

        saved.chmod(stat.S_IREAD)
        try:
            try:
                ezsxf.write_sfc(standalone, saved)
            except OSError:
                pass
            else:
                raise AssertionError(
                    "Replacing a Windows read-only destination succeeded"
                )
            assert saved.read_bytes() == before_file
            assert not (saved.stat().st_mode & stat.S_IWRITE)
            assert not list(output.glob(".*.ezsxf-*.tmp"))
        finally:
            saved.chmod(stat.S_IREAD | stat.S_IWRITE)
        report["checks"]["windows_readonly_file_preserved_and_temporary_cleaned"] = True
    else:
        report["checks"][
            "windows_locked_file_preserves_original_and_cleans_temporary"
        ] = "not-run"
        report["checks"]["windows_readonly_file_preserved_and_temporary_cleaned"] = (
            "not-run"
        )
    report["files_sha256"] = {
        str(p.relative_to(output)): hashlib.sha256(p.read_bytes()).hexdigest()
        for p in sorted(output.rglob("*"))
        if p.is_file()
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output", type=Path, required=True, help="New result directory"
    )
    parser.add_argument(
        "--cad-saved", type=Path, help="Additionally inspect a manually CAD-saved SFC"
    )
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    report = {
        "platform": platform.platform(),
        "python": sys.version,
        "ezsxf": ezsxf.__version__,
        "native_windows": os.name == "nt",
        "checks": {},
        "complete": False,
        "cad_compatibility": "manual-review-required",
    }
    try:
        verify(args.output, report)
        if args.cad_saved:
            parsed = ezsxf.parse_sfc(str(args.cad_saved), strict=True)
            report["cad_saved"] = {
                "sha256": hashlib.sha256(args.cad_saved.read_bytes()).hexdigest(),
                "warnings": parsed["warnings"],
                "feature_counts": dict(
                    Counter(f["kind"] for f in parsed["typed_features"])
                ),
            }
        report["complete"] = True
    except (ValueError, TypeError, OSError, AssertionError) as exc:
        report["error"] = str(exc)
    (args.output / "verification.json").write_text(
        json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    print(json.dumps(report, ensure_ascii=True, indent=2))
    return 0 if report["complete"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
