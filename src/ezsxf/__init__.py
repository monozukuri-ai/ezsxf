from __future__ import annotations

import argparse
import json
import sys
from importlib.metadata import PackageNotFoundError, version
from typing import Sequence

from ezsxf._core import hello_from_bin, parse_p21, parse_sfc
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
    "__version__",
    "hello_from_bin",
    "main",
    "parse_p21",
    "parse_sfc",
    "plot",
    "to_dxf",
]
