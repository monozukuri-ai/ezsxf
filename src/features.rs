//! Typed SFC feature decoding, code tables, and semantic validation.

use encoding_rs::SHIFT_JIS;

use crate::model::*;

pub(crate) fn normalized_predefined_name(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

pub(crate) fn predefined_line_type_code(name: &str) -> Option<i64> {
    match normalized_predefined_name(name).as_str() {
        "continuous" => Some(1),
        "dashed" => Some(2),
        "dashed spaced" => Some(3),
        "long dashed dotted" => Some(4),
        "long dashed double-dotted" => Some(5),
        "long dashed triplicate-dotted" => Some(6),
        "dotted" => Some(7),
        "chain" => Some(8),
        "chain double dash" => Some(9),
        "dashed dotted" => Some(10),
        "double-dashed dotted" => Some(11),
        "dashed double-dotted" => Some(12),
        "double-dashed double-dotted" => Some(13),
        "dashed triplicate-dotted" => Some(14),
        "double-dashed triplicate-dotted" => Some(15),
        _ => None,
    }
}

pub(crate) fn predefined_color_code(name: &str) -> Option<i64> {
    match normalized_predefined_name(name).as_str() {
        "black" => Some(1),
        "red" => Some(2),
        "green" => Some(3),
        "blue" => Some(4),
        "yellow" => Some(5),
        "magenta" => Some(6),
        "cyan" => Some(7),
        "white" => Some(8),
        "deeppink" => Some(9),
        "brown" => Some(10),
        "orange" => Some(11),
        "lightgreen" => Some(12),
        "lightblue" => Some(13),
        "lavender" => Some(14),
        "lightgray" => Some(15),
        "darkgray" => Some(16),
        _ => None,
    }
}

pub(crate) fn predefined_line_width_code(width: f64) -> Option<i64> {
    const WIDTHS: [(i64, f64); 9] = [
        (1, 0.13),
        (2, 0.18),
        (3, 0.25),
        (4, 0.35),
        (5, 0.5),
        (6, 0.7),
        (7, 1.0),
        (8, 1.4),
        (9, 2.0),
    ];
    WIDTHS
        .iter()
        .find(|(_, expected)| (width - expected).abs() <= 1.0e-9)
        .map(|(code, _)| *code)
}

pub(crate) fn is_composite_curve_component(feature: &TypedFeature) -> bool {
    matches!(
        feature,
        TypedFeature::Arc(_)
            | TypedFeature::EllipseArc(_)
            | TypedFeature::Polyline(_)
            | TypedFeature::Spline(_)
    )
}

pub(crate) fn hatch_composite_curve_codes(feature: &TypedFeature) -> Option<(i64, &[i64])> {
    match feature {
        TypedFeature::ExternallyDefinedHatch(value) => Some((value.out_id, &value.in_ids)),
        TypedFeature::FillAreaStyleColour(value) => Some((value.out_id, &value.in_ids)),
        TypedFeature::FillAreaStyleHatching(value) => Some((value.out_id, &value.in_ids)),
        TypedFeature::FillAreaStyleTiles(value) => Some((value.out_id, &value.in_ids)),
        _ => None,
    }
}

pub(crate) fn resolve_composite_curve_definition(
    definitions: &[SfcCompositeCurveDefinition],
    code: i64,
) -> Option<&SfcCompositeCurveDefinition> {
    definitions
        .iter()
        .find(|definition| definition.code == code)
}

#[derive(Clone, Copy)]
pub(crate) enum SfcCodeKind {
    Color,
    LineType,
    LineWidth,
    TextFont,
}

pub(crate) fn additional_sfc_code_references(feature: &TypedFeature) -> Vec<(SfcCodeKind, i64)> {
    let mut references = Vec::new();
    let mut push_text_font = |text: &FeatureText| {
        if text.present_flag == 1 {
            references.push((SfcCodeKind::TextFont, text.font_code));
        }
    };
    match feature {
        TypedFeature::LinearDim(value) => push_text_font(&value.text),
        TypedFeature::CurveDim(value) => push_text_font(&value.text),
        TypedFeature::AngularDim(value) => push_text_font(&value.text),
        TypedFeature::RadiusDim(value) => push_text_font(&value.text),
        TypedFeature::DiameterDim(value) => push_text_font(&value.text),
        TypedFeature::Label(value) => push_text_font(&value.text),
        TypedFeature::Balloon(value) => push_text_font(&value.text),
        TypedFeature::FillAreaStyleHatching(value) => {
            for pattern in &value.patterns {
                references.push((SfcCodeKind::Color, pattern.color_code));
                references.push((SfcCodeKind::LineType, pattern.line_type_code));
                references.push((SfcCodeKind::LineWidth, pattern.line_width_code));
            }
        }
        TypedFeature::FillAreaStyleTiles(value) => {
            references.push((SfcCodeKind::Color, value.hatch_color));
        }
        _ => {}
    }
    references
}

pub(crate) fn parse_line_feature(params: &[Value]) -> Result<LineFeature, String> {
    require_parameter_count(params, 8, "line_feature")?;
    let style = parse_line_style(params)?;
    let start_x = parse_required_f64(&params[4], "start_x")?;
    let start_y = parse_required_f64(&params[5], "start_y")?;
    let end_x = parse_required_f64(&params[6], "end_x")?;
    let end_y = parse_required_f64(&params[7], "end_y")?;
    Ok(LineFeature {
        style,
        start: Point2 {
            x: start_x,
            y: start_y,
        },
        end: Point2 { x: end_x, y: end_y },
    })
}

pub(crate) fn parse_drawing_attribute_feature(
    params: &[Value],
) -> Result<DrawingAttributeFeature, String> {
    require_parameter_count(params, 12, "drawing_attribute_feature")?;

    Ok(DrawingAttributeFeature {
        project_name: parse_required_string(&params[0], "project_name")?,
        construction_name: parse_required_string(&params[1], "construction_name")?,
        contract_type: parse_required_string(&params[2], "contract_type")?,
        drawing_name: parse_required_string(&params[3], "drawing_name")?,
        drawing_number: parse_required_string(&params[4], "drawing_number")?,
        drawing_type: parse_required_string(&params[5], "drawing_type")?,
        drawing_scale: parse_required_string(&params[6], "drawing_scale")?,
        drawing_year: parse_required_i64(&params[7], "drawing_year")?,
        drawing_month: parse_required_i64(&params[8], "drawing_month")?,
        drawing_day: parse_required_i64(&params[9], "drawing_day")?,
        contractor_name: parse_required_string(&params[10], "contractor_name")?,
        owner_name: parse_required_string(&params[11], "owner_name")?,
    })
}

pub(crate) fn parse_drawing_sheet_feature(params: &[Value]) -> Result<DrawingSheetFeature, String> {
    require_parameter_count(params, 5, "drawing_sheet_feature")?;

    Ok(DrawingSheetFeature {
        name: parse_required_string(&params[0], "name")?,
        sheet_type: parse_required_i64(&params[1], "type")?,
        orientation: parse_required_i64(&params[2], "orientation")?,
        free_x_mm: parse_required_i64(&params[3], "x")?,
        free_y_mm: parse_required_i64(&params[4], "y")?,
    })
}

pub(crate) fn parse_layer_feature(params: &[Value]) -> Result<LayerFeature, String> {
    require_parameter_count(params, 2, "layer_feature")?;

    Ok(LayerFeature {
        name: parse_required_string(&params[0], "name")?,
        visibility_flag: parse_required_i64(&params[1], "lflag")?,
    })
}

pub(crate) fn parse_pre_defined_font_feature(
    params: &[Value],
) -> Result<PreDefinedFontFeature, String> {
    require_parameter_count(params, 1, "pre_defined_font_feature")?;
    Ok(PreDefinedFontFeature {
        name: parse_required_string(&params[0], "name")?,
    })
}

pub(crate) fn parse_user_defined_font_feature(
    params: &[Value],
) -> Result<UserDefinedFontFeature, String> {
    require_parameter_count(params, 3, "user_defined_font_feature")?;
    let segment_count = parse_required_i64(&params[1], "segment")?;
    let pitch = parse_required_f64_list(&params[2], "pitch")?;
    validate_declared_count(
        segment_count,
        pitch.len(),
        "user_defined_font_feature pitch",
    )?;
    Ok(UserDefinedFontFeature {
        name: parse_required_string(&params[0], "name")?,
        segment_count,
        pitch,
    })
}

pub(crate) fn parse_pre_defined_colour_feature(
    params: &[Value],
) -> Result<PreDefinedColourFeature, String> {
    require_parameter_count(params, 1, "pre_defined_colour_feature")?;
    Ok(PreDefinedColourFeature {
        name: parse_required_string(&params[0], "name")?,
    })
}

pub(crate) fn parse_user_defined_colour_feature(
    params: &[Value],
) -> Result<UserDefinedColourFeature, String> {
    require_parameter_count(params, 3, "user_defined_colour_feature")?;
    Ok(UserDefinedColourFeature {
        red: parse_required_i64(&params[0], "red")?,
        green: parse_required_i64(&params[1], "green")?,
        blue: parse_required_i64(&params[2], "blue")?,
    })
}

pub(crate) fn parse_width_feature(params: &[Value]) -> Result<WidthFeature, String> {
    require_parameter_count(params, 1, "width_feature")?;
    Ok(WidthFeature {
        width: parse_required_f64(&params[0], "width")?,
    })
}

pub(crate) fn parse_text_font_feature(params: &[Value]) -> Result<TextFontFeature, String> {
    require_parameter_count(params, 1, "text_font_feature")?;
    Ok(TextFontFeature {
        name: parse_required_string(&params[0], "name")?,
    })
}

pub(crate) fn parse_point_marker_feature(params: &[Value]) -> Result<PointMarkerFeature, String> {
    require_parameter_count(params, 7, "point_marker_feature")?;
    let style = parse_layer_color_style(params)?;
    Ok(PointMarkerFeature {
        style,
        position: Point2 {
            x: parse_required_f64(&params[2], "start_x")?,
            y: parse_required_f64(&params[3], "start_y")?,
        },
        marker_code: parse_required_i64(&params[4], "marker_code")?,
        rotation_angle_deg: parse_required_f64(&params[5], "rotation_angle")?,
        scale: parse_required_f64(&params[6], "scale")?,
    })
}

pub(crate) fn parse_polyline_feature(params: &[Value]) -> Result<PolylineFeature, String> {
    require_parameter_count(params, 7, "polyline_feature")?;
    let style = parse_line_style(params)?;
    let declared = parse_required_i64(&params[4], "number")?;
    let xs = parse_required_f64_list(&params[5], "x coordinates")?;
    let ys = parse_required_f64_list(&params[6], "y coordinates")?;
    if xs.len() != ys.len() {
        return Err(format!(
            "polyline_feature coordinate list lengths differ: x={}, y={}",
            xs.len(),
            ys.len()
        ));
    }
    validate_declared_count(declared, xs.len(), "polyline_feature vertices")?;
    if xs.is_empty() {
        return Err("polyline_feature has no vertices".to_string());
    }

    let mut points = Vec::with_capacity(xs.len());
    for i in 0..xs.len() {
        points.push(Point2 { x: xs[i], y: ys[i] });
    }

    Ok(PolylineFeature {
        style,
        declared_point_count: usize::try_from(declared).ok(),
        points,
    })
}

pub(crate) fn parse_circle_feature(params: &[Value]) -> Result<CircleFeature, String> {
    require_parameter_count(params, 7, "circle_feature")?;
    let style = parse_line_style(params)?;
    Ok(CircleFeature {
        style,
        center: Point2 {
            x: parse_required_f64(&params[4], "center_x")?,
            y: parse_required_f64(&params[5], "center_y")?,
        },
        radius: parse_required_f64(&params[6], "radius")?,
    })
}

pub(crate) fn parse_arc_feature(params: &[Value]) -> Result<ArcFeature, String> {
    require_parameter_count(params, 10, "arc_feature")?;
    let style = parse_line_style(params)?;
    Ok(ArcFeature {
        style,
        center: Point2 {
            x: parse_required_f64(&params[4], "center_x")?,
            y: parse_required_f64(&params[5], "center_y")?,
        },
        radius: parse_required_f64(&params[6], "radius")?,
        direction_flag: parse_required_i64(&params[7], "direction")?,
        start_angle_deg: parse_required_f64(&params[8], "start_angle")?,
        end_angle_deg: parse_required_f64(&params[9], "end_angle")?,
    })
}

pub(crate) fn parse_ellipse_feature(params: &[Value]) -> Result<EllipseFeature, String> {
    require_parameter_count(params, 9, "ellipse_feature")?;
    let style = parse_line_style(params)?;
    Ok(EllipseFeature {
        style,
        center: Point2 {
            x: parse_required_f64(&params[4], "center_x")?,
            y: parse_required_f64(&params[5], "center_y")?,
        },
        radius_x: parse_required_f64(&params[6], "radius_x")?,
        radius_y: parse_required_f64(&params[7], "radius_y")?,
        rotation_angle_deg: parse_required_f64(&params[8], "rotation_angle")?,
    })
}

pub(crate) fn parse_ellipse_arc_feature(params: &[Value]) -> Result<EllipseArcFeature, String> {
    require_parameter_count(params, 12, "ellipse_arc_feature")?;
    let style = parse_line_style(params)?;
    Ok(EllipseArcFeature {
        style,
        center: Point2 {
            x: parse_required_f64(&params[4], "center_x")?,
            y: parse_required_f64(&params[5], "center_y")?,
        },
        radius_x: parse_required_f64(&params[6], "radius_x")?,
        radius_y: parse_required_f64(&params[7], "radius_y")?,
        direction_flag: parse_required_i64(&params[8], "direction")?,
        rotation_angle_deg: parse_required_f64(&params[9], "rotation_angle")?,
        start_angle_deg: parse_required_f64(&params[10], "start_angle")?,
        end_angle_deg: parse_required_f64(&params[11], "end_angle")?,
    })
}

pub(crate) fn parse_text_feature(params: &[Value]) -> Result<TextFeature, String> {
    require_parameter_count(params, 13, "text_string_feature")?;
    Ok(TextFeature {
        style: CommonStyle {
            layer_code: parse_optional_i64(&params[0]),
            color_code: parse_optional_i64(&params[1]),
            font_code: parse_optional_i64(&params[2]),
            ..Default::default()
        },
        text: parse_required_string(&params[3], "str")?,
        anchor: Point2 {
            x: parse_required_f64(&params[4], "text_x")?,
            y: parse_required_f64(&params[5], "text_y")?,
        },
        height: parse_required_f64(&params[6], "height")?,
        width: parse_required_f64(&params[7], "width")?,
        spacing: parse_required_f64(&params[8], "spacing")?,
        angle_deg: parse_required_f64(&params[9], "angle")?,
        slant_deg: parse_required_f64(&params[10], "slant")?,
        base_point: parse_required_i64(&params[11], "base_point")?,
        direction: parse_required_i64(&params[12], "direction")?,
    })
}

pub(crate) fn parse_spline_feature(params: &[Value]) -> Result<SplineFeature, String> {
    require_parameter_count(params, 8, "spline_feature")?;
    let style = parse_line_style(params)?;
    let declared = parse_required_i64(&params[5], "number")?;
    let xs = parse_required_f64_list(&params[6], "x coordinates")?;
    let ys = parse_required_f64_list(&params[7], "y coordinates")?;
    if xs.len() != ys.len() {
        return Err(format!(
            "spline_feature coordinate list lengths differ: x={}, y={}",
            xs.len(),
            ys.len()
        ));
    }
    validate_declared_count(declared, xs.len(), "spline_feature control points")?;
    if xs.is_empty() {
        return Err("spline_feature has no control points".to_string());
    }

    let mut points = Vec::with_capacity(xs.len());
    for i in 0..xs.len() {
        points.push(Point2 { x: xs[i], y: ys[i] });
    }

    Ok(SplineFeature {
        style,
        open_close: parse_required_i64(&params[4], "open_close")?,
        declared_point_count: usize::try_from(declared).ok(),
        points,
    })
}

pub(crate) fn parse_clothoid_feature(params: &[Value]) -> Result<ClothoidFeature, String> {
    require_parameter_count(params, 11, "clothoid_feature")?;
    let style = parse_line_style(params)?;
    Ok(ClothoidFeature {
        style,
        base: Point2 {
            x: parse_required_f64(&params[4], "base_x")?,
            y: parse_required_f64(&params[5], "base_y")?,
        },
        parameter: parse_required_f64(&params[6], "parameter")?,
        direction_flag: parse_required_i64(&params[7], "direction")?,
        angle_deg: parse_required_f64(&params[8], "angle")?,
        start_length: parse_required_f64(&params[9], "start_length")?,
        end_length: parse_required_f64(&params[10], "end_length")?,
    })
}

pub(crate) fn parse_composite_curve_feature(
    params: &[Value],
) -> Result<CompositeCurveFeature, String> {
    require_parameter_count(params, 4, "composite_curve_feature")?;
    Ok(CompositeCurveFeature {
        style: CommonStyle {
            color_code: parse_optional_i64(&params[0]),
            line_type_code: parse_optional_i64(&params[1]),
            line_width_code: parse_optional_i64(&params[2]),
            ..Default::default()
        },
        visibility_flag: parse_required_i64(&params[3], "invisibility")?,
    })
}

pub(crate) fn parse_sfig_org_feature(params: &[Value]) -> Result<SfigOrgFeature, String> {
    require_parameter_count(params, 2, "sfig_org_feature")?;
    Ok(SfigOrgFeature {
        name: parse_required_string(&params[0], "name")?,
        kind_flag: parse_required_i64(&params[1], "kind_flag")?,
    })
}

pub(crate) fn parse_sfig_locate_feature(params: &[Value]) -> Result<SfigLocateFeature, String> {
    require_parameter_count(params, 7, "sfig_locate_feature")?;
    Ok(SfigLocateFeature {
        style: CommonStyle {
            layer_code: Some(parse_required_i64(&params[0], "layer")?),
            ..Default::default()
        },
        name: parse_required_string(&params[1], "name")?,
        position: Point2 {
            x: parse_required_f64(&params[2], "x")?,
            y: parse_required_f64(&params[3], "y")?,
        },
        angle_deg: parse_required_f64(&params[4], "angle")?,
        ratio_x: parse_required_f64(&params[5], "ratio_x")?,
        ratio_y: parse_required_f64(&params[6], "ratio_y")?,
    })
}

pub(crate) fn parse_externally_defined_symbol_feature(
    params: &[Value],
) -> Result<ExternallyDefinedSymbolFeature, String> {
    require_parameter_count(params, 8, "symbol_externally_defined_feature")?;
    Ok(ExternallyDefinedSymbolFeature {
        style: CommonStyle {
            layer_code: Some(parse_required_i64(&params[0], "layer")?),
            color_code: Some(parse_required_i64(&params[2], "color")?),
            ..Default::default()
        },
        color_flag: parse_required_i64(&params[1], "color_flag")?,
        name: parse_required_string(&params[3], "name")?,
        position: Point2 {
            x: parse_required_f64(&params[4], "x")?,
            y: parse_required_f64(&params[5], "y")?,
        },
        rotation_angle_deg: parse_required_f64(&params[6], "rotation_angle")?,
        scale: parse_required_f64(&params[7], "scale")?,
    })
}

pub(crate) fn parse_linear_dim_feature(params: &[Value]) -> Result<LinearDimFeature, String> {
    require_parameter_count(params, 44, "linear_dim_feature")?;
    Ok(LinearDimFeature {
        style: parse_line_style(params)?,
        start: parse_point_at(params, 4, 5, "dimension start")?,
        end: parse_point_at(params, 6, 7, "dimension end")?,
        extension_line1: parse_extension_line(params, 8, "extension line 1")?,
        extension_line2: parse_extension_line(params, 15, "extension line 2")?,
        arrow1: parse_dimension_arrow(params, 22, "arrow 1")?,
        arrow2: parse_dimension_arrow(params, 27, "arrow 2")?,
        text: parse_feature_text(params, 32)?,
        raw_parameters: params[4..].to_vec(),
    })
}

pub(crate) fn parse_curve_dim_feature(params: &[Value]) -> Result<CurveDimFeature, String> {
    require_parameter_count(params, 45, "curve_dim_feature")?;
    Ok(CurveDimFeature {
        style: parse_line_style(params)?,
        center: parse_point_at(params, 4, 5, "dimension center")?,
        radius: parse_required_f64(&params[6], "dimension radius")?,
        start_angle_deg: parse_required_f64(&params[7], "dimension start angle")?,
        end_angle_deg: parse_required_f64(&params[8], "dimension end angle")?,
        extension_line1: parse_extension_line(params, 9, "extension line 1")?,
        extension_line2: parse_extension_line(params, 16, "extension line 2")?,
        arrow1: parse_dimension_arrow(params, 23, "arrow 1")?,
        arrow2: parse_dimension_arrow(params, 28, "arrow 2")?,
        text: parse_feature_text(params, 33)?,
        raw_parameters: params[4..].to_vec(),
    })
}

pub(crate) fn parse_angular_dim_feature(params: &[Value]) -> Result<AngularDimFeature, String> {
    require_parameter_count(params, 45, "angular_dim_feature")?;
    Ok(AngularDimFeature {
        style: parse_line_style(params)?,
        center: parse_point_at(params, 4, 5, "dimension center")?,
        radius: parse_required_f64(&params[6], "dimension radius")?,
        start_angle_deg: parse_required_f64(&params[7], "dimension start angle")?,
        end_angle_deg: parse_required_f64(&params[8], "dimension end angle")?,
        extension_line1: parse_extension_line(params, 9, "extension line 1")?,
        extension_line2: parse_extension_line(params, 16, "extension line 2")?,
        arrow1: parse_dimension_arrow(params, 23, "arrow 1")?,
        arrow2: parse_dimension_arrow(params, 28, "arrow 2")?,
        text: parse_feature_text(params, 33)?,
        raw_parameters: params[4..].to_vec(),
    })
}

pub(crate) fn parse_radius_dim_feature(params: &[Value]) -> Result<RadiusDimFeature, String> {
    require_parameter_count(params, 25, "radius_dim_feature")?;
    Ok(RadiusDimFeature {
        style: parse_line_style(params)?,
        start: parse_point_at(params, 4, 5, "dimension start")?,
        end: parse_point_at(params, 6, 7, "dimension end")?,
        arrow: parse_dimension_arrow(params, 8, "arrow")?,
        text: parse_feature_text(params, 13)?,
        raw_parameters: params[4..].to_vec(),
    })
}

pub(crate) fn parse_diameter_dim_feature(params: &[Value]) -> Result<DiameterDimFeature, String> {
    require_parameter_count(params, 30, "diameter_dim_feature")?;
    Ok(DiameterDimFeature {
        style: parse_line_style(params)?,
        start: parse_point_at(params, 4, 5, "dimension start")?,
        end: parse_point_at(params, 6, 7, "dimension end")?,
        arrow1: parse_dimension_arrow(params, 8, "arrow 1")?,
        arrow2: parse_dimension_arrow(params, 13, "arrow 2")?,
        text: parse_feature_text(params, 18)?,
        raw_parameters: params[4..].to_vec(),
    })
}

pub(crate) fn parse_point_at(
    params: &[Value],
    x_index: usize,
    y_index: usize,
    name: &str,
) -> Result<Point2, String> {
    Ok(Point2 {
        x: parse_required_f64(&params[x_index], &format!("{name} x"))?,
        y: parse_required_f64(&params[y_index], &format!("{name} y"))?,
    })
}

pub(crate) fn parse_extension_line(
    params: &[Value],
    start_index: usize,
    name: &str,
) -> Result<ExtensionLine, String> {
    Ok(ExtensionLine {
        present_flag: parse_required_i64(&params[start_index], &format!("{name} flag"))?,
        base: parse_point_at(
            params,
            start_index + 1,
            start_index + 2,
            &format!("{name} base"),
        )?,
        start: parse_point_at(
            params,
            start_index + 3,
            start_index + 4,
            &format!("{name} start"),
        )?,
        end: parse_point_at(
            params,
            start_index + 5,
            start_index + 6,
            &format!("{name} end"),
        )?,
    })
}

pub(crate) fn parse_dimension_arrow(
    params: &[Value],
    start_index: usize,
    name: &str,
) -> Result<DimensionArrow, String> {
    Ok(DimensionArrow {
        code: parse_required_i64(&params[start_index], &format!("{name} code"))?,
        direction_flag: parse_required_i64(&params[start_index + 1], &format!("{name} direction"))?,
        position: parse_point_at(
            params,
            start_index + 2,
            start_index + 3,
            &format!("{name} position"),
        )?,
        scale: parse_required_f64(&params[start_index + 4], &format!("{name} scale"))?,
    })
}

pub(crate) fn parse_feature_text(
    params: &[Value],
    start_index: usize,
) -> Result<FeatureText, String> {
    Ok(FeatureText {
        present_flag: parse_required_i64(&params[start_index], "text present flag")?,
        font_code: parse_required_i64(&params[start_index + 1], "text font")?,
        text: parse_required_string(&params[start_index + 2], "text")?,
        anchor: parse_point_at(params, start_index + 3, start_index + 4, "text anchor")?,
        height: parse_required_f64(&params[start_index + 5], "text height")?,
        width: parse_required_f64(&params[start_index + 6], "text width")?,
        spacing: parse_required_f64(&params[start_index + 7], "text spacing")?,
        angle_deg: parse_required_f64(&params[start_index + 8], "text angle")?,
        slant_deg: parse_required_f64(&params[start_index + 9], "text slant")?,
        base_point: parse_required_i64(&params[start_index + 10], "text base point")?,
        direction: parse_required_i64(&params[start_index + 11], "text direction")?,
    })
}

pub(crate) fn parse_points_from_xy_lists(
    xs: Vec<f64>,
    ys: Vec<f64>,
    feature_name: &str,
) -> Result<Vec<Point2>, String> {
    let point_count = xs.len().min(ys.len());
    if point_count == 0 {
        return Err(format!("{feature_name} has no vertices"));
    }
    let mut points = Vec::with_capacity(point_count);
    for i in 0..point_count {
        points.push(Point2 { x: xs[i], y: ys[i] });
    }
    Ok(points)
}

pub(crate) fn parse_label_feature(params: &[Value]) -> Result<LabelFeature, String> {
    require_parameter_count(params, 21, "label_feature")?;
    let style = parse_line_style(params)?;
    let declared = parse_required_i64(&params[4], "number")?;
    let points = parse_points_from_xy_lists(
        parse_required_f64_list(&params[5], "x coordinates")?,
        parse_required_f64_list(&params[6], "y coordinates")?,
        "label_feature",
    )?;
    Ok(LabelFeature {
        style,
        declared_vertex_count: usize::try_from(declared).ok(),
        vertices: points,
        arrow: LeaderArrow {
            code: parse_required_i64(&params[7], "arrow code")?,
            scale: parse_required_f64(&params[8], "arrow scale")?,
        },
        text: parse_feature_text(params, 9)?,
    })
}

pub(crate) fn parse_balloon_feature(params: &[Value]) -> Result<BalloonFeature, String> {
    require_parameter_count(params, 24, "balloon_feature")?;
    let style = parse_line_style(params)?;
    let declared = parse_required_i64(&params[4], "number")?;
    let points = parse_points_from_xy_lists(
        parse_required_f64_list(&params[5], "x coordinates")?,
        parse_required_f64_list(&params[6], "y coordinates")?,
        "balloon_feature",
    )?;
    Ok(BalloonFeature {
        style,
        declared_vertex_count: usize::try_from(declared).ok(),
        vertices: points,
        center: Point2 {
            x: parse_required_f64(&params[7], "center_x")?,
            y: parse_required_f64(&params[8], "center_y")?,
        },
        radius: parse_required_f64(&params[9], "radius")?,
        arrow: LeaderArrow {
            code: parse_required_i64(&params[10], "arrow code")?,
            scale: parse_required_f64(&params[11], "arrow scale")?,
        },
        text: parse_feature_text(params, 12)?,
    })
}

pub(crate) fn parse_externally_defined_hatch_feature(
    params: &[Value],
) -> Result<ExternallyDefinedHatchFeature, String> {
    require_parameter_count(params, 5, "externally_defined_hatch_feature")?;
    let hole_count = parse_required_i64(&params[3], "number")?;
    let in_ids = parse_required_i64_list(&params[4], "in_ids")?;
    validate_declared_count(
        hole_count,
        in_ids.len(),
        "externally_defined_hatch_feature holes",
    )?;
    Ok(ExternallyDefinedHatchFeature {
        style: CommonStyle {
            layer_code: Some(parse_required_i64(&params[0], "layer")?),
            ..Default::default()
        },
        name: parse_required_string(&params[1], "name")?,
        out_id: parse_required_i64(&params[2], "out_id")?,
        hole_count: usize::try_from(hole_count).ok(),
        in_ids,
    })
}

pub(crate) fn parse_fill_area_style_colour_feature(
    params: &[Value],
) -> Result<FillAreaStyleColourFeature, String> {
    require_parameter_count(params, 5, "fill_area_style_colour_feature")?;
    let hole_count = parse_required_i64(&params[3], "number")?;
    let in_ids = parse_required_i64_list(&params[4], "in_ids")?;
    validate_declared_count(
        hole_count,
        in_ids.len(),
        "fill_area_style_colour_feature holes",
    )?;
    Ok(FillAreaStyleColourFeature {
        style: CommonStyle {
            layer_code: Some(parse_required_i64(&params[0], "layer")?),
            color_code: Some(parse_required_i64(&params[1], "color")?),
            ..Default::default()
        },
        out_id: parse_required_i64(&params[2], "out_id")?,
        hole_count: usize::try_from(hole_count).ok(),
        in_ids,
    })
}

pub(crate) fn parse_fill_area_style_hatching_feature(
    params: &[Value],
) -> Result<FillAreaStyleHatchingFeature, String> {
    if params.len() < 6 {
        return Err(format!(
            "fill_area_style_hatching_feature requires at least 6 parameters, got {}",
            params.len()
        ));
    }

    let hatch_number = parse_required_i64(&params[1], "hatch_number")?;
    let hatch_count = usize::try_from(hatch_number)
        .map_err(|_| "hatch_number must be a non-negative integer".to_string())?;
    let hatch_end = 2 + hatch_count;
    if params.len() != hatch_end + 3 {
        return Err(format!(
            "fill_area_style_hatching_feature requires exactly {} parameters for {} hatch patterns, got {}",
            hatch_end + 3,
            hatch_count,
            params.len()
        ));
    }

    let raw_patterns = params[2..hatch_end].to_vec();
    let patterns = raw_patterns
        .iter()
        .enumerate()
        .map(|(index, value)| parse_hatch_line_pattern(value, index + 1))
        .collect::<Result<Vec<_>, _>>()?;

    let hole_count = parse_required_i64(&params[hatch_end + 1], "number")?;
    let in_ids = parse_required_i64_list(&params[hatch_end + 2], "in_ids")?;
    validate_declared_count(
        hole_count,
        in_ids.len(),
        "fill_area_style_hatching_feature holes",
    )?;
    Ok(FillAreaStyleHatchingFeature {
        style: CommonStyle {
            layer_code: Some(parse_required_i64(&params[0], "layer")?),
            ..Default::default()
        },
        hatch_number,
        hatch_patterns: raw_patterns,
        patterns,
        out_id: parse_required_i64(&params[hatch_end], "out_id")?,
        hole_count: usize::try_from(hole_count).ok(),
        in_ids,
    })
}

pub(crate) fn parse_hatch_line_pattern(
    value: &Value,
    index: usize,
) -> Result<HatchLinePattern, String> {
    let values = value
        .as_f64_list()
        .ok_or_else(|| format!("hatch pattern {index} must be a seven-value numeric list"))?;
    if values.len() != 7 {
        return Err(format!(
            "hatch pattern {index} requires 7 values, got {}",
            values.len()
        ));
    }
    let as_code = |value: f64, name: &str| -> Result<i64, String> {
        if !value.is_finite() || value.fract() != 0.0 {
            return Err(format!("hatch pattern {index} {name} must be an integer"));
        }
        Ok(value as i64)
    };
    Ok(HatchLinePattern {
        color_code: as_code(values[0], "color code")?,
        line_type_code: as_code(values[1], "line type code")?,
        line_width_code: as_code(values[2], "line width code")?,
        start: Point2 {
            x: values[3],
            y: values[4],
        },
        spacing: values[5],
        angle_deg: values[6],
    })
}

pub(crate) fn parse_fill_area_style_tiles_feature(
    params: &[Value],
) -> Result<FillAreaStyleTilesFeature, String> {
    require_parameter_count(params, 15, "fill_area_style_tiles_hatching_feature")?;
    let hole_count = parse_required_i64(&params[13], "number")?;
    let in_ids = parse_required_i64_list(&params[14], "in_ids")?;
    validate_declared_count(
        hole_count,
        in_ids.len(),
        "fill_area_style_tiles_hatching_feature holes",
    )?;
    Ok(FillAreaStyleTilesFeature {
        style: CommonStyle {
            layer_code: Some(parse_required_i64(&params[0], "layer")?),
            ..Default::default()
        },
        name: parse_required_string(&params[1], "name")?,
        hatch_color: parse_required_i64(&params[2], "hatch_color")?,
        hatch_pattern_position: Point2 {
            x: parse_required_f64(&params[3], "hatch_pattern_position_x")?,
            y: parse_required_f64(&params[4], "hatch_pattern_position_y")?,
        },
        out_id: parse_required_i64(&params[12], "out_id")?,
        hatch_pattern_vector1: parse_required_f64(&params[5], "hatch_pattern_vector1")?,
        hatch_pattern_vector1_angle_deg: parse_required_f64(
            &params[6],
            "hatch_pattern_vector1_angle",
        )?,
        hatch_pattern_vector2: parse_required_f64(&params[7], "hatch_pattern_vector2")?,
        hatch_pattern_vector2_angle_deg: parse_required_f64(
            &params[8],
            "hatch_pattern_vector2_angle",
        )?,
        hatch_pattern_scale_x: parse_required_f64(&params[9], "hatch_pattern_scale_x")?,
        hatch_pattern_scale_y: parse_required_f64(&params[10], "hatch_pattern_scale_y")?,
        hatch_pattern_angle_deg: parse_required_f64(&params[11], "hatch_pattern_angle")?,
        hole_count: usize::try_from(hole_count).ok(),
        in_ids,
    })
}

pub(crate) fn parse_layer_color_style(params: &[Value]) -> Result<CommonStyle, String> {
    Ok(CommonStyle {
        layer_code: Some(parse_required_i64(&params[0], "layer")?),
        color_code: Some(parse_required_i64(&params[1], "color")?),
        ..Default::default()
    })
}

pub(crate) fn parse_line_style(params: &[Value]) -> Result<CommonStyle, String> {
    Ok(CommonStyle {
        layer_code: Some(parse_required_i64(&params[0], "layer")?),
        color_code: Some(parse_required_i64(&params[1], "color")?),
        line_type_code: Some(parse_required_i64(&params[2], "line_type")?),
        line_width_code: Some(parse_required_i64(&params[3], "line_width")?),
        ..Default::default()
    })
}

pub(crate) fn parse_required_string(value: &Value, name: &str) -> Result<String, String> {
    value
        .as_string()
        .ok_or_else(|| format!("{name} must be a string"))
}

pub(crate) fn parse_required_i64(value: &Value, name: &str) -> Result<i64, String> {
    value
        .as_i64()
        .ok_or_else(|| format!("{name} must be an integer-compatible value"))
}

pub(crate) fn parse_optional_i64(value: &Value) -> Option<i64> {
    value.as_i64()
}

pub(crate) fn parse_required_f64(value: &Value, name: &str) -> Result<f64, String> {
    value
        .as_f64()
        .ok_or_else(|| format!("{name} must be a numeric value"))
}

pub(crate) fn parse_required_f64_list(value: &Value, name: &str) -> Result<Vec<f64>, String> {
    value
        .as_f64_list()
        .ok_or_else(|| format!("{name} must be a numeric list"))
}

pub(crate) fn parse_required_i64_list(value: &Value, name: &str) -> Result<Vec<i64>, String> {
    value
        .as_i64_list()
        .ok_or_else(|| format!("{name} must be an integer list"))
}

pub(crate) fn require_parameter_count(
    params: &[Value],
    expected: usize,
    feature_name: &str,
) -> Result<(), String> {
    if params.len() == expected {
        Ok(())
    } else {
        Err(format!(
            "{feature_name} requires exactly {expected} parameters, got {}",
            params.len()
        ))
    }
}

pub(crate) fn validate_declared_count(
    declared: i64,
    actual: usize,
    field_name: &str,
) -> Result<(), String> {
    let declared = usize::try_from(declared)
        .map_err(|_| format!("{field_name} count must be a non-negative integer"))?;
    if declared == actual {
        Ok(())
    } else {
        Err(format!(
            "{field_name} count mismatch: declared {declared}, got {actual}"
        ))
    }
}

pub(crate) fn required_sfc_version(keyword: &str) -> Option<SfcVersionTag> {
    match keyword.to_ascii_lowercase().as_str() {
        "drawing_attribute_feature" => Some(SfcVersionTag::V3),
        "clothoid_feature" | "curve_dim_feature" => Some(SfcVersionTag::V31),
        "drawing_sheet_feature"
        | "layer_feature"
        | "pre_defined_font_feature"
        | "user_defined_font_feature"
        | "pre_defined_colour_feature"
        | "user_defined_colour_feature"
        | "width_feature"
        | "text_font_feature"
        | "point_marker_feature"
        | "line_feature"
        | "polyline_feature"
        | "circle_feature"
        | "arc_feature"
        | "ellipse_feature"
        | "ellipse_arc_feature"
        | "text_string_feature"
        | "spline_feature"
        | "sfig_org_feature"
        | "sfig_locate_feature"
        | "symbol_externally_defined_feature"
        | "externally_defined_symbol_feature"
        | "linear_dim_feature"
        | "angular_dim_feature"
        | "radius_dim_feature"
        | "diameter_dim_feature"
        | "label_feature"
        | "balloon_feature"
        | "externally_defined_hatch_feature"
        | "fill_area_style_colour_feature"
        | "fill_area_style_hatching_feature"
        | "fill_area_style_tiles_hatching_feature"
        | "fill_area_style_tiles_feature"
        | "composite_curve_feature"
        | "composite_curve_org_feature" => Some(SfcVersionTag::V2),
        _ => None,
    }
}

const SFC_UPPER_BOUND: f64 = 1.0e15;

pub(crate) fn validate_typed_feature_values(feature: &TypedFeature) -> Result<(), String> {
    match feature {
        TypedFeature::DrawingAttribute(value) => {
            for (name, text) in [
                ("project name", &value.project_name),
                ("construction name", &value.construction_name),
                ("contract type", &value.contract_type),
                ("drawing name", &value.drawing_name),
                ("drawing number", &value.drawing_number),
                ("drawing type", &value.drawing_type),
                ("drawing scale", &value.drawing_scale),
                ("contractor name", &value.contractor_name),
                ("owner name", &value.owner_name),
            ] {
                validate_sfc_semantic_string(text, name, false)?;
            }
            ensure(
                (1..=12).contains(&value.drawing_month),
                "drawing month must be in 1..=12",
            )?;
            ensure(
                (1..=31).contains(&value.drawing_day),
                "drawing day must be in 1..=31",
            )?;
        }
        TypedFeature::DrawingSheet(value) => {
            validate_sfc_semantic_string(&value.name, "sheet name", false)?;
            ensure(
                matches!(value.sheet_type, 0..=4 | 9),
                "sheet type must be 0..4 or 9 (FREE)",
            )?;
            ensure(
                matches!(value.orientation, 0 | 1),
                "sheet orientation must be 0 or 1",
            )?;
            ensure(
                value.free_x_mm > 0 && value.free_x_mm <= i64::from(i32::MAX),
                "sheet X length must fit a positive 32-bit integer",
            )?;
            ensure(
                value.free_y_mm > 0 && value.free_y_mm <= i64::from(i32::MAX),
                "sheet Y length must fit a positive 32-bit integer",
            )?;
        }
        TypedFeature::Layer(value) => {
            validate_sfc_semantic_string(&value.name, "layer name", false)?;
            validate_flag(value.visibility_flag, "layer visibility flag")?;
        }
        TypedFeature::PreDefinedFont(value) => {
            validate_sfc_semantic_string(&value.name, "predefined line type name", false)?;
        }
        TypedFeature::PreDefinedColour(value) => {
            validate_sfc_semantic_string(&value.name, "predefined color name", false)?;
        }
        TypedFeature::UserDefinedFont(value) => {
            validate_sfc_semantic_string(&value.name, "user line type name", false)?;
            for (index, pitch) in value.pitch.iter().enumerate() {
                validate_nonnegative(*pitch, &format!("line type pitch {}", index + 1))?;
            }
        }
        TypedFeature::UserDefinedColour(value) => {
            for (name, component) in [
                ("red", value.red),
                ("green", value.green),
                ("blue", value.blue),
            ] {
                ensure(
                    (0..=255).contains(&component),
                    format!("{name} must be in 0..=255"),
                )?;
            }
        }
        TypedFeature::Width(value) => validate_positive(value.width, "line width")?,
        TypedFeature::TextFont(value) => {
            validate_sfc_semantic_string(&value.name, "text font name", false)?;
        }
        TypedFeature::PointMarker(value) => {
            validate_style(&value.style)?;
            validate_point(&value.position, "marker position")?;
            ensure(
                (1..=7).contains(&value.marker_code),
                "marker code must be in 1..=7",
            )?;
            validate_angle(value.rotation_angle_deg, "marker rotation angle")?;
            validate_positive(value.scale, "marker scale")?;
        }
        TypedFeature::Line(value) => {
            validate_style(&value.style)?;
            validate_point(&value.start, "line start")?;
            validate_point(&value.end, "line end")?;
        }
        TypedFeature::Polyline(value) => {
            validate_style(&value.style)?;
            ensure(
                value.points.len() >= 2,
                "polyline must contain at least two vertices",
            )?;
            validate_points(&value.points, "polyline vertex")?;
        }
        TypedFeature::Circle(value) => {
            validate_style(&value.style)?;
            validate_point(&value.center, "circle center")?;
            validate_positive(value.radius, "circle radius")?;
        }
        TypedFeature::Arc(value) => {
            validate_style(&value.style)?;
            validate_point(&value.center, "arc center")?;
            validate_positive(value.radius, "arc radius")?;
            validate_flag(value.direction_flag, "arc direction flag")?;
            validate_angle(value.start_angle_deg, "arc start angle")?;
            validate_angle(value.end_angle_deg, "arc end angle")?;
        }
        TypedFeature::Ellipse(value) => {
            validate_style(&value.style)?;
            validate_point(&value.center, "ellipse center")?;
            validate_positive(value.radius_x, "ellipse X radius")?;
            validate_positive(value.radius_y, "ellipse Y radius")?;
            validate_angle(value.rotation_angle_deg, "ellipse rotation angle")?;
        }
        TypedFeature::EllipseArc(value) => {
            validate_style(&value.style)?;
            validate_point(&value.center, "ellipse arc center")?;
            validate_positive(value.radius_x, "ellipse arc X radius")?;
            validate_positive(value.radius_y, "ellipse arc Y radius")?;
            validate_flag(value.direction_flag, "ellipse arc direction flag")?;
            validate_angle(value.rotation_angle_deg, "ellipse arc rotation angle")?;
            validate_angle(value.start_angle_deg, "ellipse arc start angle")?;
            validate_angle(value.end_angle_deg, "ellipse arc end angle")?;
        }
        TypedFeature::Text(value) => {
            validate_style(&value.style)?;
            validate_sfc_semantic_string(&value.text, "text string", false)?;
            validate_point(&value.anchor, "text anchor")?;
            validate_text_metrics(
                value.height,
                value.width,
                value.spacing,
                value.angle_deg,
                value.slant_deg,
                value.base_point,
                value.direction,
            )?;
        }
        TypedFeature::Spline(value) => {
            validate_style(&value.style)?;
            validate_flag(value.open_close, "spline open/close flag")?;
            ensure(
                value.points.len() >= 2,
                "spline must contain at least two control points",
            )?;
            validate_points(&value.points, "spline control point")?;
        }
        TypedFeature::Clothoid(value) => {
            validate_style(&value.style)?;
            validate_point(&value.base, "clothoid base")?;
            validate_positive(value.parameter, "clothoid parameter")?;
            validate_flag(value.direction_flag, "clothoid direction flag")?;
            validate_angle(value.angle_deg, "clothoid rotation angle")?;
            validate_nonnegative(value.start_length, "clothoid start length")?;
            validate_nonnegative(value.end_length, "clothoid end length")?;
        }
        TypedFeature::SfigOrg(value) => {
            validate_sfc_semantic_string(&value.name, "sfig name", false)?;
            ensure(
                (1..=4).contains(&value.kind_flag),
                "sfig kind flag must be in 1..=4",
            )?;
        }
        TypedFeature::SfigLocate(value) => {
            validate_style(&value.style)?;
            validate_sfc_semantic_string(&value.name, "sfig name", false)?;
            validate_point(&value.position, "sfig position")?;
            validate_angle(value.angle_deg, "sfig rotation angle")?;
            validate_positive(value.ratio_x, "sfig X scale")?;
            validate_positive(value.ratio_y, "sfig Y scale")?;
        }
        TypedFeature::ExternallyDefinedSymbol(value) => {
            validate_style(&value.style)?;
            validate_sfc_semantic_string(&value.name, "symbol name", false)?;
            validate_flag(value.color_flag, "symbol color flag")?;
            validate_point(&value.position, "symbol position")?;
            validate_angle(value.rotation_angle_deg, "symbol rotation angle")?;
            validate_positive(value.scale, "symbol scale")?;
        }
        TypedFeature::LinearDim(value) => {
            validate_style(&value.style)?;
            validate_point(&value.start, "linear dimension start")?;
            validate_point(&value.end, "linear dimension end")?;
            validate_extension_line(&value.extension_line1, "extension line 1")?;
            validate_extension_line(&value.extension_line2, "extension line 2")?;
            validate_dimension_arrow(&value.arrow1, "dimension arrow 1")?;
            validate_dimension_arrow(&value.arrow2, "dimension arrow 2")?;
            validate_feature_text(&value.text)?;
        }
        TypedFeature::CurveDim(value) => {
            validate_curve_dimension(
                &value.style,
                &value.center,
                value.radius,
                value.start_angle_deg,
                value.end_angle_deg,
                &value.extension_line1,
                &value.extension_line2,
                &value.arrow1,
                &value.arrow2,
                &value.text,
            )?;
        }
        TypedFeature::AngularDim(value) => {
            validate_curve_dimension(
                &value.style,
                &value.center,
                value.radius,
                value.start_angle_deg,
                value.end_angle_deg,
                &value.extension_line1,
                &value.extension_line2,
                &value.arrow1,
                &value.arrow2,
                &value.text,
            )?;
        }
        TypedFeature::RadiusDim(value) => {
            validate_style(&value.style)?;
            validate_point(&value.start, "radius dimension start")?;
            validate_point(&value.end, "radius dimension end")?;
            validate_dimension_arrow(&value.arrow, "radius dimension arrow")?;
            validate_feature_text(&value.text)?;
        }
        TypedFeature::DiameterDim(value) => {
            validate_style(&value.style)?;
            validate_point(&value.start, "diameter dimension start")?;
            validate_point(&value.end, "diameter dimension end")?;
            validate_dimension_arrow(&value.arrow1, "diameter dimension arrow 1")?;
            validate_dimension_arrow(&value.arrow2, "diameter dimension arrow 2")?;
            validate_feature_text(&value.text)?;
        }
        TypedFeature::Label(value) => {
            validate_style(&value.style)?;
            ensure(
                value.vertices.len() >= 2,
                "label must contain at least two vertices",
            )?;
            validate_points(&value.vertices, "label vertex")?;
            validate_leader_arrow(&value.arrow)?;
            validate_feature_text(&value.text)?;
        }
        TypedFeature::Balloon(value) => {
            validate_style(&value.style)?;
            ensure(
                value.vertices.len() >= 2,
                "balloon must contain at least two vertices",
            )?;
            validate_points(&value.vertices, "balloon vertex")?;
            validate_point(&value.center, "balloon center")?;
            validate_positive(value.radius, "balloon radius")?;
            validate_leader_arrow(&value.arrow)?;
            validate_feature_text(&value.text)?;
        }
        TypedFeature::ExternallyDefinedHatch(value) => {
            validate_style(&value.style)?;
            validate_sfc_semantic_string(&value.name, "hatch name", false)?;
            validate_composite_curve_codes(value.out_id, &value.in_ids)?;
        }
        TypedFeature::FillAreaStyleColour(value) => {
            validate_style(&value.style)?;
            validate_composite_curve_codes(value.out_id, &value.in_ids)?;
        }
        TypedFeature::FillAreaStyleHatching(value) => {
            validate_style(&value.style)?;
            ensure(
                (1..=4).contains(&value.hatch_number),
                "hatch pattern count must be in 1..=4",
            )?;
            for pattern in &value.patterns {
                validate_style(&CommonStyle {
                    color_code: Some(pattern.color_code),
                    line_type_code: Some(pattern.line_type_code),
                    line_width_code: Some(pattern.line_width_code),
                    ..Default::default()
                })?;
                validate_point(&pattern.start, "hatch line start")?;
                validate_positive(pattern.spacing, "hatch line spacing")?;
                validate_angle(pattern.angle_deg, "hatch line angle")?;
            }
            validate_composite_curve_codes(value.out_id, &value.in_ids)?;
        }
        TypedFeature::FillAreaStyleTiles(value) => {
            validate_style(&value.style)?;
            validate_sfc_semantic_string(&value.name, "tile symbol name", false)?;
            validate_style(&CommonStyle {
                color_code: Some(value.hatch_color),
                ..Default::default()
            })?;
            validate_point(&value.hatch_pattern_position, "tile pattern position")?;
            validate_positive(value.hatch_pattern_vector1, "tile pattern vector 1")?;
            validate_positive(value.hatch_pattern_vector2, "tile pattern vector 2")?;
            validate_angle(
                value.hatch_pattern_vector1_angle_deg,
                "tile pattern vector 1 angle",
            )?;
            validate_angle(
                value.hatch_pattern_vector2_angle_deg,
                "tile pattern vector 2 angle",
            )?;
            validate_positive(value.hatch_pattern_scale_x, "tile pattern X scale")?;
            validate_positive(value.hatch_pattern_scale_y, "tile pattern Y scale")?;
            validate_angle(value.hatch_pattern_angle_deg, "tile pattern angle")?;
            validate_composite_curve_codes(value.out_id, &value.in_ids)?;
        }
        TypedFeature::CompositeCurve(value) => {
            validate_style(&value.style)?;
            validate_flag(value.visibility_flag, "composite curve visibility flag")?;
        }
    }
    Ok(())
}

pub(crate) fn ensure(condition: bool, message: impl Into<String>) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

pub(crate) fn validate_sfc_semantic_string(
    value: &str,
    name: &str,
    allow_empty: bool,
) -> Result<(), String> {
    if !allow_empty {
        ensure(!value.is_empty(), format!("{name} must not be empty"))?;
    }
    let (encoded, _, had_errors) = SHIFT_JIS.encode(value);
    ensure(
        !had_errors,
        format!("{name} contains characters not representable in Shift-JIS"),
    )?;
    ensure(
        encoded.len() <= 256,
        format!(
            "{name} exceeds the 256-byte limit ({} Shift-JIS bytes)",
            encoded.len()
        ),
    )
}

pub(crate) fn validate_style(style: &CommonStyle) -> Result<(), String> {
    for (name, code, max) in [
        ("layer", style.layer_code, 256),
        ("color", style.color_code, 256),
        ("line type", style.line_type_code, 32),
        ("line width", style.line_width_code, 16),
        ("text font", style.font_code, 1024),
    ] {
        if let Some(code) = code {
            ensure(
                (0..=max).contains(&code),
                format!("{name} code must be in 0..={max}"),
            )?;
        }
    }
    Ok(())
}

pub(crate) fn validate_point(point: &Point2, name: &str) -> Result<(), String> {
    ensure(
        point.x.is_finite() && point.y.is_finite(),
        format!("{name} must contain finite coordinates"),
    )
}

pub(crate) fn validate_points(points: &[Point2], name: &str) -> Result<(), String> {
    for (index, point) in points.iter().enumerate() {
        validate_point(point, &format!("{name} {}", index + 1))?;
    }
    Ok(())
}

pub(crate) fn validate_positive(value: f64, name: &str) -> Result<(), String> {
    ensure(
        value.is_finite() && value > 0.0 && value < SFC_UPPER_BOUND,
        format!("{name} must be greater than 0 and less than 1.0e15"),
    )
}

pub(crate) fn validate_nonnegative(value: f64, name: &str) -> Result<(), String> {
    ensure(
        value.is_finite() && (0.0..SFC_UPPER_BOUND).contains(&value),
        format!("{name} must be at least 0 and less than 1.0e15"),
    )
}

pub(crate) fn validate_angle(value: f64, name: &str) -> Result<(), String> {
    ensure(
        value.is_finite() && (0.0..360.0).contains(&value),
        format!("{name} must be in 0<=angle<360"),
    )
}

pub(crate) fn validate_flag(value: i64, name: &str) -> Result<(), String> {
    ensure(matches!(value, 0 | 1), format!("{name} must be 0 or 1"))
}

pub(crate) fn validate_text_metrics(
    height: f64,
    width: f64,
    spacing: f64,
    angle: f64,
    slant: f64,
    base_point: i64,
    direction: i64,
) -> Result<(), String> {
    validate_positive(height, "text height")?;
    validate_positive(width, "text width")?;
    validate_nonnegative(spacing, "text spacing")?;
    validate_angle(angle, "text rotation angle")?;
    ensure(
        slant.is_finite() && (-85.0..=85.0).contains(&slant),
        "text slant must be in -85..=85",
    )?;
    ensure(
        (1..=9).contains(&base_point),
        "text base point must be in 1..=9",
    )?;
    ensure(matches!(direction, 1 | 2), "text direction must be 1 or 2")
}

pub(crate) fn validate_feature_text(value: &FeatureText) -> Result<(), String> {
    validate_flag(value.present_flag, "text present flag")?;
    if value.present_flag == 0 {
        return Ok(());
    }
    ensure(
        (1..=1024).contains(&value.font_code),
        "text font code must be in 1..=1024",
    )?;
    validate_sfc_semantic_string(&value.text, "feature text", false)?;
    validate_point(&value.anchor, "text anchor")?;
    validate_text_metrics(
        value.height,
        value.width,
        value.spacing,
        value.angle_deg,
        value.slant_deg,
        value.base_point,
        value.direction,
    )
}

pub(crate) fn validate_extension_line(value: &ExtensionLine, name: &str) -> Result<(), String> {
    validate_flag(value.present_flag, &format!("{name} present flag"))?;
    if value.present_flag == 1 {
        validate_point(&value.base, &format!("{name} base"))?;
        validate_point(&value.start, &format!("{name} start"))?;
        validate_point(&value.end, &format!("{name} end"))?;
    }
    Ok(())
}

pub(crate) fn validate_dimension_arrow(value: &DimensionArrow, name: &str) -> Result<(), String> {
    ensure(
        (0..=2).contains(&value.direction_flag),
        format!("{name} direction must be in 0..=2"),
    )?;
    if value.direction_flag != 0 {
        ensure(
            (1..=11).contains(&value.code),
            format!("{name} code must be in 1..=11"),
        )?;
        validate_point(&value.position, &format!("{name} position"))?;
        validate_positive(value.scale, &format!("{name} scale"))?;
    }
    Ok(())
}

pub(crate) fn validate_leader_arrow(value: &LeaderArrow) -> Result<(), String> {
    ensure(
        (1..=11).contains(&value.code),
        "leader arrow code must be in 1..=11",
    )?;
    validate_positive(value.scale, "leader arrow scale")
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn validate_curve_dimension(
    style: &CommonStyle,
    center: &Point2,
    radius: f64,
    start_angle: f64,
    end_angle: f64,
    extension_line1: &ExtensionLine,
    extension_line2: &ExtensionLine,
    arrow1: &DimensionArrow,
    arrow2: &DimensionArrow,
    text: &FeatureText,
) -> Result<(), String> {
    validate_style(style)?;
    validate_point(center, "dimension center")?;
    validate_positive(radius, "dimension radius")?;
    validate_angle(start_angle, "dimension start angle")?;
    validate_angle(end_angle, "dimension end angle")?;
    validate_extension_line(extension_line1, "extension line 1")?;
    validate_extension_line(extension_line2, "extension line 2")?;
    validate_dimension_arrow(arrow1, "dimension arrow 1")?;
    validate_dimension_arrow(arrow2, "dimension arrow 2")?;
    validate_feature_text(text)
}

pub(crate) fn validate_composite_curve_codes(out_id: i64, in_ids: &[i64]) -> Result<(), String> {
    ensure(
        out_id > 0,
        "hatch outer composite curve code must be positive",
    )?;
    ensure(
        in_ids.iter().all(|code| *code > 0),
        "hatch inner composite curve codes must be positive",
    )
}
