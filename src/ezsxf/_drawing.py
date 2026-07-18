"""Public backend-neutral drawing primitives for SFC conversion."""

from __future__ import annotations

import math
import os
from collections.abc import Mapping
from dataclasses import dataclass, field
from typing import Any, List, Optional, Tuple, Union

from ezsxf._core import parse_p21, parse_sfc

Point = Tuple[float, float]
Color = Tuple[int, int, int]
Affine = Tuple[float, float, float, float, float, float]
DrawingInput = Union[str, bytes, os.PathLike[str], Mapping[str, Any]]
# Kept as a compatibility alias for the first SFC-only converter release.
SfcInput = DrawingInput


@dataclass(frozen=True)
class RenderStyle:
    layer: str
    color: Color
    line_type: str
    line_width_mm: float
    font_name: Optional[str] = None
    visible: bool = True


@dataclass(frozen=True)
class PathPrimitive:
    points: Tuple[Point, ...]
    closed: bool
    style: RenderStyle
    source_id: int


@dataclass(frozen=True)
class FillPrimitive:
    outer: Tuple[Point, ...]
    holes: Tuple[Tuple[Point, ...], ...]
    style: RenderStyle
    source_id: int


@dataclass(frozen=True)
class TextPrimitive:
    text: str
    anchor: Point
    height: float
    width: float
    angle_deg: float
    base_point: int
    direction: int
    style: RenderStyle
    source_id: int


@dataclass(frozen=True)
class MarkerPrimitive:
    position: Point
    marker_code: int
    scale: float
    style: RenderStyle
    source_id: int
    name: Optional[str] = None


@dataclass
class Drawing:
    paths: List[PathPrimitive] = field(default_factory=list)
    fills: List[FillPrimitive] = field(default_factory=list)
    texts: List[TextPrimitive] = field(default_factory=list)
    markers: List[MarkerPrimitive] = field(default_factory=list)
    warnings: List[str] = field(default_factory=list)
    background_color: Color = (255, 255, 255)

    def bounds(self) -> Optional[Tuple[float, float, float, float]]:
        points: List[Point] = []
        for path in self.paths:
            points.extend(path.points)
        for fill in self.fills:
            points.extend(fill.outer)
            for hole in fill.holes:
                points.extend(hole)
        for marker in self.markers:
            points.append(marker.position)
        for text in self.texts:
            points.append(text.anchor)
            radius = math.hypot(text.width, text.height)
            points.append((text.anchor[0] - radius, text.anchor[1] - radius))
            points.append((text.anchor[0] + radius, text.anchor[1] + radius))
        if not points:
            return None
        xs = [point[0] for point in points]
        ys = [point[1] for point in points]
        return min(xs), min(ys), max(xs), max(ys)


def build_drawing(
    source: DrawingInput,
    *,
    strict: bool = True,
    curve_segments: int = 64,
) -> Drawing:
    """Parse *source* and flatten its P21/SFC drawing into primitives."""

    if curve_segments < 8:
        raise ValueError("curve_segments must be at least 8")
    parsed = _coerce_parsed_drawing(source, strict=strict)
    if parsed.get("format") == "p21":
        from ezsxf._p21_drawing import P21DrawingBuilder

        return P21DrawingBuilder(parsed, curve_segments=curve_segments).build()
    from ezsxf._drawing_builder import DrawingBuilder

    return DrawingBuilder(parsed, curve_segments=curve_segments).build()


def _coerce_parsed_drawing(source: DrawingInput, *, strict: bool) -> Mapping[str, Any]:
    if isinstance(source, Mapping):
        parsed = source
    else:
        parser_input: Union[str, bytes]
        if isinstance(source, os.PathLike):
            parser_input = os.fspath(source)
        else:
            parser_input = source
        parser = parse_p21 if _looks_like_p21(source) else parse_sfc
        parsed = parser(parser_input, strict=strict)
    if parsed.get("format") not in {"p21", "sfc"}:
        raise ValueError("drawing conversion requires parsed P21 or SFC input")
    if parsed.get("format") == "sfc" and not isinstance(parsed.get("model"), Mapping):
        raise ValueError("parsed SFC input does not contain a resolved model")
    return parsed


def _looks_like_p21(source: Union[str, bytes, os.PathLike[str]]) -> bool:
    if isinstance(source, os.PathLike):
        return os.fspath(source).lower().endswith(".p21")
    if isinstance(source, bytes):
        header = source[:4096].decode("latin-1", errors="ignore").lower()
        return "ap202_mode" in header
    stripped = source.lstrip()
    if "\n" not in stripped[:4096] and "\r" not in stripped[:4096]:
        if stripped.lower().endswith(".p21"):
            return True
    header = stripped[:4096].lower()
    return "ap202_mode" in header


__all__ = [
    "Drawing",
    "DrawingInput",
    "FillPrimitive",
    "MarkerPrimitive",
    "PathPrimitive",
    "RenderStyle",
    "TextPrimitive",
    "build_drawing",
]
