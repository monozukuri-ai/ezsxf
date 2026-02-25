from __future__ import annotations

import argparse
import json
import sys
from importlib.metadata import PackageNotFoundError, version
from typing import Sequence

from ezsxf._core import hello_from_bin, parse_p21, parse_sfc

try:
    __version__ = version("ezsxf")
except PackageNotFoundError:
    __version__ = "0.0.0"


def _build_cli_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="ezsxf",
        description="SXF (P21/SFC) parser CLI",
    )
    subcommands = parser.add_subparsers(dest="command")

    subcommands.add_parser("hello", help="Print extension smoke-test message")

    parse_cmd = subcommands.add_parser("parse", help="Parse P21/SFC and emit JSON")
    parse_cmd.add_argument("format", choices=["p21", "sfc"], help="Input file format")
    parse_cmd.add_argument("input", help="Path to input file")
    strict_group = parse_cmd.add_mutually_exclusive_group()
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
    parse_cmd.add_argument(
        "--pretty",
        action="store_true",
        help="Pretty-print JSON output",
    )

    return parser


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

    parser.error(f"Unsupported command: {args.command}")
    return 2


__all__ = ["__version__", "hello_from_bin", "parse_p21", "parse_sfc", "main"]
