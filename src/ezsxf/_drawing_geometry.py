"""Affine transforms and curve approximation for drawing backends."""

from __future__ import annotations

import math
from collections.abc import Mapping, Sequence
from typing import Any, Callable, List, Optional, Tuple

from ezsxf._drawing import Affine, Point

IDENTITY: Affine = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0)
Geometry = Tuple[List[Point], bool]
WarningCallback = Callable[[str, str], None]


def feature_geometry(
    feature: Mapping[str, Any],
    curve_segments: int,
    warn_once: WarningCallback,
) -> Optional[List[Geometry]]:
    kind = feature.get("kind")
    if kind == "line":
        return [([point(feature["start"]), point(feature["end"])], False)]
    if kind == "polyline":
        points = [point(value) for value in feature.get("points", [])]
        closed = len(points) > 2 and near(points[0], points[-1])
        if closed:
            points = points[:-1]
        return [(points, closed)]
    if kind == "circle":
        center = point(feature["center"])
        radius = float(feature["radius"])
        points = [
            (
                center[0] + radius * math.cos(2.0 * math.pi * index / curve_segments),
                center[1] + radius * math.sin(2.0 * math.pi * index / curve_segments),
            )
            for index in range(curve_segments)
        ]
        return [(points, True)]
    if kind == "arc":
        return [
            (
                sample_arc(
                    point(feature["center"]),
                    float(feature["radius"]),
                    float(feature["start_angle_deg"]),
                    float(feature["end_angle_deg"]),
                    int(feature["direction_flag"]),
                    curve_segments,
                ),
                False,
            )
        ]
    if kind == "ellipse":
        return [
            (
                sample_ellipse(
                    point(feature["center"]),
                    float(feature["radius_x"]),
                    float(feature["radius_y"]),
                    float(feature["rotation_angle_deg"]),
                    0.0,
                    360.0,
                    0,
                    curve_segments,
                    closed=True,
                ),
                True,
            )
        ]
    if kind == "ellipse_arc":
        return [
            (
                sample_ellipse(
                    point(feature["center"]),
                    float(feature["radius_x"]),
                    float(feature["radius_y"]),
                    float(feature["rotation_angle_deg"]),
                    float(feature["start_angle_deg"]),
                    float(feature["end_angle_deg"]),
                    int(feature["direction_flag"]),
                    curve_segments,
                    closed=False,
                ),
                False,
            )
        ]
    if kind == "spline":
        controls = [point(value) for value in feature.get("points", [])]
        points = sample_spline(controls, curve_segments)
        if points is None:
            warn_once(
                "spline-control-layout",
                "Spline control-point count is not 3n+1; control polygon was used",
            )
            points = controls
        closed = int(feature.get("open_close", 1)) == 0
        if closed and len(points) > 1 and near(points[0], points[-1]):
            points = points[:-1]
        return [(points, closed)]
    if kind == "clothoid":
        return [
            (
                sample_clothoid(
                    point(feature["base"]),
                    float(feature["parameter"]),
                    int(feature["direction_flag"]),
                    float(feature["angle_deg"]),
                    float(feature["start_length"]),
                    float(feature["end_length"]),
                    curve_segments,
                ),
                False,
            )
        ]
    return None


def sample_arc(
    center: Point,
    radius: float,
    start_angle_deg: float,
    end_angle_deg: float,
    direction_flag: int,
    curve_segments: int,
) -> List[Point]:
    if direction_flag == 1:
        sweep = -((start_angle_deg - end_angle_deg) % 360.0)
    else:
        sweep = (end_angle_deg - start_angle_deg) % 360.0
    if abs(sweep) <= 1.0e-12:
        sweep = -360.0 if direction_flag == 1 else 360.0
    count = max(2, int(math.ceil(curve_segments * abs(sweep) / 360.0)))
    return [
        (
            center[0]
            + radius * math.cos(math.radians(start_angle_deg + sweep * index / count)),
            center[1]
            + radius * math.sin(math.radians(start_angle_deg + sweep * index / count)),
        )
        for index in range(count + 1)
    ]


def sample_ellipse(
    center: Point,
    radius_x: float,
    radius_y: float,
    rotation_angle_deg: float,
    start_angle_deg: float,
    end_angle_deg: float,
    direction_flag: int,
    curve_segments: int,
    *,
    closed: bool,
) -> List[Point]:
    if closed:
        sweep = 360.0
        count = curve_segments
    elif direction_flag == 1:
        sweep = -((start_angle_deg - end_angle_deg) % 360.0) or -360.0
        count = max(2, int(math.ceil(curve_segments * abs(sweep) / 360.0)))
    else:
        sweep = (end_angle_deg - start_angle_deg) % 360.0 or 360.0
        count = max(2, int(math.ceil(curve_segments * abs(sweep) / 360.0)))
    rotation = math.radians(rotation_angle_deg)
    cos_rotation = math.cos(rotation)
    sin_rotation = math.sin(rotation)
    limit = count if closed else count + 1
    points: List[Point] = []
    for index in range(limit):
        angle = math.radians(start_angle_deg + sweep * index / count)
        x = radius_x * math.cos(angle)
        y = radius_y * math.sin(angle)
        points.append(
            (
                center[0] + x * cos_rotation - y * sin_rotation,
                center[1] + x * sin_rotation + y * cos_rotation,
            )
        )
    return points


def sample_spline(
    controls: Sequence[Point], curve_segments: int
) -> Optional[List[Point]]:
    if len(controls) < 4 or (len(controls) - 1) % 3 != 0:
        return None
    per_segment = max(4, curve_segments // 4)
    output: List[Point] = []
    for offset in range(0, len(controls) - 1, 3):
        p0, p1, p2, p3 = controls[offset : offset + 4]
        for index in range(per_segment + 1):
            if output and index == 0:
                continue
            t = index / per_segment
            u = 1.0 - t
            output.append(
                (
                    u**3 * p0[0]
                    + 3.0 * u * u * t * p1[0]
                    + 3.0 * u * t * t * p2[0]
                    + t**3 * p3[0],
                    u**3 * p0[1]
                    + 3.0 * u * u * t * p1[1]
                    + 3.0 * u * t * t * p2[1]
                    + t**3 * p3[1],
                )
            )
    return output


def sample_clothoid(
    base: Point,
    parameter: float,
    direction_flag: int,
    angle_deg: float,
    start_length: float,
    end_length: float,
    curve_segments: int,
) -> List[Point]:
    start = min(start_length, end_length)
    end = max(start_length, end_length)
    span = end - start
    count = min(
        max(2, int(math.ceil(curve_segments * span / parameter))),
        curve_segments * 16,
    )
    warmup_count = min(
        max(1, int(math.ceil(curve_segments * start / parameter))),
        curve_segments * 16,
    )
    lengths = (
        [0.0]
        if start <= 1.0e-12
        else [start * index / warmup_count for index in range(warmup_count + 1)]
    )
    if span > 1.0e-12:
        lengths.extend(start + span * index / count for index in range(1, count + 1))
    sign = -1.0 if direction_flag == 1 else 1.0
    x = 0.0
    y = 0.0
    samples: List[Tuple[float, Point]] = [(0.0, (0.0, 0.0))]
    previous = 0.0
    for length in lengths[1:]:
        delta = length - previous
        midpoint = previous + delta * 0.5
        tangent = sign * midpoint * midpoint / (2.0 * parameter * parameter)
        x += math.cos(tangent) * delta
        y += math.sin(tangent) * delta
        samples.append((length, (x, y)))
        previous = length
    rotation = math.radians(angle_deg)
    cos_rotation = math.cos(rotation)
    sin_rotation = math.sin(rotation)
    points = [
        (
            base[0] + value[0] * cos_rotation - value[1] * sin_rotation,
            base[1] + value[0] * sin_rotation + value[1] * cos_rotation,
        )
        for length, value in samples
        if length + 1.0e-12 >= start
    ]
    if start_length > end_length:
        points.reverse()
    return points


def clip_hatch_lines(
    outer: Sequence[Point],
    holes: Sequence[Sequence[Point]],
    start: Point,
    spacing: float,
    angle_deg: float,
) -> List[Tuple[Point, Point]]:
    if spacing <= 0.0 or len(outer) < 3:
        return []
    radians = math.radians(angle_deg)
    direction = (math.cos(radians), math.sin(radians))
    normal = (-direction[1], direction[0])
    rings = [outer] + list(holes)
    normal_values = [dot(value, normal) for ring in rings for value in ring]
    if not normal_values:
        return []
    minimum = min(normal_values)
    maximum = max(normal_values)
    base = dot(start, normal)
    first = int(math.floor((minimum - base) / spacing))
    last = min(int(math.ceil((maximum - base) / spacing)), first + 200_000)

    output: List[Tuple[Point, Point]] = []
    for index in range(first, last + 1):
        offset = base + index * spacing
        intersections: List[float] = []
        for ring in rings:
            if len(ring) < 3:
                continue
            closed_ring = list(ring)
            for point1, point2 in zip(closed_ring, closed_ring[1:] + closed_ring[:1]):
                normal1 = dot(point1, normal)
                normal2 = dot(point2, normal)
                if (normal1 <= offset < normal2) or (normal2 <= offset < normal1):
                    ratio = (offset - normal1) / (normal2 - normal1)
                    along1 = dot(point1, direction)
                    along2 = dot(point2, direction)
                    intersections.append(along1 + ratio * (along2 - along1))
        intersections.sort()
        deduplicated: List[float] = []
        for value in intersections:
            if not deduplicated or abs(value - deduplicated[-1]) > 1.0e-9:
                deduplicated.append(value)
        for begin, finish in zip(deduplicated[0::2], deduplicated[1::2]):
            output.append(
                (
                    (
                        direction[0] * begin + normal[0] * offset,
                        direction[1] * begin + normal[1] * offset,
                    ),
                    (
                        direction[0] * finish + normal[0] * offset,
                        direction[1] * finish + normal[1] * offset,
                    ),
                )
            )
    return output


def compose(parent: Affine, local: Affine) -> Affine:
    a, b, c, d, e, f = parent
    la, lb, lc, ld, le, lf = local
    return (
        a * la + c * lb,
        b * la + d * lb,
        a * lc + c * ld,
        b * lc + d * ld,
        a * le + c * lf + e,
        b * le + d * lf + f,
    )


def apply(transform: Affine, value: Point) -> Point:
    a, b, c, d, e, f = transform
    return a * value[0] + c * value[1] + e, b * value[0] + d * value[1] + f


def apply_vector(transform: Affine, vector: Point) -> Point:
    a, b, c, d, _, _ = transform
    return a * vector[0] + c * vector[1], b * vector[0] + d * vector[1]


def average_scale(transform: Affine) -> float:
    return (
        math.hypot(transform[0], transform[1]) + math.hypot(transform[2], transform[3])
    ) * 0.5


def point(value: Mapping[str, Any]) -> Point:
    return float(value["x"]), float(value["y"])


def distance(point1: Point, point2: Point) -> float:
    return math.hypot(point1[0] - point2[0], point1[1] - point2[1])


def near(point1: Point, point2: Point) -> bool:
    scale = max(
        1.0,
        abs(point1[0]),
        abs(point1[1]),
        abs(point2[0]),
        abs(point2[1]),
    )
    return distance(point1, point2) <= scale * 1.0e-9


def without_duplicate_end(points: List[Point]) -> List[Point]:
    if len(points) > 2 and near(points[0], points[-1]):
        return points[:-1]
    return points


def subtract(point1: Point, point2: Point) -> Point:
    return point1[0] - point2[0], point1[1] - point2[1]


def dot(point1: Point, point2: Point) -> float:
    return point1[0] * point2[0] + point1[1] * point2[1]


__all__ = [
    "IDENTITY",
    "apply",
    "apply_vector",
    "average_scale",
    "clip_hatch_lines",
    "compose",
    "distance",
    "dot",
    "feature_geometry",
    "near",
    "point",
    "sample_arc",
    "subtract",
    "without_duplicate_end",
]
