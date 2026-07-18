"""Resolve composite curves and flatten SXF hatch features."""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from typing import Any, List, Optional, Tuple

from ezsxf._drawing import Affine, FillPrimitive, PathPrimitive, Point
from ezsxf._drawing_geometry import (
    apply,
    clip_hatch_lines,
    distance,
    point,
    without_duplicate_end,
)
from ezsxf._drawing_style import optional_int


def render_hatch(
    builder: Any,
    feature: Mapping[str, Any],
    transform: Affine,
    *,
    layer_override: Optional[int],
    parent_visible: bool,
) -> None:
    feature_id = int(feature["id"])
    reference = builder.hatch_references.get(feature_id)
    if reference is None:
        builder.warn_once(
            "unresolved-hatch-{0}".format(feature_id),
            "Hatch #{0} has no resolved composite-curve boundaries".format(feature_id),
        )
        return

    hatch_layer = optional_int(feature.get("style", {}).get("layer_code"))
    effective_layer = layer_override
    if effective_layer is None and hatch_layer not in (None, 0):
        effective_layer = hatch_layer

    outer_paths, outer_visible = _composite_paths(
        builder,
        int(reference["outer_definition_id"]),
        effective_layer,
        parent_visible,
    )
    outer = _join_composite(builder, outer_paths, feature_id, "outer")
    if not outer:
        return
    holes: List[List[Point]] = []
    boundary_paths = list(outer_paths)
    boundary_visibility = [outer_visible] * len(outer_paths)
    for definition_id in reference.get("inner_definition_ids", []):
        paths, visible = _composite_paths(
            builder, int(definition_id), effective_layer, parent_visible
        )
        ring = _join_composite(builder, paths, feature_id, "inner")
        if ring:
            holes.append(ring)
        boundary_paths.extend(paths)
        boundary_visibility.extend([visible] * len(paths))

    for path, visible in zip(boundary_paths, boundary_visibility):
        if visible:
            builder.drawing.paths.append(
                PathPrimitive(
                    points=tuple(apply(transform, value) for value in path.points),
                    closed=path.closed,
                    style=path.style,
                    source_id=feature_id,
                )
            )

    kind = str(feature["kind"])
    hatch_style = builder.styles.resolve(
        feature.get("style"),
        layer_override=effective_layer,
        parent_visible=parent_visible,
    )
    transformed_outer = tuple(apply(transform, value) for value in outer)
    transformed_holes = tuple(
        tuple(apply(transform, value) for value in hole) for hole in holes
    )
    if kind == "fill_area_style_colour":
        builder.drawing.fills.append(
            FillPrimitive(
                outer=transformed_outer,
                holes=transformed_holes,
                style=hatch_style,
                source_id=feature_id,
            )
        )
    elif kind == "fill_area_style_hatching":
        _render_line_patterns(
            builder,
            feature,
            feature_id,
            outer,
            holes,
            transform,
            hatch_layer,
            effective_layer,
            parent_visible,
        )
    elif kind == "externally_defined_hatch":
        builder.warn_once(
            "external-hatch",
            "Externally defined hatch patterns are represented by their visible boundaries",
        )
    elif kind == "fill_area_style_tiles":
        builder.warn_once(
            "tile-hatch",
            "Tile hatch patterns are represented by their visible boundaries",
        )


def _render_line_patterns(
    builder: Any,
    feature: Mapping[str, Any],
    feature_id: int,
    outer: Sequence[Point],
    holes: Sequence[Sequence[Point]],
    transform: Affine,
    hatch_layer: Optional[int],
    effective_layer: Optional[int],
    parent_visible: bool,
) -> None:
    for pattern in feature.get("patterns", []):
        pattern_style = builder.styles.resolve(
            {
                "layer_code": hatch_layer,
                "color_code": pattern.get("color_code"),
                "line_type_code": pattern.get("line_type_code"),
                "line_width_code": pattern.get("line_width_code"),
            },
            layer_override=effective_layer,
            parent_visible=parent_visible,
        )
        segments = clip_hatch_lines(
            outer,
            holes,
            point(pattern["start"]),
            float(pattern["spacing"]),
            float(pattern["angle_deg"]),
        )
        if len(segments) >= 200_000:
            builder.warn_once(
                "hatch-line-limit",
                "Hatch line generation was limited to 200000 segments",
            )
        for segment in segments[:200_000]:
            builder.drawing.paths.append(
                PathPrimitive(
                    points=tuple(apply(transform, value) for value in segment),
                    closed=False,
                    style=pattern_style,
                    source_id=feature_id,
                )
            )


def _composite_paths(
    builder: Any,
    definition_id: int,
    layer_override: Optional[int],
    parent_visible: bool,
) -> Tuple[List[PathPrimitive], bool]:
    definition = builder.composites.get(definition_id)
    marker = builder.features.get(definition_id)
    if definition is None or marker is None:
        builder.warn_once(
            "missing-composite-{0}".format(definition_id),
            "Composite-curve definition #{0} is missing".format(definition_id),
        )
        return [], False
    marker_style = marker.get("style", {})
    paths: List[PathPrimitive] = []
    for component_id in definition.get("component_ids", []):
        component = builder.features.get(int(component_id))
        if component is None:
            continue
        geometries = builder.simple_geometry(component)
        if geometries is None:
            continue
        raw_style = dict(component.get("style", {}))
        for name in ("color_code", "line_type_code", "line_width_code"):
            if marker_style.get(name) is not None:
                raw_style[name] = marker_style[name]
        style = builder.styles.resolve(
            raw_style,
            layer_override=layer_override,
            parent_visible=parent_visible,
        )
        for points, closed in geometries:
            paths.append(
                PathPrimitive(
                    points=tuple(points),
                    closed=closed,
                    style=style,
                    source_id=int(component_id),
                )
            )
    return paths, int(marker.get("visibility_flag", 0)) == 1


def _join_composite(
    builder: Any,
    paths: Sequence[PathPrimitive],
    hatch_id: int,
    boundary_name: str,
) -> List[Point]:
    if not paths:
        return []
    points = list(paths[0].points)
    if paths[0].closed:
        return without_duplicate_end(points)
    scale = max(
        1.0,
        max(abs(value) for path in paths for item in path.points for value in item),
    )
    tolerance = scale * 1.0e-7
    for path in paths[1:]:
        candidate = list(path.points)
        if not candidate:
            continue
        if distance(points[-1], candidate[0]) <= tolerance:
            points.extend(candidate[1:])
        elif distance(points[-1], candidate[-1]) <= tolerance:
            candidate.reverse()
            points.extend(candidate[1:])
        else:
            builder.warn_once(
                "hatch-gap-{0}-{1}".format(hatch_id, boundary_name),
                "Hatch #{0} {1} boundary contains a disconnected segment".format(
                    hatch_id, boundary_name
                ),
            )
            points.extend(candidate)
    if len(points) > 2 and distance(points[0], points[-1]) <= tolerance:
        points.pop()
    elif len(points) > 2:
        builder.warn_once(
            "hatch-open-{0}-{1}".format(hatch_id, boundary_name),
            "Hatch #{0} {1} boundary was closed during conversion".format(
                hatch_id, boundary_name
            ),
        )
    return points


__all__ = ["render_hatch"]
