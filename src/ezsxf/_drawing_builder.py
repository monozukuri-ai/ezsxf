"""Flatten a resolved SFC model into backend-neutral drawing primitives."""

from __future__ import annotations

import math
from collections.abc import Mapping
from typing import Any, Dict, List, Optional, Set

from ezsxf._drawing import (
    Affine,
    Color,
    Drawing,
    MarkerPrimitive,
    PathPrimitive,
    RenderStyle,
    TextPrimitive,
)
from ezsxf._drawing_annotations import render_annotation
from ezsxf._drawing_geometry import (
    IDENTITY,
    Geometry,
    apply,
    apply_vector,
    average_scale,
    compose,
    feature_geometry,
    point,
)
from ezsxf._drawing_hatches import render_hatch
from ezsxf._drawing_style import StyleResolver, optional_int


class DrawingBuilder:
    def __init__(self, parsed: Mapping[str, Any], *, curve_segments: int) -> None:
        self.parsed = parsed
        self.model: Mapping[str, Any] = parsed["model"]
        self.curve_segments = curve_segments
        self.features: Dict[int, Mapping[str, Any]] = {
            int(item["id"]): item for item in parsed.get("typed_features", [])
        }
        self.styles = StyleResolver(self.model, self.features)
        self.definitions = {
            int(item["entity_id"]): item
            for item in self.model.get("sfig_definitions", [])
        }
        self.attachments = {
            int(item["definition_id"]): item
            for item in self.model.get("attribute_attachments", [])
        }
        self.placement_targets: Dict[int, int] = {
            int(item["placement_id"]): int(item["definition_id"])
            for item in self.model.get("sfig_references", [])
        }
        for attachment in self.model.get("attribute_attachments", []):
            definition_id = int(attachment["definition_id"])
            for placement_id in attachment.get("placement_ids", []):
                self.placement_targets[int(placement_id)] = definition_id
        self.composites = {
            int(item["entity_id"]): item
            for item in self.model.get("composite_curve_definitions", [])
        }
        self.hatch_references = {
            int(item["hatch_id"]): item
            for item in self.model.get("hatch_references", [])
        }
        self.drawing = Drawing(background_color=self._find_background_color())
        self._warning_keys: Set[str] = set()

    def build(self) -> Drawing:
        for warning in self.parsed.get("warnings", []):
            if isinstance(warning, Mapping):
                message = str(warning.get("message") or warning)
            else:
                message = str(warning)
            self.drawing.warnings.append(message)

        sheet = self.model.get("sheet")
        if not isinstance(sheet, Mapping):
            raise ValueError("SFC drawing has no drawing_sheet_feature")
        for component_id in sheet.get("component_ids", []):
            self._render_feature(
                int(component_id),
                IDENTITY,
                layer_override=None,
                parent_visible=True,
                active_definitions=set(),
            )
        return self.drawing

    def warn_once(self, key: str, message: str) -> None:
        if key not in self._warning_keys:
            self._warning_keys.add(key)
            self.drawing.warnings.append(message)

    def simple_geometry(self, feature: Mapping[str, Any]) -> Optional[List[Geometry]]:
        return feature_geometry(feature, self.curve_segments, self.warn_once)

    def make_text(
        self,
        owner: Mapping[str, Any],
        text: Mapping[str, Any],
        style: RenderStyle,
        transform: Affine,
    ) -> TextPrimitive:
        angle = float(text.get("angle_deg", 0.0))
        if int(text.get("direction", 1)) == 2:
            angle += 90.0
        radians = math.radians(angle)
        direction = apply_vector(transform, (math.cos(radians), math.sin(radians)))
        normal = apply_vector(transform, (-math.sin(radians), math.cos(radians)))
        transformed_angle = math.degrees(math.atan2(direction[1], direction[0]))
        return TextPrimitive(
            text=str(text.get("text") or ""),
            anchor=apply(transform, point(text["anchor"])),
            height=abs(float(text.get("height", 0.0))) * math.hypot(*normal),
            width=abs(float(text.get("width", 0.0))) * math.hypot(*direction),
            angle_deg=transformed_angle,
            base_point=int(text.get("base_point", 1)),
            direction=int(text.get("direction", 1)),
            style=style,
            source_id=int(owner["id"]),
        )

    def _find_background_color(self) -> Color:
        for attachment in self.model.get("attribute_attachments", []):
            attribute = attachment.get("attribute", {})
            name = " ".join(
                str(
                    attribute.get("figure_name")
                    or attribute.get("attribute_name")
                    or ""
                ).split()
            )
            if "背景" not in name and "background" not in name.lower():
                continue
            value = str(attribute.get("attribute_value") or "")
            parts = value.replace(",", "_").split("_")
            if len(parts) != 3:
                continue
            try:
                color = tuple(_clamp_color(int(part)) for part in parts)
            except ValueError:
                continue
            return color  # type: ignore[return-value]
        return (255, 255, 255)

    def _render_feature(
        self,
        feature_id: int,
        transform: Affine,
        *,
        layer_override: Optional[int],
        parent_visible: bool,
        active_definitions: Set[int],
    ) -> None:
        feature = self.features.get(feature_id)
        if feature is None:
            self.warn_once(
                "missing-feature-{0}".format(feature_id),
                "Drawing component #{0} is missing from typed features".format(
                    feature_id
                ),
            )
            return
        kind = str(feature.get("kind"))
        if kind == "sfig_locate":
            self._render_placement(
                feature,
                transform,
                layer_override=layer_override,
                parent_visible=parent_visible,
                active_definitions=active_definitions,
            )
            return
        if kind in {
            "externally_defined_hatch",
            "fill_area_style_colour",
            "fill_area_style_hatching",
            "fill_area_style_tiles",
        }:
            render_hatch(
                self,
                feature,
                transform,
                layer_override=layer_override,
                parent_visible=parent_visible,
            )
            return

        style = self.styles.resolve(
            feature.get("style"),
            layer_override=layer_override,
            parent_visible=parent_visible,
        )
        geometries = self.simple_geometry(feature)
        if geometries is not None:
            for points, closed in geometries:
                if len(points) >= 2:
                    self.drawing.paths.append(
                        PathPrimitive(
                            points=tuple(apply(transform, value) for value in points),
                            closed=closed,
                            style=style,
                            source_id=feature_id,
                        )
                    )
            return

        if kind == "text":
            self.drawing.texts.append(
                self.make_text(feature, feature, style, transform)
            )
        elif kind == "point_marker":
            self.drawing.markers.append(
                MarkerPrimitive(
                    position=apply(transform, point(feature["position"])),
                    marker_code=int(feature.get("marker_code", 0)),
                    scale=float(feature.get("scale", 1.0)) * average_scale(transform),
                    style=style,
                    source_id=feature_id,
                )
            )
        elif kind == "externally_defined_symbol":
            self.drawing.markers.append(
                MarkerPrimitive(
                    position=apply(transform, point(feature["position"])),
                    marker_code=0,
                    scale=float(feature.get("scale", 1.0)) * average_scale(transform),
                    style=style,
                    source_id=feature_id,
                    name=str(feature.get("name") or ""),
                )
            )
            self.warn_once(
                "external-symbol",
                "Externally defined symbols are represented by insertion-point markers",
            )
        elif kind in {
            "linear_dim",
            "curve_dim",
            "angular_dim",
            "radius_dim",
            "diameter_dim",
            "label",
            "balloon",
        }:
            render_annotation(self, feature, transform, style)
        elif kind not in _NON_RENDERED_FEATURES:
            self.warn_once(
                "unsupported-{0}".format(kind),
                "Drawing conversion does not yet render feature kind {0!r}".format(
                    kind
                ),
            )

    def _render_placement(
        self,
        feature: Mapping[str, Any],
        transform: Affine,
        *,
        layer_override: Optional[int],
        parent_visible: bool,
        active_definitions: Set[int],
    ) -> None:
        placement_id = int(feature["id"])
        definition_id = self.placement_targets.get(placement_id)
        if definition_id is None:
            self.warn_once(
                "unresolved-placement-{0}".format(placement_id),
                "Compound-figure placement #{0} has no resolved definition".format(
                    placement_id
                ),
            )
            return
        if definition_id in active_definitions:
            self.warn_once(
                "cyclic-placement-{0}".format(definition_id),
                "Cyclic compound-figure definition #{0} was not expanded".format(
                    definition_id
                ),
            )
            return

        raw_layer = optional_int(feature.get("style", {}).get("layer_code"))
        placement_visible = parent_visible and self.styles.layer_visible(raw_layer)
        next_layer = layer_override
        if next_layer is None and raw_layer not in (None, 0):
            next_layer = raw_layer
        angle = math.radians(float(feature.get("angle_deg", 0.0)))
        ratio_x = float(feature.get("ratio_x", 1.0))
        ratio_y = float(feature.get("ratio_y", 1.0))
        position = point(feature["position"])
        local: Affine = (
            math.cos(angle) * ratio_x,
            math.sin(angle) * ratio_x,
            -math.sin(angle) * ratio_y,
            math.cos(angle) * ratio_y,
            position[0],
            position[1],
        )
        definition = self.definitions.get(definition_id) or self.attachments.get(
            definition_id
        )
        if definition is None:
            self.warn_once(
                "missing-definition-{0}".format(definition_id),
                "Resolved compound-figure definition #{0} is missing".format(
                    definition_id
                ),
            )
            return

        active = set(active_definitions)
        active.add(definition_id)
        placed_transform = compose(transform, local)
        for component_id in definition.get("component_ids", []):
            self._render_feature(
                int(component_id),
                placed_transform,
                layer_override=next_layer,
                parent_visible=placement_visible,
                active_definitions=active,
            )


_NON_RENDERED_FEATURES = {
    "composite_curve",
    "drawing_attribute",
    "drawing_sheet",
    "layer",
    "pre_defined_font",
    "user_defined_font",
    "pre_defined_colour",
    "user_defined_colour",
    "width",
    "text_font",
    "sfig_org",
}


def _clamp_color(value: int) -> int:
    return max(0, min(255, value))


__all__ = ["DrawingBuilder"]
