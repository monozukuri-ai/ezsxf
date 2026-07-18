"""Resolve SFC style codes to backend-neutral drawing styles."""

from __future__ import annotations

from collections.abc import Mapping
from typing import Any, Dict, Optional, Tuple

from ezsxf._drawing import Color, RenderStyle

_PREDEFINED_COLORS: Dict[int, Color] = {
    1: (0, 0, 0),
    2: (255, 0, 0),
    3: (0, 255, 0),
    4: (0, 0, 255),
    5: (255, 255, 0),
    6: (255, 0, 255),
    7: (0, 255, 255),
    8: (255, 255, 255),
    9: (192, 0, 128),
    10: (192, 128, 64),
    11: (255, 128, 0),
    12: (128, 192, 128),
    13: (0, 128, 255),
    14: (128, 64, 255),
    15: (192, 192, 192),
    16: (128, 128, 128),
}

_COLOR_NAMES: Dict[str, Color] = {
    "black": _PREDEFINED_COLORS[1],
    "red": _PREDEFINED_COLORS[2],
    "green": _PREDEFINED_COLORS[3],
    "blue": _PREDEFINED_COLORS[4],
    "yellow": _PREDEFINED_COLORS[5],
    "magenta": _PREDEFINED_COLORS[6],
    "cyan": _PREDEFINED_COLORS[7],
    "white": _PREDEFINED_COLORS[8],
    "deeppink": _PREDEFINED_COLORS[9],
    "brown": _PREDEFINED_COLORS[10],
    "orange": _PREDEFINED_COLORS[11],
    "lightgreen": _PREDEFINED_COLORS[12],
    "lightblue": _PREDEFINED_COLORS[13],
    "lavender": _PREDEFINED_COLORS[14],
    "lightgray": _PREDEFINED_COLORS[15],
    "darkgray": _PREDEFINED_COLORS[16],
}

_PREDEFINED_LINE_TYPES: Dict[int, str] = {
    1: "continuous",
    2: "dashed",
    3: "dashed spaced",
    4: "long dashed dotted",
    5: "long dashed double-dotted",
    6: "long dashed triplicate-dotted",
    7: "dotted",
    8: "chain",
    9: "chain double dash",
    10: "dashed dotted",
    11: "double-dashed dotted",
    12: "dashed double-dotted",
    13: "double-dashed double-dotted",
    14: "dashed triplicate-dotted",
    15: "double-dashed triplicate-dotted",
}

_PREDEFINED_WIDTHS: Dict[int, float] = {
    1: 0.13,
    2: 0.18,
    3: 0.25,
    4: 0.35,
    5: 0.5,
    6: 0.7,
    7: 1.0,
    8: 1.4,
    9: 2.0,
}

# Reference pitches in millimetres from the SXF Ver.3.1 common predefined
# elements specification. Positive entries draw; negative entries are gaps.
LINE_TYPE_PATTERNS: Dict[str, Tuple[float, ...]] = {
    "continuous": (),
    "dashed": (6.0, -1.5),
    "dashed spaced": (6.0, -6.0),
    "long dashed dotted": (12.0, -1.5, 0.25, -1.5),
    "long dashed double-dotted": (12.0, -1.5, 0.25, -1.5, 0.25, -1.5),
    "long dashed triplicate-dotted": (
        12.0,
        -1.5,
        0.25,
        -1.5,
        0.25,
        -1.5,
        0.25,
        -1.5,
    ),
    "dotted": (0.25, -1.5),
    "chain": (12.0, -1.5, 3.5, -1.5),
    "chain double dash": (12.0, -1.5, 3.5, -1.5, 3.5, -1.5),
    "dashed dotted": (6.0, -1.5, 0.25, -1.5),
    "double-dashed dotted": (6.0, -1.5, 6.0, -1.5, 0.25, -1.5),
    "dashed double-dotted": (6.0, -1.5, 0.25, -1.5, 0.25, -1.5),
    "double-dashed double-dotted": (
        6.0,
        -1.5,
        6.0,
        -1.5,
        0.25,
        -1.5,
        0.25,
        -1.5,
    ),
    "dashed triplicate-dotted": (
        6.0,
        -1.5,
        0.25,
        -1.5,
        0.25,
        -1.5,
        0.25,
        -1.5,
    ),
    "double-dashed triplicate-dotted": (
        6.0,
        -1.5,
        6.0,
        -1.5,
        0.25,
        -1.5,
        0.25,
        -1.5,
        0.25,
        -1.5,
    ),
}


class StyleResolver:
    def __init__(
        self,
        model: Mapping[str, Any],
        features_by_id: Mapping[int, Mapping[str, Any]],
    ) -> None:
        self._features = features_by_id
        tables = model.get("code_tables", {})
        self._bindings: Dict[str, Dict[int, int]] = {}
        for table_name in (
            "layers",
            "line_types",
            "colors",
            "line_widths",
            "text_fonts",
        ):
            self._bindings[table_name] = {
                int(item["code"]): int(item["entity_id"])
                for item in tables.get(table_name, [])
            }

    def layer_visible(self, code: Optional[int]) -> bool:
        feature = self._bound_feature("layers", code)
        return feature is None or int(feature.get("visibility_flag", 1)) == 1

    def resolve(
        self,
        raw_style: Optional[Mapping[str, Any]],
        *,
        layer_override: Optional[int] = None,
        parent_visible: bool = True,
    ) -> RenderStyle:
        raw = raw_style or {}
        raw_layer = optional_int(raw.get("layer_code"))
        layer_code = layer_override if layer_override is not None else raw_layer
        layer_feature = self._bound_feature("layers", layer_code)
        if layer_feature is not None:
            layer = str(layer_feature.get("name") or "0")
            layer_visible = int(layer_feature.get("visibility_flag", 1)) == 1
        elif layer_code not in (None, 0):
            layer = "SXF_LAYER_{0}".format(layer_code)
            layer_visible = True
        else:
            layer = "0"
            layer_visible = True

        color = self._resolve_color(optional_int(raw.get("color_code")))
        line_type = self._resolve_line_type(optional_int(raw.get("line_type_code")))
        line_width = self._resolve_line_width(optional_int(raw.get("line_width_code")))
        font_feature = self._bound_feature(
            "text_fonts", optional_int(raw.get("font_code"))
        )
        font_name = (
            None
            if font_feature is None
            else str(font_feature.get("name") or "") or None
        )
        return RenderStyle(
            layer=layer,
            color=color,
            line_type=line_type,
            line_width_mm=line_width,
            font_name=font_name,
            visible=parent_visible and layer_visible,
        )

    def _bound_feature(
        self, table_name: str, code: Optional[int]
    ) -> Optional[Mapping[str, Any]]:
        if code is None:
            return None
        entity_id = self._bindings.get(table_name, {}).get(code)
        return None if entity_id is None else self._features.get(entity_id)

    def _resolve_color(self, code: Optional[int]) -> Color:
        if code in _PREDEFINED_COLORS:
            return _PREDEFINED_COLORS[code]
        feature = self._bound_feature("colors", code)
        if feature is None:
            return (0, 0, 0)
        if feature.get("kind") == "user_defined_colour":
            values = tuple(
                _clamp_color(int(feature.get(name, 0)))
                for name in ("red", "green", "blue")
            )
            return values  # type: ignore[return-value]
        name = " ".join(str(feature.get("name", "")).lower().split())
        return _COLOR_NAMES.get(name, (0, 0, 0))

    def _resolve_line_type(self, code: Optional[int]) -> str:
        if code in _PREDEFINED_LINE_TYPES:
            return _PREDEFINED_LINE_TYPES[code]
        feature = self._bound_feature("line_types", code)
        if feature is None:
            return "continuous"
        return str(feature.get("name") or "continuous")

    def _resolve_line_width(self, code: Optional[int]) -> float:
        if code in _PREDEFINED_WIDTHS:
            return _PREDEFINED_WIDTHS[code]
        feature = self._bound_feature("line_widths", code)
        if feature is None:
            return 0.25
        try:
            value = float(feature.get("width", 0.25))
        except (TypeError, ValueError):
            return 0.25
        return value if value > 0.0 else 0.25


def optional_int(value: Any) -> Optional[int]:
    if value is None:
        return None
    return int(value)


def _clamp_color(value: int) -> int:
    return max(0, min(255, value))


__all__ = ["LINE_TYPE_PATTERNS", "StyleResolver", "optional_int"]
