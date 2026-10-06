"""Compare strict SFC text contents, resolved fonts and placement after CAD saves.

This checks saved data. A separate visual review must check the CAD canvas.
"""

import argparse
from collections import Counter
import json
from pathlib import Path

import ezsxf


def text_records(path: Path) -> Counter:
    parsed = ezsxf.parse_sfc(path.read_bytes(), strict=True)
    if parsed["warnings"]:
        raise ValueError(f"SFC warnings in {path.name}")
    features = parsed["typed_features"]
    fonts = [f["name"] for f in features if f["kind"] == "text_font"]
    rows = []
    for feature in features:
        if feature["kind"] != "text":
            continue
        row = {key: value for key, value in feature.items()
               if key not in {"id", "keyword", "kind", "style"}}
        code = feature["style"]["font_code"]
        row["font"] = fonts[code - 1] if code else None
        rows.append(json.dumps(row, ensure_ascii=False, sort_keys=True))
    return Counter(rows)


def verify(root: Path) -> dict:
    result = {}
    for case in ["basic", "quoted", "display-text", "display-text-literal"]:
        folder = root / case
        if not folder.exists():
            continue
        record = {"passed": False}
        try:
            source = text_records(folder / "input.sfc")
            if not source:
                raise ValueError("Expected text input is empty")
            record["text_count"] = sum(source.values())
            for name in ["first-save.sfc", "overwrite.sfc"]:
                saved = text_records(folder / name)
                if saved != source:
                    record["missing"] = list((source - saved).elements())
                    record["unexpected"] = list((saved - source).elements())
                    raise ValueError(f"Text contents, fonts or placement changed in {name}")
            record["passed"] = True
        except (OSError, ValueError, IndexError) as error:
            record["error"] = str(error)
        result[case] = record
    return result


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("root", type=Path)
    parser.add_argument("--require", action="store_true")
    args = parser.parse_args()
    result = verify(args.root)
    (args.root / "text-verification.json").write_text(
        json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    print(json.dumps({case: row["passed"] for case, row in result.items()}))
    if args.require and (len(result) != 4 or not all(row["passed"] for row in result.values())):
        raise SystemExit("CAD text save verification failed")


if __name__ == "__main__":
    main()
