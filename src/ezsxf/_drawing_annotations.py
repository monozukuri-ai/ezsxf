"""Flatten SXF dimensions, labels, and balloons into drawing primitives."""

from __future__ import annotations

import math
from collections.abc import Iterable, Mapping, Sequence
from typing import Any, Tuple

from ezsxf._drawing import Affine, PathPrimitive, Point, RenderStyle
from ezsxf._drawing_geometry import apply, point, sample_arc, subtract


def render_annotation(
    builder: Any,
    feature: Mapping[str, Any],
    transform: Affine,
    style: RenderStyle,
) -> None:
    kind = str(feature["kind"])
    feature_id = int(feature["id"])

    if kind in {"linear_dim", "radius_dim", "diameter_dim"}:
        start = point(feature["start"])
        end = point(feature["end"])
        _append_path(builder, [start, end], False, style, feature_id, transform)
    elif kind in {"curve_dim", "angular_dim"}:
        points = sample_arc(
            point(feature["center"]),
            float(feature["radius"]),
            float(feature["start_angle_deg"]),
            float(feature["end_angle_deg"]),
            0,
            builder.curve_segments,
        )
        _append_path(builder, points, False, style, feature_id, transform)

    for key in ("extension_line1", "extension_line2"):
        extension = feature.get(key)
        if (
            isinstance(extension, Mapping)
            and int(extension.get("present_flag", 0)) == 1
        ):
            _append_path(
                builder,
                [point(extension["start"]), point(extension["end"])],
                False,
                style,
                feature_id,
                transform,
            )

    if kind in {"linear_dim", "radius_dim", "diameter_dim"}:
        start = point(feature["start"])
        end = point(feature["end"])
        arrows: Iterable[Tuple[str, Point]]
        if kind == "linear_dim":
            arrows = (
                ("arrow1", subtract(end, start)),
                ("arrow2", subtract(start, end)),
            )
        elif kind == "radius_dim":
            arrows = (("arrow", subtract(start, end)),)
        else:
            arrows = (
                ("arrow1", subtract(end, start)),
                ("arrow2", subtract(start, end)),
            )
        for key, vector in arrows:
            arrow = feature.get(key)
            if isinstance(arrow, Mapping):
                _append_arrow(builder, arrow, vector, style, feature_id, transform)
    elif kind in {"curve_dim", "angular_dim"}:
        center = point(feature["center"])
        for key, sign in (("arrow1", 1.0), ("arrow2", -1.0)):
            arrow = feature.get(key)
            if not isinstance(arrow, Mapping):
                continue
            position = point(arrow["position"])
            radial = subtract(position, center)
            tangent = (-radial[1] * sign, radial[0] * sign)
            _append_arrow(builder, arrow, tangent, style, feature_id, transform)

    if kind in {"label", "balloon"}:
        vertices = [point(value) for value in feature.get("vertices", [])]
        if len(vertices) >= 2:
            _append_path(builder, vertices, False, style, feature_id, transform)
            arrow = feature.get("arrow", {})
            if int(arrow.get("code", 0)) != 0:
                _append_arrow(
                    builder,
                    {
                        "position": {"x": vertices[0][0], "y": vertices[0][1]},
                        "direction_flag": 2,
                        "scale": arrow.get("scale", 1.0),
                    },
                    subtract(vertices[1], vertices[0]),
                    style,
                    feature_id,
                    transform,
                )
        if kind == "balloon":
            circle = sample_arc(
                point(feature["center"]),
                float(feature["radius"]),
                0.0,
                360.0,
                0,
                builder.curve_segments,
            )[:-1]
            _append_path(builder, circle, True, style, feature_id, transform)

    text = feature.get("text")
    if isinstance(text, Mapping) and int(text.get("present_flag", 0)) == 1:
        raw_style = dict(feature.get("style", {}))
        raw_style["font_code"] = text.get("font_code")
        font_style = builder.styles.resolve(raw_style)
        text_style = RenderStyle(
            layer=style.layer,
            color=style.color,
            line_type=style.line_type,
            line_width_mm=style.line_width_mm,
            font_name=font_style.font_name,
            visible=style.visible,
        )
        builder.drawing.texts.append(
            builder.make_text(feature, text, text_style, transform)
        )


def _append_arrow(
    builder: Any,
    arrow: Mapping[str, Any],
    vector: Point,
    style: RenderStyle,
    source_id: int,
    transform: Affine,
) -> None:
    if int(arrow.get("direction_flag", 1)) == 0:
        return
    length = math.hypot(*vector)
    if length <= 1.0e-12:
        return
    unit = (vector[0] / length, vector[1] / length)
    if int(arrow.get("direction_flag", 1)) == 1:
        unit = (-unit[0], -unit[1])
    normal = (-unit[1], unit[0])
    size = max(
        float(arrow.get("scale", 1.0)) * 10.0,
        style.line_width_mm * 3.0,
    )
    tip = point(arrow.get("position", {"x": 0.0, "y": 0.0}))
    left = (
        tip[0] - unit[0] * size + normal[0] * size * 0.35,
        tip[1] - unit[1] * size + normal[1] * size * 0.35,
    )
    right = (
        tip[0] - unit[0] * size - normal[0] * size * 0.35,
        tip[1] - unit[1] * size - normal[1] * size * 0.35,
    )
    _append_path(builder, [left, tip, right], False, style, source_id, transform)


def _append_path(
    builder: Any,
    points: Sequence[Point],
    closed: bool,
    style: RenderStyle,
    source_id: int,
    transform: Affine,
) -> None:
    if len(points) < 2:
        return
    builder.drawing.paths.append(
        PathPrimitive(
            points=tuple(apply(transform, value) for value in points),
            closed=closed,
            style=style,
            source_id=source_id,
        )
    )


__all__ = ["render_annotation"]
