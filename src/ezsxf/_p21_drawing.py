"""Resolve AP202-mode P21 presentation entities into drawing primitives."""

from __future__ import annotations

import math
import re
from collections.abc import Iterable, Mapping, Sequence
from dataclasses import dataclass
from typing import Any, Dict, List, Optional, Set, Tuple

from ezsxf._drawing import (
    Affine,
    Color,
    Drawing,
    FillPrimitive,
    MarkerPrimitive,
    PathPrimitive,
    Point,
    RenderStyle,
    TextPrimitive,
)
from ezsxf._drawing_geometry import (
    IDENTITY,
    Geometry,
    apply,
    apply_vector,
    average_scale,
    clip_hatch_lines,
    compose,
    near,
    sample_arc,
    sample_ellipse,
    without_duplicate_end,
)
from ezsxf._drawing_style import _COLOR_NAMES

_STEP_ESCAPE_RE = re.compile(r"\\X([24])\\([0-9A-Fa-f]+)\\X0\\", re.IGNORECASE)
_BACKGROUND_RGB_RE = re.compile(r"(?<!\d)(\d{1,3})_(\d{1,3})_(\d{1,3})(?!\d)")


@dataclass
class _StyleValues:
    color: Optional[Color] = None
    line_type: Optional[str] = None
    line_width_mm: Optional[float] = None
    font_name: Optional[str] = None


class P21DrawingBuilder:
    """Flatten the AP202 presentation graph emitted by SXF P21 files."""

    def __init__(self, parsed: Mapping[str, Any], *, curve_segments: int) -> None:
        self.parsed = parsed
        self.curve_segments = curve_segments
        self.entities: Dict[int, Mapping[str, Any]] = {
            int(entity["id"]): entity for entity in parsed.get("entities", [])
        }
        self.records: Dict[int, Dict[str, Mapping[str, Any]]] = {
            entity_id: {
                str(record["keyword"]).upper(): record
                for record in _entity_records(entity)
            }
            for entity_id, entity in self.entities.items()
        }
        self.layers: Dict[int, str] = {}
        self.hidden: Set[int] = set()
        self.sheet_items: List[int] = []
        self._geometry_cache: Dict[int, List[Geometry]] = {}
        self._style_cache: Dict[Tuple[int, ...], _StyleValues] = {}
        self._text_keys: Set[Tuple[Any, ...]] = set()
        self._warning_keys: Set[str] = set()
        self._scan_presentation_structure()
        self.drawing = Drawing(background_color=self._find_background_color())

    def build(self) -> Drawing:
        for warning in self.parsed.get("warnings", []):
            if isinstance(warning, Mapping):
                message = str(warning.get("message") or warning)
            else:
                message = str(warning)
            self.drawing.warnings.append(message)

        if not self.sheet_items:
            raise ValueError("P21 drawing has no DRAWING_SHEET_REVISION items")
        self._render_items(
            self.sheet_items,
            IDENTITY,
            layer_override=None,
            parent_visible=True,
            active=set(),
        )
        return self.drawing

    def warn_once(self, key: str, message: str) -> None:
        if key not in self._warning_keys:
            self._warning_keys.add(key)
            self.drawing.warnings.append(message)

    def _scan_presentation_structure(self) -> None:
        for entity_id, records in self.records.items():
            layer = records.get("PRESENTATION_LAYER_ASSIGNMENT")
            if layer is not None:
                params = layer.get("parameters", [])
                if len(params) >= 3:
                    name = decode_step_string(str(params[0]))
                    for item_id in _references(params[2]):
                        self.layers[item_id] = name

            invisibility = records.get("INVISIBILITY")
            if invisibility is not None:
                params = invisibility.get("parameters", [])
                if params:
                    self.hidden.update(_references(params[0]))

            sheet = records.get("DRAWING_SHEET_REVISION")
            if sheet is not None:
                params = sheet.get("parameters", [])
                if len(params) >= 2:
                    self.sheet_items = list(_references(params[1]))

    def _find_background_color(self) -> Color:
        for records in self.records.values():
            relevant = (
                records.get("DRAUGHTING_SUBFIGURE_REPRESENTATION"),
                records.get("REPRESENTATION_ITEM"),
            )
            for record in relevant:
                if record is None:
                    continue
                for value in _strings(record.get("parameters", [])):
                    decoded = decode_step_string(value)
                    if "背景色" not in decoded and "background" not in decoded.lower():
                        continue
                    matches = list(_BACKGROUND_RGB_RE.finditer(decoded))
                    if not matches:
                        continue
                    red, green, blue = (
                        _clamp_color(int(channel)) for channel in matches[-1].groups()
                    )
                    return red, green, blue
        return (255, 255, 255)

    def _render_item(
        self,
        entity_id: int,
        transform: Affine,
        *,
        layer_override: Optional[str],
        parent_visible: bool,
        active: Set[int],
    ) -> None:
        if entity_id in active:
            self.warn_once(
                "p21-cycle-{0}".format(entity_id),
                "Cyclic P21 presentation reference at #{0} was skipped".format(
                    entity_id
                ),
            )
            return
        records = self.records.get(entity_id)
        if records is None:
            self.warn_once(
                "p21-missing-{0}".format(entity_id),
                "P21 presentation item #{0} is missing".format(entity_id),
            )
            return

        active = set(active)
        active.add(entity_id)
        visible = parent_visible and entity_id not in self.hidden
        own_layer = self.layers.get(entity_id)
        next_layer = layer_override
        if next_layer is None and own_layer and not _is_dummy_layer(own_layer):
            next_layer = own_layer

        callout = records.get("DRAUGHTING_CALLOUT")
        if callout is not None:
            params = callout.get("parameters", [])
            if params:
                child_ids = list(_references(params[0]))
                if next_layer is None:
                    next_layer = self._find_nested_layer(child_ids, set())
                for child_id in child_ids:
                    self._render_item(
                        child_id,
                        transform,
                        layer_override=next_layer,
                        parent_visible=visible,
                        active=active,
                    )
            return

        styled = records.get("STYLED_ITEM")
        if styled is not None:
            assignment_ids, target_id = _styled_item_parts(styled)
            if target_id is not None:
                style = self._render_style(
                    assignment_ids,
                    layer=next_layer or "0",
                    visible=visible,
                )
                self._render_target(
                    target_id,
                    style,
                    transform,
                    layer_override=next_layer,
                    source_id=entity_id,
                    active=active,
                )
                return

        if "MAPPED_ITEM" in records:
            self._render_mapped_item(
                entity_id,
                transform,
                layer_override=next_layer,
                parent_visible=visible,
                active=active,
            )
            return

        representation = records.get("DRAUGHTING_SUBFIGURE_REPRESENTATION")
        if representation is not None:
            self._render_representation(
                representation,
                transform,
                layer_override=next_layer,
                parent_visible=visible,
                active=active,
            )
            return

        if self._geometry(entity_id):
            style = self._render_style((), layer=next_layer or "0", visible=visible)
            self._append_geometry(entity_id, style, transform, entity_id)
            return

        if not set(records).issubset(_NON_RENDERED_P21_RECORDS):
            names = ", ".join(sorted(records))
            self.warn_once(
                "p21-unsupported-{0}".format(names),
                "P21 drawing conversion does not render record set {0}".format(names),
            )

    def _render_items(
        self,
        item_ids: Sequence[int],
        transform: Affine,
        *,
        layer_override: Optional[str],
        parent_visible: bool,
        active: Set[int],
    ) -> None:
        item_set = set(item_ids)
        nested_callout_items: Set[int] = set()
        for item_id in item_ids:
            callout = self.records.get(item_id, {}).get("DRAUGHTING_CALLOUT")
            if callout is None:
                continue
            params = callout.get("parameters", [])
            if params:
                nested_callout_items.update(
                    child_id
                    for child_id in _references(params[0])
                    if child_id in item_set
                )
        for item_id in item_ids:
            if item_id in nested_callout_items:
                continue
            self._render_item(
                item_id,
                transform,
                layer_override=layer_override,
                parent_visible=parent_visible,
                active=active,
            )

    def _render_target(
        self,
        target_id: int,
        style: RenderStyle,
        transform: Affine,
        *,
        layer_override: Optional[str],
        source_id: int,
        active: Set[int],
    ) -> None:
        records = self.records.get(target_id, {})
        if "TEXT_LITERAL_WITH_EXTENT" in records:
            self._append_text(target_id, style, transform, source_id)
        elif "ANNOTATION_FILL_AREA" in records:
            self._append_fill_area(target_id, style, transform, source_id)
        elif "DEFINED_SYMBOL" in records:
            self._append_defined_symbol(target_id, style, transform, source_id)
        elif "MAPPED_ITEM" in records:
            self._render_mapped_item(
                target_id,
                transform,
                layer_override=layer_override,
                parent_visible=style.visible,
                active=active,
            )
        elif "DRAUGHTING_SUBFIGURE_REPRESENTATION" in records:
            self._render_representation(
                records["DRAUGHTING_SUBFIGURE_REPRESENTATION"],
                transform,
                layer_override=layer_override,
                parent_visible=style.visible,
                active=active,
            )
        elif self._geometry(target_id):
            self._append_geometry(target_id, style, transform, source_id)
        else:
            self._render_item(
                target_id,
                transform,
                layer_override=layer_override,
                parent_visible=style.visible,
                active=active,
            )

    def _render_representation(
        self,
        record: Mapping[str, Any],
        transform: Affine,
        *,
        layer_override: Optional[str],
        parent_visible: bool,
        active: Set[int],
    ) -> None:
        params = record.get("parameters", [])
        if len(params) < 2:
            return
        self._render_items(
            list(_references(params[1])),
            transform,
            layer_override=layer_override,
            parent_visible=parent_visible,
            active=active,
        )

    def _render_mapped_item(
        self,
        entity_id: int,
        transform: Affine,
        *,
        layer_override: Optional[str],
        parent_visible: bool,
        active: Set[int],
    ) -> None:
        mapped = self.records.get(entity_id, {}).get("MAPPED_ITEM")
        if mapped is None:
            return
        params = mapped.get("parameters", [])
        if len(params) < 2:
            return
        map_id = _reference(params[0])
        target_id = _reference(params[1])
        if map_id is None or target_id is None:
            return
        map_record = self.records.get(map_id, {}).get("SYMBOL_REPRESENTATION_MAP")
        if map_record is None:
            return
        map_params = map_record.get("parameters", [])
        if len(map_params) < 2:
            return
        source_axis_id = _reference(map_params[0])
        representation_id = _reference(map_params[1])
        if source_axis_id is None or representation_id is None:
            return
        source = self._axis_transform(source_axis_id)
        target = self._symbol_target_transform(target_id)
        mapping = compose(target, _inverse_affine(source))
        mapped_transform = compose(transform, mapping)
        representation = self.records.get(representation_id, {}).get(
            "DRAUGHTING_SUBFIGURE_REPRESENTATION"
        )
        if representation is not None:
            self._render_representation(
                representation,
                mapped_transform,
                layer_override=layer_override,
                parent_visible=parent_visible,
                active=active,
            )

    def _append_geometry(
        self,
        geometry_id: int,
        style: RenderStyle,
        transform: Affine,
        source_id: int,
    ) -> None:
        for points, closed in self._geometry(geometry_id):
            transformed = tuple(apply(transform, point) for point in points)
            if len(transformed) < 2:
                continue
            self.drawing.paths.append(
                PathPrimitive(
                    points=transformed,
                    closed=closed,
                    style=style,
                    source_id=source_id,
                )
            )

    def _append_text(
        self,
        text_id: int,
        style: RenderStyle,
        transform: Affine,
        source_id: int,
    ) -> None:
        record = self.records[text_id]["TEXT_LITERAL_WITH_EXTENT"]
        params = record.get("parameters", [])
        if len(params) < 7:
            return
        placement_id = _reference(params[2])
        extent_id = _reference(params[6])
        if placement_id is None or extent_id is None:
            return
        placement = compose(transform, self._axis_transform(placement_id))
        extent = self.records.get(extent_id, {}).get("PLANAR_EXTENT")
        if extent is None:
            return
        extent_params = extent.get("parameters", [])
        if len(extent_params) < 3:
            return
        width = abs(float(extent_params[1]))
        height = abs(float(extent_params[2]))
        text_value = decode_step_string(str(params[1]))
        if not text_value:
            return
        x_vector = apply_vector(placement, (1.0, 0.0))
        y_vector = apply_vector(placement, (0.0, 1.0))
        font_name = self._font_name(_reference(params[5]))
        text_style = RenderStyle(
            layer=style.layer,
            color=style.color,
            line_type=style.line_type,
            line_width_mm=style.line_width_mm,
            font_name=font_name or style.font_name,
            visible=style.visible,
        )
        primitive = TextPrimitive(
            text=text_value,
            anchor=apply(
                placement,
                (0.0, _text_vertical_offset(_text_base_point(str(params[0])), height)),
            ),
            height=height * math.hypot(*y_vector),
            width=width * math.hypot(*x_vector),
            angle_deg=math.degrees(math.atan2(x_vector[1], x_vector[0])),
            base_point=_text_base_point(str(params[0])),
            direction=1,
            style=text_style,
            source_id=source_id,
        )
        key = (
            primitive.source_id,
            primitive.text,
            primitive.anchor,
            primitive.height,
            primitive.width,
            primitive.angle_deg,
            primitive.base_point,
            primitive.style,
        )
        if key not in self._text_keys:
            self._text_keys.add(key)
            self.drawing.texts.append(primitive)

    def _find_nested_layer(
        self, item_ids: Sequence[int], visited: Set[int]
    ) -> Optional[str]:
        for item_id in item_ids:
            if item_id in visited:
                continue
            visited.add(item_id)
            layer = self.layers.get(item_id)
            if layer and not _is_dummy_layer(layer):
                return layer
        for item_id in item_ids:
            callout = self.records.get(item_id, {}).get("DRAUGHTING_CALLOUT")
            if callout is None:
                continue
            params = callout.get("parameters", [])
            if not params:
                continue
            layer = self._find_nested_layer(list(_references(params[0])), visited)
            if layer is not None:
                return layer
        return None

    def _append_fill_area(
        self,
        fill_id: int,
        fallback_style: RenderStyle,
        transform: Affine,
        source_id: int,
    ) -> None:
        record = self.records[fill_id]["ANNOTATION_FILL_AREA"]
        params = record.get("parameters", [])
        if len(params) < 2:
            return
        rings: List[List[Point]] = []
        for boundary_id in _references(params[1]):
            pieces = self._geometry(boundary_id)
            merged = _merge_geometry_points(pieces)
            if len(merged) >= 3:
                rings.append(
                    without_duplicate_end([apply(transform, point) for point in merged])
                )
        if not rings:
            return
        outer, holes = rings[0], rings[1:]

        style_ids = self._fill_style_entities_from_occurrence(source_id)
        rendered = False
        for style_id in style_ids:
            records = self.records.get(style_id, {})
            colour = records.get("FILL_AREA_STYLE_COLOUR")
            if colour is not None:
                colour_params = colour.get("parameters", [])
                colour_id = (
                    _reference(colour_params[1]) if len(colour_params) > 1 else None
                )
                values = self._style_values((colour_id,) if colour_id else ())
                style = self._style_from_values(
                    values,
                    layer=fallback_style.layer,
                    visible=fallback_style.visible,
                )
                self.drawing.fills.append(
                    FillPrimitive(
                        outer=tuple(outer),
                        holes=tuple(tuple(hole) for hole in holes),
                        style=style,
                        source_id=source_id,
                    )
                )
                rendered = True
                continue

            hatching = records.get("FILL_AREA_STYLE_HATCHING")
            if hatching is not None:
                hatch_params = hatching.get("parameters", [])
                if len(hatch_params) < 6:
                    continue
                curve_style_id = _reference(hatch_params[1])
                repeat_id = _reference(hatch_params[2])
                start_id = _reference(hatch_params[3])
                if curve_style_id is None or repeat_id is None or start_id is None:
                    continue
                repeat = self._repeat_spacing(repeat_id) * average_scale(transform)
                start = apply(transform, self._point(start_id))
                local_angle = float(hatch_params[5])
                direction = apply_vector(
                    transform,
                    (math.cos(local_angle), math.sin(local_angle)),
                )
                angle_deg = math.degrees(math.atan2(direction[1], direction[0]))
                hatch_style = self._render_style(
                    (curve_style_id,),
                    layer=fallback_style.layer,
                    visible=fallback_style.visible,
                )
                for begin, end in clip_hatch_lines(
                    outer, holes, start, repeat, angle_deg
                ):
                    self.drawing.paths.append(
                        PathPrimitive(
                            points=(begin, end),
                            closed=False,
                            style=hatch_style,
                            source_id=source_id,
                        )
                    )
                rendered = True
                continue

            if "EXTERNALLY_DEFINED_HATCH_STYLE" in records:
                self.warn_once(
                    "p21-external-hatch",
                    "Externally defined P21 hatch patterns have no embedded geometry",
                )
                rendered = True

        if not rendered:
            for ring in rings:
                self.drawing.paths.append(
                    PathPrimitive(
                        points=tuple(ring),
                        closed=True,
                        style=fallback_style,
                        source_id=source_id,
                    )
                )

    def _append_defined_symbol(
        self,
        symbol_id: int,
        style: RenderStyle,
        transform: Affine,
        source_id: int,
    ) -> None:
        record = self.records[symbol_id]["DEFINED_SYMBOL"]
        params = record.get("parameters", [])
        if len(params) < 3:
            return
        definition_id = _reference(params[1])
        target_id = _reference(params[2])
        if definition_id is None or target_id is None:
            return
        symbol_transform = compose(transform, self._symbol_target_transform(target_id))
        definition = self.records.get(definition_id, {})

        terminator = definition.get("PRE_DEFINED_TERMINATOR_SYMBOL")
        if terminator is not None:
            curve_style = self._terminator_curve_style(source_id, style)
            if curve_style is not None:
                style = RenderStyle(
                    layer=style.layer,
                    color=style.color,
                    line_type=curve_style.line_type,
                    line_width_mm=curve_style.line_width_mm,
                    font_name=style.font_name,
                    visible=style.visible,
                )
            symbol_params = terminator.get("parameters", [])
            name = str(symbol_params[0]).strip().lower() if symbol_params else ""
            if name == "filled dot":
                points = sample_ellipse(
                    (0.0, 0.0), 5.0, 5.0, 0.0, 0.0, 360.0, 0, 16, closed=True
                )
                self.drawing.fills.append(
                    FillPrimitive(
                        outer=tuple(apply(symbol_transform, point) for point in points),
                        holes=(),
                        style=style,
                        source_id=source_id,
                    )
                )
            elif name == "filled arrow":
                points = [(-10.0, 3.5), (0.0, 0.0), (-10.0, -3.5)]
                self.drawing.fills.append(
                    FillPrimitive(
                        outer=tuple(apply(symbol_transform, point) for point in points),
                        holes=(),
                        style=style,
                        source_id=source_id,
                    )
                )
            else:
                points = [(-10.0, 3.5), (0.0, 0.0), (-10.0, -3.5)]
                self.drawing.paths.append(
                    PathPrimitive(
                        points=tuple(
                            apply(symbol_transform, point) for point in points
                        ),
                        closed=False,
                        style=style,
                        source_id=source_id,
                    )
                )
            return

        point_marker = definition.get("PRE_DEFINED_POINT_MARKER_SYMBOL")
        if point_marker is not None:
            marker_params = point_marker.get("parameters", [])
            name = str(marker_params[0]) if marker_params else ""
            self.drawing.markers.append(
                MarkerPrimitive(
                    position=apply(symbol_transform, (0.0, 0.0)),
                    marker_code=1,
                    scale=average_scale(symbol_transform),
                    style=style,
                    source_id=source_id,
                    name=name,
                )
            )
            return

        self.warn_once(
            "p21-symbol-{0}".format(definition_id),
            "Unsupported P21 defined symbol #{0}".format(definition_id),
        )

    def _terminator_curve_style(
        self, occurrence_id: int, fallback: RenderStyle
    ) -> Optional[RenderStyle]:
        terminator = self.records.get(occurrence_id, {}).get("TERMINATOR_SYMBOL")
        if terminator is None:
            return None
        params = terminator.get("parameters", [])
        curve_id = _reference(params[0]) if params else None
        if curve_id is None:
            return None
        styled = self.records.get(curve_id, {}).get("STYLED_ITEM")
        if styled is None:
            return None
        assignment_ids, _ = _styled_item_parts(styled)
        if not assignment_ids:
            return None
        return self._render_style(
            assignment_ids,
            layer=fallback.layer,
            visible=fallback.visible,
        )

    def _fill_style_entities_from_occurrence(self, occurrence_id: int) -> List[int]:
        styled = self.records.get(occurrence_id, {}).get("STYLED_ITEM")
        if styled is None:
            return []
        assignment_ids, _ = _styled_item_parts(styled)
        if not assignment_ids:
            return []
        output: List[int] = []
        for assignment_id in assignment_ids:
            assignment = self.records.get(assignment_id, {}).get(
                "PRESENTATION_STYLE_ASSIGNMENT"
            )
            if assignment is None:
                continue
            assignment_params = assignment.get("parameters", [])
            if not assignment_params:
                continue
            for fill_style_id in _references(assignment_params[0]):
                fill_style = self.records.get(fill_style_id, {}).get("FILL_AREA_STYLE")
                if fill_style is None:
                    continue
                fill_params = fill_style.get("parameters", [])
                if len(fill_params) >= 2:
                    output.extend(_references(fill_params[1]))
        return output

    def _render_style(
        self,
        style_ids: Sequence[int],
        *,
        layer: str,
        visible: bool,
    ) -> RenderStyle:
        return self._style_from_values(
            self._style_values(tuple(style_ids)), layer=layer, visible=visible
        )

    def _style_from_values(
        self,
        values: _StyleValues,
        *,
        layer: str,
        visible: bool,
    ) -> RenderStyle:
        return RenderStyle(
            layer=layer,
            color=values.color or (0, 0, 0),
            line_type=values.line_type or "continuous",
            line_width_mm=values.line_width_mm or 0.25,
            font_name=values.font_name,
            visible=visible,
        )

    def _style_values(self, style_ids: Tuple[int, ...]) -> _StyleValues:
        cached = self._style_cache.get(style_ids)
        if cached is not None:
            return cached
        values = _StyleValues()
        visited: Set[int] = set()
        for style_id in style_ids:
            self._visit_style(style_id, values, visited)
        self._style_cache[style_ids] = values
        return values

    def _visit_style(
        self,
        entity_id: int,
        values: _StyleValues,
        visited: Set[int],
    ) -> None:
        if entity_id in visited:
            return
        visited.add(entity_id)
        records = self.records.get(entity_id, {})

        colour = records.get("DRAUGHTING_PRE_DEFINED_COLOUR")
        if colour is not None:
            params = colour.get("parameters", [])
            if params:
                normalized = " ".join(str(params[0]).lower().split())
                values.color = _COLOR_NAMES.get(normalized, (0, 0, 0))

        rgb = records.get("COLOUR_RGB")
        if rgb is not None:
            params = rgb.get("parameters", [])
            if len(params) >= 4:
                values.color = tuple(
                    _clamp_color(int(round(float(channel) * 255.0)))
                    for channel in params[1:4]
                )  # type: ignore[assignment]

        line_type = records.get("DRAUGHTING_PRE_DEFINED_CURVE_FONT")
        if line_type is not None:
            params = line_type.get("parameters", [])
            if params:
                values.line_type = " ".join(str(params[0]).lower().split())

        measure = records.get("LENGTH_MEASURE_WITH_UNIT")
        if measure is not None:
            params = measure.get("parameters", [])
            if params:
                number = _first_number(params[0])
                if number is not None and number > 0.0:
                    values.line_width_mm = number

        font = records.get("EXTERNALLY_DEFINED_TEXT_FONT")
        if font is not None:
            params = font.get("parameters", [])
            if params:
                name = _first_string(params[0])
                if name:
                    values.font_name = decode_step_string(name)

        traversed_records = set(records) & _STYLE_CONTAINER_RECORDS
        if traversed_records:
            for name in traversed_records:
                record = records[name]
                for reference_id in _references(record.get("parameters", [])):
                    self._visit_style(reference_id, values, visited)

    def _font_name(self, entity_id: Optional[int]) -> Optional[str]:
        if entity_id is None:
            return None
        values = _StyleValues()
        self._visit_style(entity_id, values, set())
        return values.font_name

    def _geometry(self, entity_id: int) -> List[Geometry]:
        cached = self._geometry_cache.get(entity_id)
        if cached is not None:
            return cached
        self._geometry_cache[entity_id] = []
        records = self.records.get(entity_id, {})
        geometry: List[Geometry] = []

        polyline = records.get("POLYLINE")
        if polyline is not None:
            params = polyline.get("parameters", [])
            if len(params) >= 2:
                points = [self._point(point_id) for point_id in _references(params[1])]
                closed = len(points) > 2 and near(points[0], points[-1])
                geometry = [(without_duplicate_end(points), closed)]

        circle = records.get("CIRCLE")
        if circle is not None:
            params = circle.get("parameters", [])
            if len(params) >= 3:
                axis_id = _reference(params[1])
                if axis_id is not None:
                    axis = self._axis_transform(axis_id)
                    center = apply(axis, (0.0, 0.0))
                    rotation = math.degrees(math.atan2(axis[1], axis[0]))
                    points = sample_ellipse(
                        center,
                        float(params[2]),
                        float(params[2]),
                        rotation,
                        0.0,
                        360.0,
                        0,
                        self.curve_segments,
                        closed=True,
                    )
                    geometry = [(points, True)]

        ellipse = records.get("ELLIPSE")
        if ellipse is not None:
            params = ellipse.get("parameters", [])
            if len(params) >= 4:
                axis_id = _reference(params[1])
                if axis_id is not None:
                    axis = self._axis_transform(axis_id)
                    center = apply(axis, (0.0, 0.0))
                    rotation = math.degrees(math.atan2(axis[1], axis[0]))
                    points = sample_ellipse(
                        center,
                        float(params[2]),
                        float(params[3]),
                        rotation,
                        0.0,
                        360.0,
                        0,
                        self.curve_segments,
                        closed=True,
                    )
                    geometry = [(points, True)]

        trimmed = records.get("TRIMMED_CURVE")
        if trimmed is not None:
            geometry = self._trimmed_curve(trimmed)

        composite = records.get("COMPOSITE_CURVE")
        if composite is not None:
            params = composite.get("parameters", [])
            pieces: List[Geometry] = []
            if len(params) >= 2:
                for segment_id in _references(params[1]):
                    segment = self.records.get(segment_id, {}).get(
                        "COMPOSITE_CURVE_SEGMENT"
                    )
                    if segment is None:
                        continue
                    segment_params = segment.get("parameters", [])
                    if len(segment_params) < 3:
                        continue
                    curve_id = _reference(segment_params[2])
                    if curve_id is None:
                        continue
                    segment_geometry = self._geometry(curve_id)
                    if _enum_value(segment_params[1]) == "F":
                        segment_geometry = [
                            (list(reversed(points)), closed)
                            for points, closed in reversed(segment_geometry)
                        ]
                    pieces.extend(segment_geometry)
            merged = _merge_geometry_points(pieces)
            if merged:
                closed = len(merged) > 2 and near(merged[0], merged[-1])
                geometry = [(without_duplicate_end(merged), closed)]

        self._geometry_cache[entity_id] = geometry
        return geometry

    def _trimmed_curve(self, record: Mapping[str, Any]) -> List[Geometry]:
        params = record.get("parameters", [])
        if len(params) < 6:
            return []
        basis_id = _reference(params[1])
        if basis_id is None:
            return []
        basis = self.records.get(basis_id, {})
        agrees = _enum_value(params[4]) != "F"

        if "LINE" in basis:
            start_id = next(iter(_references(params[2])), None)
            end_id = next(iter(_references(params[3])), None)
            if start_id is None or end_id is None:
                return []
            points = [self._point(start_id), self._point(end_id)]
            if not agrees:
                points.reverse()
            return [(points, False)]

        circle = basis.get("CIRCLE")
        if circle is not None:
            circle_params = circle.get("parameters", [])
            if len(circle_params) < 3:
                return []
            axis_id = _reference(circle_params[1])
            if axis_id is None:
                return []
            start = _first_number(params[2])
            end = _first_number(params[3])
            if start is None or end is None:
                return []
            axis = self._axis_transform(axis_id)
            center = apply(axis, (0.0, 0.0))
            rotation = math.atan2(axis[1], axis[0])
            start_angle = round(math.degrees(start + rotation), 10)
            end_angle = round(math.degrees(end + rotation), 10)
            counter_clockwise_sweep = (end_angle - start_angle) % 360.0
            direction_flag = 0 if counter_clockwise_sweep <= 180.0 else 1
            points = sample_arc(
                center,
                float(circle_params[2]),
                start_angle,
                end_angle,
                direction_flag,
                self.curve_segments,
            )
            return [(points, False)]
        return []

    def _point(self, entity_id: int) -> Point:
        point = self.records.get(entity_id, {}).get("CARTESIAN_POINT")
        if point is None:
            return (0.0, 0.0)
        params = point.get("parameters", [])
        coordinates = params[1] if len(params) >= 2 else []
        if not isinstance(coordinates, Sequence) or len(coordinates) < 2:
            return (0.0, 0.0)
        return float(coordinates[0]), float(coordinates[1])

    def _direction(self, entity_id: Optional[int]) -> Point:
        if entity_id is None:
            return (1.0, 0.0)
        direction = self.records.get(entity_id, {}).get("DIRECTION")
        if direction is None:
            return (1.0, 0.0)
        params = direction.get("parameters", [])
        values = params[1] if len(params) >= 2 else []
        if not isinstance(values, Sequence) or len(values) < 2:
            return (1.0, 0.0)
        x, y = float(values[0]), float(values[1])
        length = math.hypot(x, y)
        return (1.0, 0.0) if length <= 1.0e-15 else (x / length, y / length)

    def _axis_transform(self, entity_id: int) -> Affine:
        axis = self.records.get(entity_id, {}).get("AXIS2_PLACEMENT_2D")
        if axis is None:
            return IDENTITY
        params = axis.get("parameters", [])
        origin_id = _reference(params[1]) if len(params) >= 2 else None
        direction_id = _reference(params[2]) if len(params) >= 3 else None
        origin = self._point(origin_id) if origin_id is not None else (0.0, 0.0)
        direction = self._direction(direction_id)
        return (
            direction[0],
            direction[1],
            -direction[1],
            direction[0],
            origin[0],
            origin[1],
        )

    def _symbol_target_transform(self, entity_id: int) -> Affine:
        target = self.records.get(entity_id, {}).get("SYMBOL_TARGET")
        if target is None:
            return IDENTITY
        params = target.get("parameters", [])
        axis_id = _reference(params[1]) if len(params) >= 2 else None
        axis = self._axis_transform(axis_id) if axis_id is not None else IDENTITY
        ratio_x = float(params[2]) if len(params) >= 3 else 1.0
        ratio_y = float(params[3]) if len(params) >= 4 else ratio_x
        return compose(axis, (ratio_x, 0.0, 0.0, ratio_y, 0.0, 0.0))

    def _repeat_spacing(self, entity_id: int) -> float:
        repeat = self.records.get(entity_id, {}).get("ONE_DIRECTION_REPEAT_FACTOR")
        if repeat is None:
            return 0.0
        params = repeat.get("parameters", [])
        vector_id = _reference(params[1]) if len(params) >= 2 else None
        if vector_id is None:
            return 0.0
        vector = self.records.get(vector_id, {}).get("VECTOR")
        if vector is None:
            return 0.0
        vector_params = vector.get("parameters", [])
        return abs(float(vector_params[2])) if len(vector_params) >= 3 else 0.0


def decode_step_string(value: str) -> str:
    """Decode Part 21 ``X2``/``X4`` Unicode escape sequences."""

    def replace(match: re.Match[str]) -> str:
        width = match.group(1)
        payload = match.group(2)
        try:
            encoding = "utf-16-be" if width == "2" else "utf-32-be"
            return bytes.fromhex(payload).decode(encoding)
        except (UnicodeDecodeError, ValueError):
            return match.group(0)

    return _STEP_ESCAPE_RE.sub(replace, value)


def _entity_records(entity: Mapping[str, Any]) -> List[Mapping[str, Any]]:
    records = entity.get("records")
    if isinstance(records, list):
        return records
    record = entity.get("record")
    return [record] if isinstance(record, Mapping) else []


def _styled_item_parts(
    record: Mapping[str, Any],
) -> Tuple[Tuple[int, ...], Optional[int]]:
    params = record.get("parameters", [])
    offset = 1 if len(params) >= 3 and isinstance(params[0], str) else 0
    if len(params) < offset + 2:
        return (), None
    return tuple(_references(params[offset])), _reference(params[offset + 1])


def _reference(value: Any) -> Optional[int]:
    if isinstance(value, Mapping) and value.get("kind") == "reference":
        return int(value["value"])
    return None


def _references(value: Any) -> Iterable[int]:
    reference = _reference(value)
    if reference is not None:
        yield reference
    elif isinstance(value, Mapping):
        for child in value.values():
            yield from _references(child)
    elif isinstance(value, (list, tuple)):
        for child in value:
            yield from _references(child)


def _strings(value: Any) -> Iterable[str]:
    if isinstance(value, str):
        yield value
    elif isinstance(value, Mapping):
        for child in value.values():
            yield from _strings(child)
    elif isinstance(value, (list, tuple)):
        for child in value:
            yield from _strings(child)


def _first_string(value: Any) -> Optional[str]:
    return next(iter(_strings(value)), None)


def _first_number(value: Any) -> Optional[float]:
    if isinstance(value, bool):
        return None
    if isinstance(value, (int, float)):
        return float(value)
    if isinstance(value, Mapping):
        for child in value.values():
            number = _first_number(child)
            if number is not None:
                return number
    elif isinstance(value, (list, tuple)):
        for child in value:
            number = _first_number(child)
            if number is not None:
                return number
    return None


def _enum_value(value: Any) -> Optional[str]:
    if isinstance(value, Mapping) and value.get("kind") == "enum":
        return str(value.get("value", "")).upper()
    return None


def _merge_geometry_points(pieces: Sequence[Geometry]) -> List[Point]:
    output: List[Point] = []
    for points, _ in pieces:
        if not points:
            continue
        if not output:
            output.extend(points)
        elif near(output[-1], points[0]):
            output.extend(points[1:])
        elif near(output[-1], points[-1]):
            output.extend(reversed(points[:-1]))
        else:
            output.extend(points)
    return output


def _inverse_affine(transform: Affine) -> Affine:
    a, b, c, d, e, f = transform
    determinant = a * d - b * c
    if abs(determinant) <= 1.0e-15:
        return IDENTITY
    inverse_a = d / determinant
    inverse_b = -b / determinant
    inverse_c = -c / determinant
    inverse_d = a / determinant
    return (
        inverse_a,
        inverse_b,
        inverse_c,
        inverse_d,
        -(inverse_a * e + inverse_c * f),
        -(inverse_b * e + inverse_d * f),
    )


def _text_base_point(name: str) -> int:
    normalized = decode_step_string(name).lower()
    horizontal = 2
    if "left" in normalized:
        horizontal = 1
    elif "right" in normalized:
        horizontal = 3
    vertical = 1
    if "middleline" in normalized or "centreline" in normalized:
        vertical = 2
    elif "topline" in normalized or "capline" in normalized:
        vertical = 3
    return (vertical - 1) * 3 + horizontal


def _text_vertical_offset(base_point: int, height: float) -> float:
    vertical = (base_point - 1) // 3
    return (-0.5, 0.5, 1.0)[max(0, min(2, vertical))] * height


def _is_dummy_layer(name: str) -> bool:
    normalized = name.lower()
    return "dummy_layer" in normalized or normalized.startswith("$$sxf_dummy")


def _clamp_color(value: int) -> int:
    return max(0, min(255, value))


_STYLE_CONTAINER_RECORDS = {
    "CURVE_STYLE",
    "FILL_AREA_STYLE",
    "FILL_AREA_STYLE_COLOUR",
    "FILL_AREA_STYLE_HATCHING",
    "PRESENTATION_STYLE_ASSIGNMENT",
    "SYMBOL_COLOUR",
    "SYMBOL_STYLE",
    "TEXT_STYLE",
    "TEXT_STYLE_FOR_DEFINED_FONT",
    "TEXT_STYLE_WITH_BOX_CHARACTERISTICS",
    "TEXT_STYLE_WITH_SPACING",
}

_NON_RENDERED_P21_RECORDS = {
    "ANNOTATION_SYMBOL",
    "AXIS2_PLACEMENT_2D",
    "CARTESIAN_POINT",
    "DIRECTION",
    "DRAUGHTING_ELEMENTS",
    "GEOMETRIC_REPRESENTATION_ITEM",
    "PLANAR_BOX",
    "REPRESENTATION_ITEM",
    "SYMBOL_REPRESENTATION_MAP",
    "SYMBOL_TARGET",
    "VECTOR",
}


__all__ = ["P21DrawingBuilder", "decode_step_string"]
