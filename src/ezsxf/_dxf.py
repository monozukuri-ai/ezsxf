"""Dependency-free ASCII DXF writer for flattened SFC drawings."""

from __future__ import annotations

import math
import os
from pathlib import Path
from typing import Dict, Iterable, List, Mapping, Optional, Sequence, Tuple, Union

from ezsxf._drawing import (
    Drawing,
    FillPrimitive,
    MarkerPrimitive,
    PathPrimitive,
    RenderStyle,
    SfcInput,
    TextPrimitive,
    build_drawing,
)
from ezsxf._drawing_style import LINE_TYPE_PATTERNS

OutputPath = Union[str, os.PathLike[str]]

_DXF_LINE_TYPE_NAMES: Mapping[str, str] = {
    "continuous": "CONTINUOUS",
    "dashed": "SXF_DASHED",
    "dashed spaced": "SXF_DASHED_SPACED",
    "long dashed dotted": "SXF_LONG_DASH_DOT",
    "long dashed double-dotted": "SXF_LONG_DASH_2DOT",
    "long dashed triplicate-dotted": "SXF_LONG_DASH_3DOT",
    "dotted": "SXF_DOTTED",
    "chain": "SXF_CHAIN",
    "chain double dash": "SXF_CHAIN_2DASH",
    "dashed dotted": "SXF_DASH_DOT",
    "double-dashed dotted": "SXF_2DASH_DOT",
    "dashed double-dotted": "SXF_DASH_2DOT",
    "double-dashed double-dotted": "SXF_2DASH_2DOT",
    "dashed triplicate-dotted": "SXF_DASH_3DOT",
    "double-dashed triplicate-dotted": "SXF_2DASH_3DOT",
}

_LINE_TYPES: Mapping[str, Tuple[str, Sequence[float]]] = {
    _DXF_LINE_TYPE_NAMES[name]: (name, pattern)
    for name, pattern in LINE_TYPE_PATTERNS.items()
}

_DXF_LINE_WEIGHTS = (
    0,
    5,
    9,
    13,
    15,
    18,
    20,
    25,
    30,
    35,
    40,
    50,
    53,
    60,
    70,
    80,
    90,
    100,
    106,
    120,
    140,
    158,
    200,
    211,
)


def to_dxf(
    source: SfcInput,
    output: Optional[OutputPath] = None,
    *,
    strict: bool = True,
    curve_segments: int = 64,
) -> str:
    """Convert SFC input to an ASCII DXF string and optionally write a file."""

    drawing = build_drawing(source, strict=strict, curve_segments=curve_segments)
    dxf = drawing_to_dxf(drawing)
    if output is not None:
        with Path(output).open("w", encoding="utf-8", newline="") as stream:
            stream.write(dxf)
    return dxf


def drawing_to_dxf(drawing: Drawing) -> str:
    """Serialize backend-neutral drawing primitives as an AutoCAD 2007 DXF."""

    layers = _collect_layers(drawing)
    lines: List[str] = []

    _pairs(
        lines,
        (0, "SECTION"),
        (2, "HEADER"),
        (9, "$ACADVER"),
        (1, "AC1021"),
        (9, "$DWGCODEPAGE"),
        (3, "UTF-8"),
        (9, "$INSUNITS"),
        (70, 4),
        (9, "$MEASUREMENT"),
        (70, 1),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "TABLES"),
    )
    _write_line_type_table(lines)
    _write_layer_table(lines, layers)
    _write_text_style_table(lines)
    _pairs(
        lines,
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "BLOCKS"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "ENTITIES"),
    )

    for fill in drawing.fills:
        _write_fill(lines, fill, layers)
    for path in drawing.paths:
        _write_path(lines, path, layers)
    for text in drawing.texts:
        _write_text(lines, text, layers)
    for marker in drawing.markers:
        _write_marker(lines, marker, layers)

    _pairs(lines, (0, "ENDSEC"), (0, "EOF"))
    return "\r\n".join(lines) + "\r\n"


def _collect_layers(drawing: Drawing) -> Dict[str, Tuple[str, bool]]:
    original: Dict[str, bool] = {"0": True}
    primitives: Iterable[object] = (
        list(drawing.fills)
        + list(drawing.paths)
        + list(drawing.texts)
        + list(drawing.markers)
    )
    for primitive in primitives:
        style = primitive.style  # type: ignore[attr-defined]
        original[style.layer] = original.get(style.layer, True) and style.visible

    used: Dict[str, str] = {}
    result: Dict[str, Tuple[str, bool]] = {}
    for name, visible in original.items():
        candidate = _sanitize_layer_name(name)
        base = candidate
        suffix = 2
        while candidate.casefold() in used and used[candidate.casefold()] != name:
            suffix_text = "_{0}".format(suffix)
            candidate = base[: 255 - len(suffix_text)] + suffix_text
            suffix += 1
        used[candidate.casefold()] = name
        result[name] = (candidate, visible)
    return result


def _write_line_type_table(lines: List[str]) -> None:
    _pairs(lines, (0, "TABLE"), (2, "LTYPE"), (5, "FFFF1"), (70, len(_LINE_TYPES)))
    for name, (description, pattern) in _LINE_TYPES.items():
        _pairs(
            lines,
            (0, "LTYPE"),
            (100, "AcDbSymbolTableRecord"),
            (100, "AcDbLinetypeTableRecord"),
            (2, name),
            (70, 0),
            (3, description),
            (72, 65),
            (73, len(pattern)),
            (40, sum(abs(value) for value in pattern)),
        )
        for value in pattern:
            _pairs(lines, (49, value), (74, 0))
    _pairs(lines, (0, "ENDTAB"))


def _write_layer_table(
    lines: List[str], layers: Mapping[str, Tuple[str, bool]]
) -> None:
    _pairs(lines, (0, "TABLE"), (2, "LAYER"), (5, "FFFF2"), (70, len(layers)))
    for dxf_name, visible in layers.values():
        _pairs(
            lines,
            (0, "LAYER"),
            (100, "AcDbSymbolTableRecord"),
            (100, "AcDbLayerTableRecord"),
            (2, dxf_name),
            (70, 0),
            (62, 7 if visible else -7),
            (6, "CONTINUOUS"),
        )
    _pairs(lines, (0, "ENDTAB"))


def _write_text_style_table(lines: List[str]) -> None:
    _pairs(
        lines,
        (0, "TABLE"),
        (2, "STYLE"),
        (5, "FFFF3"),
        (70, 1),
        (0, "STYLE"),
        (100, "AcDbSymbolTableRecord"),
        (100, "AcDbTextStyleTableRecord"),
        (2, "STANDARD"),
        (70, 0),
        (40, 0.0),
        (41, 1.0),
        (50, 0.0),
        (71, 0),
        (42, 2.5),
        (3, "txt"),
        (4, ""),
        (0, "ENDTAB"),
    )


def _write_path(
    lines: List[str],
    path: PathPrimitive,
    layers: Mapping[str, Tuple[str, bool]],
) -> None:
    points = list(path.points)
    if path.closed and len(points) > 2 and _near(points[0], points[-1]):
        points.pop()
    if len(points) < 2:
        return
    if len(points) == 2 and not path.closed:
        _pairs(lines, (0, "LINE"))
        _write_common_entity(lines, path.style, layers)
        _pairs(
            lines,
            (100, "AcDbLine"),
            (10, points[0][0]),
            (20, points[0][1]),
            (30, 0.0),
            (11, points[1][0]),
            (21, points[1][1]),
            (31, 0.0),
        )
        return

    _pairs(lines, (0, "LWPOLYLINE"))
    _write_common_entity(lines, path.style, layers)
    _pairs(
        lines,
        (100, "AcDbPolyline"),
        (90, len(points)),
        (70, 1 if path.closed else 0),
    )
    for point in points:
        _pairs(lines, (10, point[0]), (20, point[1]))


def _write_fill(
    lines: List[str],
    fill: FillPrimitive,
    layers: Mapping[str, Tuple[str, bool]],
) -> None:
    rings = [fill.outer] + list(fill.holes)
    rings = [ring for ring in rings if len(ring) >= 3]
    if not rings:
        return
    _pairs(lines, (0, "HATCH"))
    _write_common_entity(lines, fill.style, layers)
    _pairs(
        lines,
        (100, "AcDbHatch"),
        (10, 0.0),
        (20, 0.0),
        (30, 0.0),
        (210, 0.0),
        (220, 0.0),
        (230, 1.0),
        (2, "SOLID"),
        (70, 1),
        (71, 0),
        (91, len(rings)),
    )
    for index, ring in enumerate(rings):
        points = list(ring)
        if len(points) > 2 and _near(points[0], points[-1]):
            points.pop()
        _pairs(
            lines,
            (92, 3 if index == 0 else 2),
            (72, 0),
            (73, 1),
            (93, len(points)),
        )
        for point in points:
            _pairs(lines, (10, point[0]), (20, point[1]))
        _pairs(lines, (97, 0))
    _pairs(lines, (75, 0), (76, 1), (98, 0))


def _write_text(
    lines: List[str],
    text: TextPrimitive,
    layers: Mapping[str, Tuple[str, bool]],
) -> None:
    if not text.text or text.height <= 0.0:
        return
    horizontal = max(0, min(2, (text.base_point - 1) % 3))
    vertical = max(1, min(3, (text.base_point - 1) // 3 + 1))
    estimated_width = max(text.height * 0.6 * len(text.text), 1.0e-12)
    width_factor = max(0.01, min(100.0, text.width / estimated_width))
    _pairs(lines, (0, "TEXT"))
    _write_common_entity(lines, text.style, layers)
    _pairs(
        lines,
        (100, "AcDbText"),
        (10, text.anchor[0]),
        (20, text.anchor[1]),
        (30, 0.0),
        (40, text.height),
        (1, _clean_text(text.text)),
        (50, text.angle_deg % 360.0),
        (41, width_factor),
        (7, "STANDARD"),
        (72, horizontal),
        (11, text.anchor[0]),
        (21, text.anchor[1]),
        (31, 0.0),
        (73, vertical),
    )


def _write_marker(
    lines: List[str],
    marker: MarkerPrimitive,
    layers: Mapping[str, Tuple[str, bool]],
) -> None:
    _pairs(lines, (0, "POINT"))
    _write_common_entity(lines, marker.style, layers)
    _pairs(
        lines,
        (100, "AcDbPoint"),
        (10, marker.position[0]),
        (20, marker.position[1]),
        (30, 0.0),
    )


def _write_common_entity(
    lines: List[str],
    style: RenderStyle,
    layers: Mapping[str, Tuple[str, bool]],
) -> None:
    dxf_layer = layers.get(style.layer, ("0", True))[0]
    _pairs(
        lines,
        (100, "AcDbEntity"),
        (8, dxf_layer),
        (6, _line_type_name(style.line_type)),
        (420, _true_color(style.color)),
        (370, _line_weight(style.line_width_mm)),
    )


def _line_type_name(name: str) -> str:
    normalized = " ".join(name.lower().split())
    predefined = _DXF_LINE_TYPE_NAMES.get(normalized)
    if predefined is not None:
        return predefined
    if (
        "dotted" in normalized
        and "dash" not in normalized
        and "chain" not in normalized
    ):
        return "SXF_DOTTED"
    if "chain" in normalized or ("dash" in normalized and "dotted" in normalized):
        return "SXF_DASH_DOT"
    if "dash" in normalized:
        return "SXF_DASHED"
    return "CONTINUOUS"


def _line_weight(width_mm: float) -> int:
    requested = max(0, int(round(width_mm * 100.0)))
    return min(_DXF_LINE_WEIGHTS, key=lambda value: abs(value - requested))


def _true_color(color: Tuple[int, int, int]) -> int:
    return (color[0] << 16) | (color[1] << 8) | color[2]


def _sanitize_layer_name(name: str) -> str:
    forbidden = set('<>/\\":;?*|=,')
    cleaned = "".join(
        "_" if character in forbidden or ord(character) < 32 else character
        for character in str(name)
    ).strip()
    return (cleaned or "0")[:255]


def _clean_text(value: str) -> str:
    return str(value).replace("\x00", "").replace("\r", " ").replace("\n", " ")


def _near(point1: Tuple[float, float], point2: Tuple[float, float]) -> bool:
    scale = max(1.0, *(abs(value) for value in point1 + point2))
    return math.hypot(point1[0] - point2[0], point1[1] - point2[1]) <= scale * 1.0e-9


def _pairs(lines: List[str], *pairs: Tuple[int, object]) -> None:
    for code, value in pairs:
        lines.append(str(code))
        if isinstance(value, float):
            lines.append(_format_float(value))
        else:
            lines.append(str(value))


def _format_float(value: float) -> str:
    if abs(value) < 1.0e-14:
        return "0.0"
    return format(value, ".15g")


__all__ = ["drawing_to_dxf", "to_dxf"]
