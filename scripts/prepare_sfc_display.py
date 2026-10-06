"""Create owned, visible SFC/SAF/TIFF/JPEG cards for external display review.

Requires Pillow only in the qualification environment. No vendor drawings or
generated images are shipped with ezsxf. Pixel layout and text expectations are
recorded separately from any CAD screenshot, so a blank image cannot pass.
"""

import argparse
import hashlib
import json
from pathlib import Path

import PIL
from PIL import Image, ImageDraw

import ezsxf


def prepare(root: Path) -> None:
    root.mkdir(parents=True, exist_ok=False)
    mono = Image.new("1", (256, 128), 1)
    draw = ImageDraw.Draw(mono)
    draw.rectangle((0, 0, 255, 127), outline=0, width=4)
    draw.rectangle((12, 12, 44, 110), fill=0)
    draw.rectangle((12, 80, 120, 110), fill=0)
    draw.text((80, 20), "TIFF TOP", fill=0)
    for y in range(64, 112, 16):
        for x in range(160, 240, 16):
            if (x // 16 + y // 16) % 2:
                draw.rectangle((x, y, x + 15, y + 15), fill=0)
    tiff = root / "card.tif"
    mono.save(tiff, compression="group4", dpi=(100, 100))
    rgb = Image.new("RGB", (128, 128), "white")
    draw = ImageDraw.Draw(rgb)
    for bounds, color in [
        ((0, 0, 63, 63), "red"),
        ((64, 0, 127, 63), "lime"),
        ((0, 64, 63, 127), "blue"),
        ((64, 64, 127, 127), "yellow"),
    ]:
        draw.rectangle(bounds, fill=color)
    draw.text((8, 8), "TOP", fill="white")
    draw.rectangle((0, 0, 127, 127), outline="black", width=4)
    jpeg = root / "card.jpg"
    rgb.save(jpeg, quality=95, subsampling=0)
    texts = [
        ("日本語 ABC 123", "ＭＳ ゴシック"),
        ("backslash: \\ end", "Arial"),
        ("path C:\\temp\\new.sfc", "Arial"),
        ("ソ 予 表 申 能", "ＭＳ ゴシック"),
        ("日本語 + \\ + quote' ) ,", "ＭＳ ゴシック"),
    ]
    text_doc = ezsxf.new_sfc("text.sfc", timestamp="2026-10-06T00:00:00")
    for index, (text, font) in enumerate(texts):
        code = text_doc.add_font(font)
        text_doc.add_text(
            text, (20, 170 - index * 30), height=8,
            width=4 * len(text.encode("cp932")), font=code,
        )
    text_doc.save(root / "text.sfc")
    text_doc.save(root / "text-literal.sfc", literal_backslashes=True)
    attributes = ezsxf.new_sfc("attributes.sfc", timestamp="2026-10-06T00:00:00")
    target = attributes.add_circle((50, 50), 10)
    attributes.set_attribute(target, "材料", "鉄 & 鋼", group=["設計"])
    attributes.set_attribute(target, "備考", "日本語 + \\ + quote' ) ,", group=["設計"])
    attributes.save_bundle(root / "attributes", file_name="attributes.sfc")
    images = ezsxf.new_sfc("images.sfc", timestamp="2026-10-06T00:00:00")
    tiff_id = images.add_image(tiff, (20, 80), 80, 40)
    jpeg_id = images.add_image(jpeg, (150, 80), 50, 50)
    images.save_bundle(root / "images", file_name="images.sfc")
    images.update_image(tiff_id, (30, 60), 100, 50, angle=15)
    images.update_image(jpeg_id, (160, 90), 40, 40, angle=-15)
    images.save_bundle(root / "revised-images", file_name="images.sfc")
    expectations = {
        "pillow": PIL.__version__,
        "texts": [text for text, _ in texts],
        "saf": attributes.get_attributes(target),
        "image_pixels": {"TIFF": "nonblank black L and checkerboard", "JPEG": "red/green above blue/yellow"},
        "source_model_reparse": True,
        "files_sha256": {},
        "external_display_verified": False,
        "literal_backslashes": "text-literal.sfc uses nonstandard single-byte spelling for isolated backslashes; its strict parsed model equals text.sfc",
    }
    for file in sorted(root.rglob("*")):
        if file.is_file():
            expectations["files_sha256"][file.relative_to(root).as_posix()] = hashlib.sha256(file.read_bytes()).hexdigest()
            if file.suffix == ".sfc":
                parsed = ezsxf.parse_sfc(file.read_bytes())
                assert not parsed["warnings"]
                assert ezsxf.parse_sfc(ezsxf.serialize_sfc(parsed, allow_external_references=True)) == parsed
    (root / "expectations.json").write_text(json.dumps(expectations, ensure_ascii=False, indent=2), encoding="utf-8")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    prepare(parser.parse_args().output)
