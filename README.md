# ezsxf

A fast SXF parser, SFC writer and drawing converter for Python, powered by Rust
and PyO3. Python 3.9 or later is supported.

- Parse P21/SFC from a file path, text or bytes, with strict or lenient recovery.
- Create, edit and save SFC geometry, text, styles, groups, dimensions and hatches.
- Add elements in a single validated batch and export P21/P2Z with dimensions, hatches, attributes and images.
- Deliver drawings with SAF attributes and TIFF/JPEG dependencies.
- Export DXF or plot drawings with the optional matplotlib backend.

## Installation

```bash
python -m pip install ezsxf
```

For plotting:

```bash
python -m pip install "ezsxf[plot]"
```

The core package has no runtime Python dependencies. Prebuilt wheels are
available for Linux x86-64, Windows x86-64 and macOS x86-64/ARM64. Building from
source requires Rust; see [Contributing](CONTRIBUTING.md).

## Quick start

```python
import ezsxf

doc = ezsxf.new_sfc("drawing.sfc", name="Drawing", paper="A1", orientation="landscape")
layer = doc.add_layer("Structure")
color = doc.add_color((32, 96, 160))
doc.add_line((10, 10), (100, 50), layer=layer, color=color)
doc.add_circle((50, 50), 10, layer=layer)
doc.add_text("SXF 日本語", (10, 80), height=3.5)  # estimates a monospaced box width
doc.save("drawing.sfc")
doc.save_p21("drawing.p21")

parsed = ezsxf.parse_sfc("drawing.sfc", strict=True)
ezsxf.write_sfc(parsed, "copy.sfc")
print(len(parsed["typed_features"]))
```

Use `parse_p21` for P21 input. For existing SFC edits, start with `edit_sfc(parsed)`.
SFC output uses CP932 and validates the complete model before saving. Excess
precision or unencodable text raises an error. Drawings with external SAF/image
files should be delivered with the bundle APIs.

## CLI

```bash
python -m ezsxf parse sfc drawing.sfc --pretty
python -m ezsxf to-dxf drawing.sfc drawing.dxf
python -m ezsxf resave-sfc drawing.sfc copy.sfc
python -m ezsxf to-p21 drawing.sfc drawing.p21
```

Run `python -m ezsxf --help` or see the [CLI guide](docs/cli.md).

## Supported scope

| Format | Support |
| --- | --- |
| SFC | All 34 SXF Ver.3.1 feature types are parsed with resolved hierarchy, style codes and attribute attachments. Saving and supported editing APIs retain the SFC model. |
| P21/P2Z output | AP202 output from SFC models includes curves, text, dimensions/leaders, fills/hatches, styles, subfigures and attributes. P21 bundles and P2Z include validated SAF/images. Unsupported elements fail before saving; see [P21 writing](docs/p21-writing.md). |

Model preservation and third-party CAD display/re-export have separate limits.
See [compatibility](docs/compatibility.md) for Japanese text, backslashes,
groups, SAF and images. The package does not claim OCF certification.

## Documentation

Start with the [documentation index](docs/README.md), or choose a guide:

- [Reading P21/SFC and parse results](docs/usage.md)
- [Saving existing SFC drawings](docs/sfc-writing.md)
- [Basic SFC creation, editing and style codes](docs/sfc-writer-mvp.md)
- [P21 export and interoperability checks](docs/p21-writing.md)
- [Complex elements, groups, dimensions and hatches](docs/sfc-editing.md)
- [SAF attributes, images and bundle delivery](docs/sfc-bundles.md)
- [Title-block attribute names](docs/title-block-attributes.md)
- [DXF conversion and plotting](docs/conversion.md)

## Contributing and license

See [Contributing](CONTRIBUTING.md) for source setup and checks, and
[Code of Conduct](CODE_OF_CONDUCT.md) for participation guidelines.
MIT licensed; see [LICENSE](LICENSE).
