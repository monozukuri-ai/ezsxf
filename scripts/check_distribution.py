"""Check release payload, versions, source completeness and artifact hashes."""

from __future__ import annotations

import argparse
import hashlib
import re
import tarfile
import zipfile
from email.parser import BytesParser
from pathlib import Path, PurePosixPath


def check(directory: Path, version: str) -> dict[str, str]:
    wheels = sorted(directory.glob("*.whl"))
    sources = sorted(directory.glob("*.tar.gz"))
    if not wheels or len(sources) != 1:
        raise ValueError("Expected wheels and exactly one source distribution")
    hashes = {}
    for path in [*wheels, *sources]:
        hashes[path.name] = hashlib.sha256(path.read_bytes()).hexdigest()
        if path.suffix == ".whl":
            with zipfile.ZipFile(path) as archive:
                names = set(archive.namelist())
                assert {
                    "ezsxf/__init__.py",
                    "ezsxf/_core.pyi",
                    "ezsxf/py.typed",
                } <= names
                assert any(
                    n.startswith("ezsxf/_core") and n.endswith((".so", ".pyd"))
                    for n in names
                )
                metadata = [n for n in names if n.endswith(".dist-info/METADATA")]
                assert len(metadata) == 1
                msg = BytesParser().parsebytes(archive.read(metadata[0]))
                assert msg["Name"] == "ezsxf" and msg["Version"] == version
                assert msg["Requires-Python"] == ">=3.9"
                stub = archive.read("ezsxf/_core.pyi").decode()
                for api in (
                    "new_sfc",
                    "add_color",
                    "add_line_type",
                    "add_line_width",
                    "save",
                    "extend",
                    "create_part",
                    "to_p21_bytes",
                    "save_p21",
                    "estimate_text_width",
                    "serialize_p21",
                    "write_p21",
                    "save_p21_bundle",
                    "write_p21_bundle",
                    "to_p2z_bytes",
                    "save_p2z",
                    "write_p2z",
                ):
                    assert f"def {api}(" in stub
                assert any(n.endswith("LICENSE") for n in names)
        else:
            with tarfile.open(path) as archive:
                members = archive.getmembers()
                roots = {PurePosixPath(m.name).parts[0] for m in members}
                assert len(roots) == 1
                names = {
                    str(PurePosixPath(m.name).relative_to(next(iter(roots))))
                    for m in members
                }
                root = next(iter(roots))
                for required in (
                    "Cargo.toml",
                    "Cargo.lock",
                    "pyproject.toml",
                    "LICENSE",
                    "src/style_editor.rs",
                    "src/writer.rs",
                    "src/authoring.rs",
                    "src/p21_writer.rs",
                    "src/p21_annotations.rs",
                    "src/p21_hatches.rs",
                    "src/p21_bundle.rs",
                    "src/editor.rs",
                    "src/python_editor.rs",
                    "src/ezsxf/_core.pyi",
                    "tests/test_sfc_styles.py",
                    "tests/test_writer_requests.py",
                    "tests/test_p21_delivery.py",
                    "tests/fixtures/writer_all_features.sfc",
                    "scripts/verify_sfc_mvp.py",
                    "scripts/verify_writer_requests.py",
                    "docs/sfc-writer-mvp.md",
                    "docs/p21-writing.md",
                ):
                    assert required in names, f"Missing source file: {required}"
                for member in members:
                    relative = PurePosixPath(member.name).relative_to(root)
                    assert not member.issym() and not member.islnk()
                    assert not (
                        {".local", "resources", "data", ".venv", ".git", "target"}
                        & set(relative.parts)
                    )
                    assert member.isdir() or relative.suffix.lower() not in {
                        ".exe",
                        ".dll",
                        ".msi",
                        ".zip",
                    }
                for name in ("Cargo.toml", "pyproject.toml"):
                    content = archive.extractfile(f"{root}/{name}").read().decode()
                    assert re.search(
                        r'^version = "' + re.escape(version) + '"$',
                        content,
                        re.MULTILINE,
                    )
    return hashes


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--version", required=True)
    parser.add_argument("--hashes", type=Path, required=True)
    args = parser.parse_args()
    hashes = check(args.directory, args.version)
    args.hashes.write_text(
        "".join(f"{value}  {name}\n" for name, value in hashes.items()),
        encoding="utf-8",
    )
    print(f"Qualified {len(hashes)} distribution artifacts for {args.version}")


if __name__ == "__main__":
    main()
