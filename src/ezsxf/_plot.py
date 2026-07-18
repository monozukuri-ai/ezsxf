"""Optional matplotlib backend for SFC drawing primitives."""

from __future__ import annotations

from collections import defaultdict
from typing import Any, DefaultDict, List, Optional, Tuple, Union

from ezsxf._drawing import Drawing, RenderStyle, SfcInput, build_drawing
from ezsxf._drawing_style import LINE_TYPE_PATTERNS

PlotInput = Union[SfcInput, Drawing]
MatplotlibLineStyle = Union[str, Tuple[float, Tuple[float, ...]]]


def plot(
    source: PlotInput,
    *,
    ax: Optional[Any] = None,
    strict: bool = True,
    curve_segments: int = 64,
    monochrome: bool = False,
    linewidth_scale: float = 1.0,
    background: Optional[Tuple[int, int, int]] = None,
    font_family: Optional[str] = None,
    show_axes: bool = False,
    show: bool = False,
) -> Any:
    """Draw SFC input on a matplotlib ``Axes`` and return that axes."""

    try:
        import matplotlib.pyplot as plt
        from matplotlib.collections import LineCollection
        from matplotlib.font_manager import FontProperties
        from matplotlib.path import Path as MplPath
        from matplotlib.patches import PathPatch
        from matplotlib.textpath import TextPath
        from matplotlib.transforms import Affine2D
    except ImportError as exc:  # pragma: no cover - depends on optional package
        raise ImportError(
            "matplotlib is required for plotting; install ezsxf[plot]"
        ) from exc

    if linewidth_scale <= 0.0:
        raise ValueError("linewidth_scale must be positive")
    drawing = (
        source
        if isinstance(source, Drawing)
        else build_drawing(source, strict=strict, curve_segments=curve_segments)
    )
    if ax is None:
        _, ax = plt.subplots()

    background_rgb = background or drawing.background_color
    background_color = _mpl_color(background_rgb)
    foreground = (
        (1.0, 1.0, 1.0) if _luminance(background_rgb) < 128.0 else (0.0, 0.0, 0.0)
    )
    ax.set_facecolor(background_color)
    ax.figure.set_facecolor(background_color)

    for fill in drawing.fills:
        if not fill.style.visible or len(fill.outer) < 3:
            continue
        vertices: List[Tuple[float, float]] = []
        codes: List[int] = []
        outer = _oriented_ring(list(fill.outer), counter_clockwise=True)
        holes = [
            _oriented_ring(list(hole), counter_clockwise=False)
            for hole in fill.holes
            if len(hole) >= 3
        ]
        for ring in [outer] + holes:
            vertices.extend(ring)
            vertices.append(ring[0])
            codes.extend(
                [MplPath.MOVETO]
                + [MplPath.LINETO] * (len(ring) - 1)
                + [MplPath.CLOSEPOLY]
            )
        color = foreground if monochrome else _mpl_color(fill.style.color)
        ax.add_patch(
            PathPatch(
                MplPath(vertices, codes),
                facecolor=color,
                edgecolor="none",
                linewidth=0.0,
            )
        )

    grouped_paths: DefaultDict[
        Tuple[Tuple[float, float, float], float, MatplotlibLineStyle],
        List[List[Tuple[float, float]]],
    ] = defaultdict(list)
    for path in drawing.paths:
        if not path.style.visible or len(path.points) < 2:
            continue
        points = list(path.points)
        if path.closed:
            points.append(points[0])
        color = foreground if monochrome else _mpl_color(path.style.color)
        linewidth = max(0.1, path.style.line_width_mm * 72.0 / 25.4 * linewidth_scale)
        grouped_paths[(color, linewidth, _matplotlib_line_style(path.style))].append(
            points
        )
    for (color, linewidth, line_style), paths in grouped_paths.items():
        collection = LineCollection(
            paths,
            colors=[color],
            linewidths=[linewidth],
            linestyles=[line_style],
        )
        ax.add_collection(collection)

    grouped_markers: DefaultDict[
        Tuple[Tuple[float, float, float], str, float],
        List[Tuple[float, float]],
    ] = defaultdict(list)
    for marker in drawing.markers:
        if not marker.style.visible:
            continue
        color = foreground if monochrome else _mpl_color(marker.style.color)
        symbol = _marker_symbol(marker.marker_code)
        size = max(4.0, abs(marker.scale) * 8.0) ** 2
        grouped_markers[(color, symbol, size)].append(marker.position)
    for (color, symbol, size), positions in grouped_markers.items():
        ax.scatter(
            [position[0] for position in positions],
            [position[1] for position in positions],
            c=[color],
            marker=symbol,
            s=size,
            linewidths=0.5,
        )

    font = FontProperties(family=font_family) if font_family else FontProperties()
    for text in drawing.texts:
        if not text.style.visible or not text.text or text.height <= 0.0:
            continue
        text_path = TextPath((0.0, 0.0), text.text, size=1.0, prop=font)
        bounds = text_path.get_extents()
        if bounds.width <= 0.0 or bounds.height <= 0.0:
            continue
        horizontal = max(0, min(2, (text.base_point - 1) % 3))
        vertical = max(0, min(2, (text.base_point - 1) // 3))
        anchors_x = (bounds.x0, (bounds.x0 + bounds.x1) * 0.5, bounds.x1)
        anchors_y = (bounds.y0, (bounds.y0 + bounds.y1) * 0.5, bounds.y1)
        width = (
            text.width
            if text.width > 0.0
            else bounds.width * text.height / bounds.height
        )
        transform = (
            Affine2D()
            .translate(-anchors_x[horizontal], -anchors_y[vertical])
            .scale(width / bounds.width, text.height / bounds.height)
            .rotate_deg(text.angle_deg)
            .translate(text.anchor[0], text.anchor[1])
        )
        color = foreground if monochrome else _mpl_color(text.style.color)
        ax.add_patch(
            PathPatch(
                text_path,
                transform=transform + ax.transData,
                facecolor=color,
                edgecolor="none",
                linewidth=0.0,
            )
        )

    bounds = drawing.bounds()
    if bounds is not None:
        minimum_x, minimum_y, maximum_x, maximum_y = bounds
        span = max(maximum_x - minimum_x, maximum_y - minimum_y, 1.0)
        margin = span * 0.02
        ax.set_xlim(minimum_x - margin, maximum_x + margin)
        ax.set_ylim(minimum_y - margin, maximum_y + margin)
    ax.set_aspect("equal", adjustable="box")
    if not show_axes:
        ax.set_axis_off()
    if show:
        plt.show()
    return ax


def _mpl_color(color: Tuple[int, int, int]) -> Tuple[float, float, float]:
    return tuple(channel / 255.0 for channel in color)  # type: ignore[return-value]


def _luminance(color: Tuple[int, int, int]) -> float:
    return 0.2126 * color[0] + 0.7152 * color[1] + 0.0722 * color[2]


def _matplotlib_line_style(style: RenderStyle) -> MatplotlibLineStyle:
    normalized = " ".join(style.line_type.lower().split())
    pattern = LINE_TYPE_PATTERNS.get(normalized)
    if pattern is not None:
        if not pattern:
            return "solid"
        return 0.0, tuple(abs(value) for value in pattern)
    if (
        "dotted" in normalized
        and "dash" not in normalized
        and "chain" not in normalized
    ):
        return "dotted"
    if "chain" in normalized or ("dash" in normalized and "dotted" in normalized):
        return "dashdot"
    if "dash" in normalized:
        return "dashed"
    return "solid"


def _marker_symbol(code: int) -> str:
    return {
        1: ".",
        2: "+",
        3: "x",
        4: "o",
        5: "s",
        6: "^",
    }.get(code, "+")


def _oriented_ring(
    ring: List[Tuple[float, float]], *, counter_clockwise: bool
) -> List[Tuple[float, float]]:
    area = sum(
        point1[0] * point2[1] - point2[0] * point1[1]
        for point1, point2 in zip(ring, ring[1:] + ring[:1])
    )
    if (area > 0.0) != counter_clockwise:
        ring.reverse()
    return ring


__all__ = ["plot"]
