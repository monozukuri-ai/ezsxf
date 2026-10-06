"""Create an owned, vendor-free group/SAF/TIFF/JPEG CAD re-export package."""

import argparse
import hashlib
import json
import zipfile
from pathlib import Path

from prepare_sfc_display import prepare
from verify_sfc_reexport import snapshot

import ezsxf


def prepare_reexport(root: Path) -> None:
    archive = root.parent / (root.name + ".zip")
    if archive.exists() or archive.is_symlink():
        raise FileExistsError(f"Archive already exists: {archive}")
    root.mkdir(parents=True, exist_ok=False)
    display = root / "display"
    prepare(display)
    cases = []
    for kind, name in [(3, "group"), (4, "part")]:
        document = ezsxf.new_sfc(name + ".sfc", timestamp="2026-10-06T00:00:00")
        child = document.add_circle((50, 50), 10)
        placement = document.group_elements("設計" + name, [child], kind=kind)
        if kind == 4:
            document.place_part(
                placement, position=(100, 30), angle=15, scale=(1.5, 1.5)
            )
        path = root / (name + ".sfc")
        document.save(path)
        cases.append({"id": name, "path": path.relative_to(root).as_posix()})
    nested = ezsxf.new_sfc("nested.sfc", timestamp="2026-10-06T00:00:00")
    child = nested.add_circle((50, 50), 10)
    inner = nested.group_elements("inner", [child])
    nested.group_elements("outer", [inner])
    nested.save(root / "nested.sfc")
    cases.append({"id": "nested", "path": "nested.sfc"})
    for identifier, name in [
        ("saf", "attributes/attributes.sfc"),
        ("images", "images/images.sfc"),
        ("revised-images", "revised-images/images.sfc"),
    ]:
        cases.append({"id": identifier, "path": "display/" + name})
    combined = ezsxf.edit_sfc_bundle(display / "attributes/attributes.sfc")
    child = combined.add_circle((100, 50), 12)
    combined.group_elements("設計グループ", [child])
    combined.add_image(display / "card.tif", (20, 80), 80, 40)
    combined.add_image(display / "card.jpg", (150, 80), 50, 50, angle=15)
    combined.save_bundle(root / "combined", file_name="combined.sfc")
    cases.append({"id": "combined", "path": "combined/combined.sfc"})
    expectations = {case["id"]: snapshot(root / case["path"]) for case in cases}
    (root / "expected.json").write_text(
        json.dumps(expectations, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    (root / "README.txt").write_text(
        "Open each input listed in cases.json in the CAD under review.\n"
        "Use SXF V3/3.1 export to a new directory per case, retaining SAF and images.\n"
        "Close CAD, reopen the exported drawing, review groups/attributes/images, and export again.\n"
        "Record CAD version, locale, export settings, warnings and actual screenshots.\n"
        "Compare input to first export and first export to second export:\n"
        "python scripts/verify_sfc_reexport.py INPUT EXPORTED --report REPORT.json\n"
        "Group names/hierarchy/reuse, geometry/styles, SAF bindings/values and image bytes must survive.\n"
        "Entity IDs, code-table indices and dependency basenames may change.\n"
        "Image recompression fails byte-exact comparison and needs separate decoded-pixel review.\n"
        "A successful local copy comparison alone does not qualify any CAD.\n",
        encoding="utf-8",
    )
    hashes = {
        p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
        for p in sorted(root.rglob("*"))
        if p.is_file()
    }
    (root / "cases.json").write_text(
        json.dumps(
            {
                "cases": cases,
                "files_sha256": hashes,
                "third_party_reexport_qualified": False,
            },
            ensure_ascii=False,
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    with zipfile.ZipFile(archive, "x", zipfile.ZIP_DEFLATED) as bundle:
        for file in sorted(root.rglob("*")):
            if file.is_file():
                bundle.write(file, file.relative_to(root))
    print(
        json.dumps(
            {
                "cases": len(cases),
                "archive": str(archive),
                "sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
            }
        )
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    prepare_reexport(parser.parse_args().output)
