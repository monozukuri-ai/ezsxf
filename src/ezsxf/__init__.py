from __future__ import annotations

import argparse
import json
import sys
from importlib.metadata import PackageNotFoundError, version
from typing import Sequence

from ezsxf._core import (
    SfcDocument,
    edit_sfc,
    edit_sfc_bundle,
    estimate_text_width,
    hello_from_bin,
    new_sfc,
    parse_p21,
    parse_sfc,
    serialize_sfc,
    serialize_p21,
    write_sfc,
    write_p21,
    write_p21_bundle,
    write_p2z,
    write_sfc_bundle,
    validate_saf,
)
from ezsxf._dxf import to_dxf
from ezsxf._plot import plot

try:
    __version__ = version("ezsxf")
except PackageNotFoundError:
    __version__ = "0.0.0"


def _build_cli_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="ezsxf",
        description="SXF (P21/SFC) parser and drawing converter CLI",
    )
    subcommands = parser.add_subparsers(dest="command")

    subcommands.add_parser("hello", help="Print extension smoke-test message")

    parse_cmd = subcommands.add_parser("parse", help="Parse P21/SFC and emit JSON")
    parse_cmd.add_argument("format", choices=["p21", "sfc"], help="Input file format")
    parse_cmd.add_argument("input", help="Path to input file")
    _add_strict_arguments(parse_cmd)
    parse_cmd.add_argument(
        "--pretty",
        action="store_true",
        help="Pretty-print JSON output",
    )

    resave_cmd = subcommands.add_parser(
        "resave-sfc", help="Validate and resave an existing SFC drawing"
    )
    resave_cmd.add_argument("input", help="Path to input SFC file")
    resave_cmd.add_argument("output", help="Path to output SFC file")
    p21_cmd = subcommands.add_parser("to-p21", help="Convert supported SFC elements to SXF P21")
    p21_cmd.add_argument("input", help="Path to input SFC file")
    p21_cmd.add_argument("output", help="Path to output P21 file")
    bundle_p21 = subcommands.add_parser("bundle-p21", help="Convert SFC and dependencies into a new P21 bundle directory")
    bundle_p21.add_argument("input", help="Path to input SFC file")
    bundle_p21.add_argument("output_directory", help="New destination directory")
    bundle_p21.add_argument("--file-name", help="P21 filename; SAF is renamed consistently")
    p2z_cmd = subcommands.add_parser("to-p2z", help="Convert SFC and its referenced SAF/images to a compressed P2Z")
    p2z_cmd.add_argument("input", help="Path to input SFC file")
    p2z_cmd.add_argument("output", help="Path to output P2Z file")
    resave_cmd.add_argument(
        "--allow-external-references",
        action="store_true",
        help="Preserve SAF references without copying SAF or image files",
    )

    bundle_cmd = subcommands.add_parser(
        "bundle-sfc", help="Save an SFC, SAF and dependencies into a new directory"
    )
    bundle_cmd.add_argument("input", help="Path to input SFC file")
    bundle_cmd.add_argument("output_directory", help="New destination directory")
    bundle_cmd.add_argument("--file-name", help="Rename drawing and SAF consistently")
    bundle_cmd.add_argument(
        "--extra-file",
        action="append",
        default=[],
        help="Additional local dependency filename",
    )

    dxf_cmd = subcommands.add_parser("to-dxf", help="Convert SFC to DXF")
    dxf_cmd.add_argument("input", help="Path to input SFC file")
    dxf_cmd.add_argument("output", help="Path to output DXF file")
    _add_strict_arguments(dxf_cmd)
    dxf_cmd.add_argument(
        "--curve-segments",
        type=int,
        default=64,
        help="Segments per full curve (default: 64)",
    )

    plot_cmd = subcommands.add_parser("plot", help="Draw SFC with matplotlib")
    plot_cmd.add_argument("input", help="Path to input SFC file")
    plot_cmd.add_argument(
        "output",
        nargs="?",
        help="Optional output image path; opens a window when omitted",
    )
    _add_strict_arguments(plot_cmd)
    plot_cmd.add_argument(
        "--curve-segments",
        type=int,
        default=64,
        help="Segments per full curve (default: 64)",
    )
    plot_cmd.add_argument(
        "--monochrome",
        action="store_true",
        help="Render all visible entities in one foreground color",
    )
    plot_cmd.add_argument(
        "--show-axes",
        action="store_true",
        help="Show matplotlib axes",
    )
    plot_cmd.add_argument(
        "--show",
        action="store_true",
        help="Show a window even when saving an image",
    )
    plot_cmd.add_argument(
        "--dpi",
        type=int,
        default=150,
        help="Output image resolution (default: 150)",
    )

    return parser


def _add_strict_arguments(parser: argparse.ArgumentParser) -> None:
    strict_group = parser.add_mutually_exclusive_group()
    strict_group.add_argument(
        "--strict",
        action="store_true",
        default=True,
        help="Fail on violations (default)",
    )
    strict_group.add_argument(
        "--lenient",
        dest="strict",
        action="store_false",
        help="Collect warnings and continue where possible",
    )


def main(argv: Sequence[str] | None = None) -> int:
    parser = _build_cli_parser()
    args = parser.parse_args(list(argv) if argv is not None else None)

    if args.command in (None, "hello"):
        print(hello_from_bin())
        return 0

    if args.command == "parse":
        try:
            result = (
                parse_p21(args.input, strict=args.strict)
                if args.format == "p21"
                else parse_sfc(args.input, strict=args.strict)
            )
        except Exception as exc:
            print(f"parse error: {exc}", file=sys.stderr)
            return 1
        if args.pretty:
            print(json.dumps(result, ensure_ascii=False, indent=2))
        else:
            print(json.dumps(result, ensure_ascii=False, separators=(",", ":")))
        return 0

    if args.command == "to-dxf":
        try:
            to_dxf(
                args.input,
                args.output,
                strict=args.strict,
                curve_segments=args.curve_segments,
            )
        except Exception as exc:
            print(f"DXF conversion error: {exc}", file=sys.stderr)
            return 1
        return 0

    if args.command == "to-p21":
        try:
            write_p21(parse_sfc(args.input, strict=True), args.output)
        except (ValueError, TypeError, OSError) as exc:
            print(f"P21 conversion error: {exc}", file=sys.stderr)
            return 1
        return 0

    if args.command in {"bundle-p21", "to-p2z"}:
        try:
            if args.command == "bundle-p21":
                report = write_p21_bundle(args.input, args.output_directory, file_name=args.file_name)
                print(json.dumps(report, ensure_ascii=False))
            else:
                write_p2z(args.input, args.output)
        except (ValueError, TypeError, OSError) as exc:
            print(f"P21 delivery error: {exc}", file=sys.stderr)
            return 1
        return 0

    if args.command == "resave-sfc":
        try:
            parsed = parse_sfc(args.input, strict=True)
            write_sfc(
                parsed,
                args.output,
                allow_external_references=args.allow_external_references,
            )
        except (ValueError, TypeError, OSError) as exc:
            print(f"SFC save error: {exc}", file=sys.stderr)
            return 1
        return 0

    if args.command == "bundle-sfc":
        try:
            report = write_sfc_bundle(
                args.input,
                args.output_directory,
                file_name=args.file_name,
                extra_files=args.extra_file,
            )
        except (ValueError, TypeError, OSError) as exc:
            print(f"SFC bundle error: {exc}", file=sys.stderr)
            return 1
        print(json.dumps(report, ensure_ascii=False))
        return 0

    if args.command == "plot":
        try:
            axes = plot(
                args.input,
                strict=args.strict,
                curve_segments=args.curve_segments,
                monochrome=args.monochrome,
                show_axes=args.show_axes,
            )
            if args.output is not None:
                axes.figure.savefig(
                    args.output,
                    dpi=args.dpi,
                    bbox_inches="tight",
                    facecolor=axes.figure.get_facecolor(),
                )
            if args.show or args.output is None:
                import matplotlib.pyplot as plt

                plt.show()
        except Exception as exc:
            print(f"plot error: {exc}", file=sys.stderr)
            return 1
        return 0

    parser.error(f"Unsupported command: {args.command}")
    return 2


__all__ = [
    "SfcDocument",
    "__version__",
    "edit_sfc",
    "edit_sfc_bundle",
    "hello_from_bin",
    "main",
    "new_sfc",
    "estimate_text_width",
    "parse_p21",
    "parse_sfc",
    "plot",
    "serialize_sfc",
    "serialize_p21",
    "to_dxf",
    "write_sfc",
    "write_p21",
    "write_p21_bundle",
    "write_p2z",
    "write_sfc_bundle",
    "validate_saf",
]
