"""Prepare owned CAD review inputs after verify_sfc_platform.py."""

from pathlib import Path
import sys

import ezsxf


def prepare(root: Path) -> None:
    document = ezsxf.edit_sfc_bundle(root / "authored/属性画像.sfc")
    document.save_bundle(root / "cad-attributes", file_name="attributes.sfc")
    quoted = ezsxf.new_sfc("quoted.sfc", timestamp="2026-10-06T00:00:00")
    quoted.add_text("quote' ) , \\ end", (20, 50), height=5, width=3.5)
    quoted.add_text("日本語", (20, 30), height=5, width=5)
    quoted.save(root / "quoted.sfc")


if __name__ == "__main__":
    prepare(Path(sys.argv[1]))
