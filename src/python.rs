//! Thin PyO3 boundary and Python dictionary conversion.

use std::fs;
use std::path::Path;

use pyo3::exceptions::{PyIOError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList};

use crate::model::*;
use crate::parser::parse_from_bytes;

fn resolve_python_input(input: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    if let Ok(raw) = input.extract::<&[u8]>() {
        return Ok(raw.to_vec());
    }

    if let Ok(text) = input.extract::<String>() {
        let path = Path::new(&text);
        if path.exists() {
            return fs::read(path).map_err(|error| {
                PyIOError::new_err(format!("Failed to read file {}: {error}", path.display()))
            });
        }
        return Ok(text.into_bytes());
    }

    Err(PyTypeError::new_err(
        "input must be bytes or str (path or text content)",
    ))
}

fn parse_error_to_py(error: ParseError) -> PyErr {
    PyValueError::new_err(error.to_string())
}

fn value_to_python<'py>(py: Python<'py>, value: &Value) -> PyResult<PyObject> {
    match value {
        Value::Unset => {
            let d = PyDict::new_bound(py);
            d.set_item("kind", "unset")?;
            Ok(d.into_py(py))
        }
        Value::Omitted => {
            let d = PyDict::new_bound(py);
            d.set_item("kind", "omitted")?;
            Ok(d.into_py(py))
        }
        Value::Integer(v) => Ok(v.into_py(py)),
        Value::Real(v) => Ok(v.into_py(py)),
        Value::String(v) => Ok(v.into_py(py)),
        Value::Enum(v) => {
            let d = PyDict::new_bound(py);
            d.set_item("kind", "enum")?;
            d.set_item("value", v)?;
            Ok(d.into_py(py))
        }
        Value::Reference(v) => {
            let d = PyDict::new_bound(py);
            d.set_item("kind", "reference")?;
            d.set_item("value", *v)?;
            Ok(d.into_py(py))
        }
        Value::Binary(v) => {
            let d = PyDict::new_bound(py);
            d.set_item("kind", "binary")?;
            d.set_item("value", v)?;
            Ok(d.into_py(py))
        }
        Value::List(values) => {
            let list = PyList::empty_bound(py);
            for item in values {
                list.append(value_to_python(py, item)?)?;
            }
            Ok(list.into_py(py))
        }
        Value::Typed {
            keyword,
            parameters,
        } => {
            let d = PyDict::new_bound(py);
            d.set_item("kind", "typed")?;
            d.set_item("keyword", keyword)?;
            let params = PyList::empty_bound(py);
            for item in parameters {
                params.append(value_to_python(py, item)?)?;
            }
            d.set_item("parameters", params)?;
            Ok(d.into_py(py))
        }
    }
}

fn record_to_python<'py>(py: Python<'py>, record: &Record) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new_bound(py);
    d.set_item("keyword", &record.keyword)?;
    let params = PyList::empty_bound(py);
    for value in &record.parameters {
        params.append(value_to_python(py, value)?)?;
    }
    d.set_item("parameters", params)?;
    Ok(d)
}

fn style_to_python<'py>(py: Python<'py>, style: &CommonStyle) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new_bound(py);
    d.set_item("layer_code", style.layer_code)?;
    d.set_item("color_code", style.color_code)?;
    d.set_item("line_type_code", style.line_type_code)?;
    d.set_item("line_width_code", style.line_width_code)?;
    d.set_item("font_code", style.font_code)?;
    Ok(d)
}

fn point_to_python<'py>(py: Python<'py>, point: &Point2) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new_bound(py);
    d.set_item("x", point.x)?;
    d.set_item("y", point.y)?;
    Ok(d)
}

fn extension_line_to_python<'py>(
    py: Python<'py>,
    line: &ExtensionLine,
) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new_bound(py);
    d.set_item("present_flag", line.present_flag)?;
    d.set_item("base", point_to_python(py, &line.base)?)?;
    d.set_item("start", point_to_python(py, &line.start)?)?;
    d.set_item("end", point_to_python(py, &line.end)?)?;
    Ok(d)
}

fn dimension_arrow_to_python<'py>(
    py: Python<'py>,
    arrow: &DimensionArrow,
) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new_bound(py);
    d.set_item("code", arrow.code)?;
    d.set_item("direction_flag", arrow.direction_flag)?;
    d.set_item("position", point_to_python(py, &arrow.position)?)?;
    d.set_item("scale", arrow.scale)?;
    Ok(d)
}

fn leader_arrow_to_python<'py>(
    py: Python<'py>,
    arrow: &LeaderArrow,
) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new_bound(py);
    d.set_item("code", arrow.code)?;
    d.set_item("scale", arrow.scale)?;
    Ok(d)
}

fn feature_text_to_python<'py>(
    py: Python<'py>,
    text: &FeatureText,
) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new_bound(py);
    d.set_item("present_flag", text.present_flag)?;
    d.set_item("font_code", text.font_code)?;
    d.set_item("text", &text.text)?;
    d.set_item("anchor", point_to_python(py, &text.anchor)?)?;
    d.set_item("height", text.height)?;
    d.set_item("width", text.width)?;
    d.set_item("spacing", text.spacing)?;
    d.set_item("angle_deg", text.angle_deg)?;
    d.set_item("slant_deg", text.slant_deg)?;
    d.set_item("base_point", text.base_point)?;
    d.set_item("direction", text.direction)?;
    Ok(d)
}

fn hatch_line_pattern_to_python<'py>(
    py: Python<'py>,
    pattern: &HatchLinePattern,
) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new_bound(py);
    d.set_item("color_code", pattern.color_code)?;
    d.set_item("line_type_code", pattern.line_type_code)?;
    d.set_item("line_width_code", pattern.line_width_code)?;
    d.set_item("start", point_to_python(py, &pattern.start)?)?;
    d.set_item("spacing", pattern.spacing)?;
    d.set_item("angle_deg", pattern.angle_deg)?;
    Ok(d)
}

fn typed_feature_instance_to_python<'py>(
    py: Python<'py>,
    instance: &TypedFeatureInstance,
) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new_bound(py);
    d.set_item("id", instance.id)?;
    d.set_item("keyword", &instance.keyword)?;

    match &instance.feature {
        TypedFeature::DrawingAttribute(feature) => {
            d.set_item("kind", "drawing_attribute")?;
            d.set_item("project_name", &feature.project_name)?;
            d.set_item("construction_name", &feature.construction_name)?;
            d.set_item("contract_type", &feature.contract_type)?;
            d.set_item("drawing_name", &feature.drawing_name)?;
            d.set_item("drawing_number", &feature.drawing_number)?;
            d.set_item("drawing_type", &feature.drawing_type)?;
            d.set_item("drawing_scale", &feature.drawing_scale)?;
            d.set_item("drawing_year", feature.drawing_year)?;
            d.set_item("drawing_month", feature.drawing_month)?;
            d.set_item("drawing_day", feature.drawing_day)?;
            d.set_item("contractor_name", &feature.contractor_name)?;
            d.set_item("owner_name", &feature.owner_name)?;
        }
        TypedFeature::DrawingSheet(feature) => {
            d.set_item("kind", "drawing_sheet")?;
            d.set_item("name", &feature.name)?;
            d.set_item("sheet_type", feature.sheet_type)?;
            d.set_item("orientation", feature.orientation)?;
            d.set_item("free_x_mm", feature.free_x_mm)?;
            d.set_item("free_y_mm", feature.free_y_mm)?;
        }
        TypedFeature::Layer(feature) => {
            d.set_item("kind", "layer")?;
            d.set_item("name", &feature.name)?;
            d.set_item("visibility_flag", feature.visibility_flag)?;
        }
        TypedFeature::PreDefinedFont(feature) => {
            d.set_item("kind", "pre_defined_font")?;
            d.set_item("name", &feature.name)?;
        }
        TypedFeature::UserDefinedFont(feature) => {
            d.set_item("kind", "user_defined_font")?;
            d.set_item("name", &feature.name)?;
            d.set_item("segment_count", feature.segment_count)?;
            d.set_item("pitch", feature.pitch.clone())?;
        }
        TypedFeature::PreDefinedColour(feature) => {
            d.set_item("kind", "pre_defined_colour")?;
            d.set_item("name", &feature.name)?;
        }
        TypedFeature::UserDefinedColour(feature) => {
            d.set_item("kind", "user_defined_colour")?;
            d.set_item("red", feature.red)?;
            d.set_item("green", feature.green)?;
            d.set_item("blue", feature.blue)?;
        }
        TypedFeature::Width(feature) => {
            d.set_item("kind", "width")?;
            d.set_item("width", feature.width)?;
        }
        TypedFeature::TextFont(feature) => {
            d.set_item("kind", "text_font")?;
            d.set_item("name", &feature.name)?;
        }
        TypedFeature::SfigOrg(feature) => {
            d.set_item("kind", "sfig_org")?;
            d.set_item("name", &feature.name)?;
            d.set_item("kind_flag", feature.kind_flag)?;
        }
        TypedFeature::SfigLocate(feature) => {
            d.set_item("kind", "sfig_locate")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("name", &feature.name)?;
            d.set_item("position", point_to_python(py, &feature.position)?)?;
            d.set_item("angle_deg", feature.angle_deg)?;
            d.set_item("ratio_x", feature.ratio_x)?;
            d.set_item("ratio_y", feature.ratio_y)?;
        }
        TypedFeature::ExternallyDefinedSymbol(feature) => {
            d.set_item("kind", "externally_defined_symbol")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("color_flag", feature.color_flag)?;
            d.set_item("name", &feature.name)?;
            d.set_item("position", point_to_python(py, &feature.position)?)?;
            d.set_item("rotation_angle_deg", feature.rotation_angle_deg)?;
            d.set_item("scale", feature.scale)?;
        }
        TypedFeature::LinearDim(feature) => {
            d.set_item("kind", "linear_dim")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("start", point_to_python(py, &feature.start)?)?;
            d.set_item("end", point_to_python(py, &feature.end)?)?;
            d.set_item(
                "extension_line1",
                extension_line_to_python(py, &feature.extension_line1)?,
            )?;
            d.set_item(
                "extension_line2",
                extension_line_to_python(py, &feature.extension_line2)?,
            )?;
            d.set_item("arrow1", dimension_arrow_to_python(py, &feature.arrow1)?)?;
            d.set_item("arrow2", dimension_arrow_to_python(py, &feature.arrow2)?)?;
            d.set_item("text", feature_text_to_python(py, &feature.text)?)?;
            let raw_parameters = PyList::empty_bound(py);
            for value in &feature.raw_parameters {
                raw_parameters.append(value_to_python(py, value)?)?;
            }
            d.set_item("raw_parameters", raw_parameters)?;
        }
        TypedFeature::CurveDim(feature) => {
            d.set_item("kind", "curve_dim")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("center", point_to_python(py, &feature.center)?)?;
            d.set_item("radius", feature.radius)?;
            d.set_item("start_angle_deg", feature.start_angle_deg)?;
            d.set_item("end_angle_deg", feature.end_angle_deg)?;
            d.set_item(
                "extension_line1",
                extension_line_to_python(py, &feature.extension_line1)?,
            )?;
            d.set_item(
                "extension_line2",
                extension_line_to_python(py, &feature.extension_line2)?,
            )?;
            d.set_item("arrow1", dimension_arrow_to_python(py, &feature.arrow1)?)?;
            d.set_item("arrow2", dimension_arrow_to_python(py, &feature.arrow2)?)?;
            d.set_item("text", feature_text_to_python(py, &feature.text)?)?;
            let raw_parameters = PyList::empty_bound(py);
            for value in &feature.raw_parameters {
                raw_parameters.append(value_to_python(py, value)?)?;
            }
            d.set_item("raw_parameters", raw_parameters)?;
        }
        TypedFeature::AngularDim(feature) => {
            d.set_item("kind", "angular_dim")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("center", point_to_python(py, &feature.center)?)?;
            d.set_item("radius", feature.radius)?;
            d.set_item("start_angle_deg", feature.start_angle_deg)?;
            d.set_item("end_angle_deg", feature.end_angle_deg)?;
            d.set_item(
                "extension_line1",
                extension_line_to_python(py, &feature.extension_line1)?,
            )?;
            d.set_item(
                "extension_line2",
                extension_line_to_python(py, &feature.extension_line2)?,
            )?;
            d.set_item("arrow1", dimension_arrow_to_python(py, &feature.arrow1)?)?;
            d.set_item("arrow2", dimension_arrow_to_python(py, &feature.arrow2)?)?;
            d.set_item("text", feature_text_to_python(py, &feature.text)?)?;
            let raw_parameters = PyList::empty_bound(py);
            for value in &feature.raw_parameters {
                raw_parameters.append(value_to_python(py, value)?)?;
            }
            d.set_item("raw_parameters", raw_parameters)?;
        }
        TypedFeature::RadiusDim(feature) => {
            d.set_item("kind", "radius_dim")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("start", point_to_python(py, &feature.start)?)?;
            d.set_item("end", point_to_python(py, &feature.end)?)?;
            d.set_item("arrow", dimension_arrow_to_python(py, &feature.arrow)?)?;
            d.set_item("text", feature_text_to_python(py, &feature.text)?)?;
            let raw_parameters = PyList::empty_bound(py);
            for value in &feature.raw_parameters {
                raw_parameters.append(value_to_python(py, value)?)?;
            }
            d.set_item("raw_parameters", raw_parameters)?;
        }
        TypedFeature::DiameterDim(feature) => {
            d.set_item("kind", "diameter_dim")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("start", point_to_python(py, &feature.start)?)?;
            d.set_item("end", point_to_python(py, &feature.end)?)?;
            d.set_item("arrow1", dimension_arrow_to_python(py, &feature.arrow1)?)?;
            d.set_item("arrow2", dimension_arrow_to_python(py, &feature.arrow2)?)?;
            d.set_item("text", feature_text_to_python(py, &feature.text)?)?;
            let raw_parameters = PyList::empty_bound(py);
            for value in &feature.raw_parameters {
                raw_parameters.append(value_to_python(py, value)?)?;
            }
            d.set_item("raw_parameters", raw_parameters)?;
        }
        TypedFeature::Label(feature) => {
            d.set_item("kind", "label")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("declared_vertex_count", feature.declared_vertex_count)?;
            let vertices = PyList::empty_bound(py);
            for point in &feature.vertices {
                vertices.append(point_to_python(py, point)?)?;
            }
            d.set_item("vertices", vertices)?;
            d.set_item("arrow", leader_arrow_to_python(py, &feature.arrow)?)?;
            d.set_item("arrow_code", feature.arrow.code)?;
            d.set_item("arrow_scale", feature.arrow.scale)?;
            d.set_item("text", feature_text_to_python(py, &feature.text)?)?;
        }
        TypedFeature::Balloon(feature) => {
            d.set_item("kind", "balloon")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("declared_vertex_count", feature.declared_vertex_count)?;
            let vertices = PyList::empty_bound(py);
            for point in &feature.vertices {
                vertices.append(point_to_python(py, point)?)?;
            }
            d.set_item("vertices", vertices)?;
            d.set_item("center", point_to_python(py, &feature.center)?)?;
            d.set_item("radius", feature.radius)?;
            d.set_item("arrow", leader_arrow_to_python(py, &feature.arrow)?)?;
            d.set_item("arrow_code", feature.arrow.code)?;
            d.set_item("arrow_scale", feature.arrow.scale)?;
            d.set_item("text", feature_text_to_python(py, &feature.text)?)?;
        }
        TypedFeature::ExternallyDefinedHatch(feature) => {
            d.set_item("kind", "externally_defined_hatch")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("name", &feature.name)?;
            d.set_item("out_id", feature.out_id)?;
            d.set_item("hole_count", feature.hole_count)?;
            d.set_item("in_ids", feature.in_ids.clone())?;
        }
        TypedFeature::FillAreaStyleColour(feature) => {
            d.set_item("kind", "fill_area_style_colour")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("out_id", feature.out_id)?;
            d.set_item("hole_count", feature.hole_count)?;
            d.set_item("in_ids", feature.in_ids.clone())?;
        }
        TypedFeature::FillAreaStyleHatching(feature) => {
            d.set_item("kind", "fill_area_style_hatching")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("hatch_number", feature.hatch_number)?;
            let hatch_patterns = PyList::empty_bound(py);
            for value in &feature.hatch_patterns {
                hatch_patterns.append(value_to_python(py, value)?)?;
            }
            d.set_item("hatch_patterns", hatch_patterns)?;
            let patterns = PyList::empty_bound(py);
            for pattern in &feature.patterns {
                patterns.append(hatch_line_pattern_to_python(py, pattern)?)?;
            }
            d.set_item("patterns", patterns)?;
            d.set_item("out_id", feature.out_id)?;
            d.set_item("hole_count", feature.hole_count)?;
            d.set_item("in_ids", feature.in_ids.clone())?;
        }
        TypedFeature::FillAreaStyleTiles(feature) => {
            d.set_item("kind", "fill_area_style_tiles")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("name", &feature.name)?;
            d.set_item("hatch_color", feature.hatch_color)?;
            d.set_item(
                "hatch_pattern_position",
                point_to_python(py, &feature.hatch_pattern_position)?,
            )?;
            d.set_item("out_id", feature.out_id)?;
            d.set_item("hatch_pattern_vector1", feature.hatch_pattern_vector1)?;
            d.set_item(
                "hatch_pattern_vector1_angle_deg",
                feature.hatch_pattern_vector1_angle_deg,
            )?;
            d.set_item("hatch_pattern_vector2", feature.hatch_pattern_vector2)?;
            d.set_item(
                "hatch_pattern_vector2_angle_deg",
                feature.hatch_pattern_vector2_angle_deg,
            )?;
            d.set_item("hatch_pattern_scale_x", feature.hatch_pattern_scale_x)?;
            d.set_item("hatch_pattern_scale_y", feature.hatch_pattern_scale_y)?;
            d.set_item("hatch_pattern_angle_deg", feature.hatch_pattern_angle_deg)?;
            d.set_item("hole_count", feature.hole_count)?;
            d.set_item("in_ids", feature.in_ids.clone())?;
        }
        TypedFeature::PointMarker(feature) => {
            d.set_item("kind", "point_marker")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("position", point_to_python(py, &feature.position)?)?;
            d.set_item("marker_code", feature.marker_code)?;
            d.set_item("rotation_angle_deg", feature.rotation_angle_deg)?;
            d.set_item("scale", feature.scale)?;
        }
        TypedFeature::Line(feature) => {
            d.set_item("kind", "line")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("start", point_to_python(py, &feature.start)?)?;
            d.set_item("end", point_to_python(py, &feature.end)?)?;
        }
        TypedFeature::Polyline(feature) => {
            d.set_item("kind", "polyline")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("declared_point_count", feature.declared_point_count)?;
            let points = PyList::empty_bound(py);
            for point in &feature.points {
                points.append(point_to_python(py, point)?)?;
            }
            d.set_item("points", points)?;
        }
        TypedFeature::Circle(feature) => {
            d.set_item("kind", "circle")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("center", point_to_python(py, &feature.center)?)?;
            d.set_item("radius", feature.radius)?;
        }
        TypedFeature::Arc(feature) => {
            d.set_item("kind", "arc")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("center", point_to_python(py, &feature.center)?)?;
            d.set_item("radius", feature.radius)?;
            d.set_item("direction_flag", feature.direction_flag)?;
            d.set_item("start_angle_deg", feature.start_angle_deg)?;
            d.set_item("end_angle_deg", feature.end_angle_deg)?;
        }
        TypedFeature::Ellipse(feature) => {
            d.set_item("kind", "ellipse")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("center", point_to_python(py, &feature.center)?)?;
            d.set_item("radius_x", feature.radius_x)?;
            d.set_item("radius_y", feature.radius_y)?;
            d.set_item("rotation_angle_deg", feature.rotation_angle_deg)?;
        }
        TypedFeature::EllipseArc(feature) => {
            d.set_item("kind", "ellipse_arc")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("center", point_to_python(py, &feature.center)?)?;
            d.set_item("radius_x", feature.radius_x)?;
            d.set_item("radius_y", feature.radius_y)?;
            d.set_item("direction_flag", feature.direction_flag)?;
            d.set_item("rotation_angle_deg", feature.rotation_angle_deg)?;
            d.set_item("start_angle_deg", feature.start_angle_deg)?;
            d.set_item("end_angle_deg", feature.end_angle_deg)?;
        }
        TypedFeature::Text(feature) => {
            d.set_item("kind", "text")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("text", &feature.text)?;
            d.set_item("anchor", point_to_python(py, &feature.anchor)?)?;
            d.set_item("height", feature.height)?;
            d.set_item("width", feature.width)?;
            d.set_item("spacing", feature.spacing)?;
            d.set_item("angle_deg", feature.angle_deg)?;
            d.set_item("slant_deg", feature.slant_deg)?;
            d.set_item("base_point", feature.base_point)?;
            d.set_item("direction", feature.direction)?;
        }
        TypedFeature::Spline(feature) => {
            d.set_item("kind", "spline")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("open_close", feature.open_close)?;
            d.set_item("declared_point_count", feature.declared_point_count)?;
            let points = PyList::empty_bound(py);
            for point in &feature.points {
                points.append(point_to_python(py, point)?)?;
            }
            d.set_item("points", points)?;
        }
        TypedFeature::Clothoid(feature) => {
            d.set_item("kind", "clothoid")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("base", point_to_python(py, &feature.base)?)?;
            d.set_item("parameter", feature.parameter)?;
            d.set_item("direction_flag", feature.direction_flag)?;
            d.set_item("angle_deg", feature.angle_deg)?;
            d.set_item("start_length", feature.start_length)?;
            d.set_item("end_length", feature.end_length)?;
        }
        TypedFeature::CompositeCurve(feature) => {
            d.set_item("kind", "composite_curve")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("visibility_flag", feature.visibility_flag)?;
        }
    }

    Ok(d)
}

fn code_bindings_to_python<'py>(
    py: Python<'py>,
    bindings: &[SfcCodeBinding],
) -> PyResult<Bound<'py, PyList>> {
    let result = PyList::empty_bound(py);
    for binding in bindings {
        let item = PyDict::new_bound(py);
        item.set_item("code", binding.code)?;
        item.set_item("entity_id", binding.entity_id)?;
        result.append(item)?;
    }
    Ok(result)
}

fn attribute_mechanism_to_python<'py>(
    py: Python<'py>,
    mechanism: &SfcAttributeMechanism,
) -> PyResult<Bound<'py, PyDict>> {
    let result = PyDict::new_bound(py);
    match mechanism {
        SfcAttributeMechanism::AttributeFile {
            figure_id,
            attribute_file_name,
        } => {
            result.set_item("mechanism", "ATRF")?;
            result.set_item("figure_id", figure_id)?;
            result.set_item("attribute_file_name", attribute_file_name)?;
        }
        SfcAttributeMechanism::SingleAttribute {
            figure_id,
            figure_name,
            attribute_name,
            attribute_value,
            attribute_type,
            unit,
        } => {
            result.set_item("mechanism", "ATRU")?;
            result.set_item("figure_id", figure_id)?;
            result.set_item("figure_name", figure_name)?;
            result.set_item("attribute_name", attribute_name)?;
            result.set_item("attribute_value", attribute_value)?;
            result.set_item("attribute_type", attribute_type)?;
            result.set_item("unit", unit)?;
        }
        SfcAttributeMechanism::TextAttribute {
            figure_id,
            attribute_name,
            attribute_type,
            unit,
        } => {
            result.set_item("mechanism", "ATRS")?;
            result.set_item("figure_id", figure_id)?;
            result.set_item("attribute_name", attribute_name)?;
            result.set_item("attribute_type", attribute_type)?;
            result.set_item("unit", unit)?;
        }
    }
    Ok(result)
}

fn sfc_model_to_python<'py>(py: Python<'py>, model: &SfcModel) -> PyResult<Bound<'py, PyDict>> {
    let result = PyDict::new_bound(py);
    let code_tables = PyDict::new_bound(py);
    code_tables.set_item(
        "layers",
        code_bindings_to_python(py, &model.code_tables.layers)?,
    )?;
    code_tables.set_item(
        "line_types",
        code_bindings_to_python(py, &model.code_tables.line_types)?,
    )?;
    code_tables.set_item(
        "colors",
        code_bindings_to_python(py, &model.code_tables.colors)?,
    )?;
    code_tables.set_item(
        "line_widths",
        code_bindings_to_python(py, &model.code_tables.line_widths)?,
    )?;
    code_tables.set_item(
        "text_fonts",
        code_bindings_to_python(py, &model.code_tables.text_fonts)?,
    )?;
    result.set_item("code_tables", code_tables)?;

    if let Some(sheet) = &model.sheet {
        let item = PyDict::new_bound(py);
        item.set_item("entity_id", sheet.entity_id)?;
        item.set_item("component_ids", &sheet.component_ids)?;
        result.set_item("sheet", item)?;
    } else {
        result.set_item("sheet", py.None())?;
    }

    let sfig_definitions = PyList::empty_bound(py);
    for definition in &model.sfig_definitions {
        let item = PyDict::new_bound(py);
        item.set_item("entity_id", definition.entity_id)?;
        item.set_item("name", &definition.name)?;
        item.set_item("kind_flag", definition.kind_flag)?;
        item.set_item("component_ids", &definition.component_ids)?;
        sfig_definitions.append(item)?;
    }
    result.set_item("sfig_definitions", sfig_definitions)?;

    let attribute_attachments = PyList::empty_bound(py);
    for attachment in &model.attribute_attachments {
        let item = PyDict::new_bound(py);
        item.set_item("definition_id", attachment.definition_id)?;
        item.set_item("name", &attachment.name)?;
        item.set_item("kind_flag", attachment.kind_flag)?;
        item.set_item("component_ids", &attachment.component_ids)?;
        item.set_item("placement_ids", &attachment.placement_ids)?;
        item.set_item(
            "resolved_attribute_file_name",
            &attachment.resolved_attribute_file_name,
        )?;
        item.set_item(
            "attribute",
            attribute_mechanism_to_python(py, &attachment.mechanism)?,
        )?;
        attribute_attachments.append(item)?;
    }
    result.set_item("attribute_attachments", attribute_attachments)?;

    let composite_definitions = PyList::empty_bound(py);
    for definition in &model.composite_curve_definitions {
        let item = PyDict::new_bound(py);
        item.set_item("code", definition.code)?;
        item.set_item("entity_id", definition.entity_id)?;
        item.set_item("component_ids", &definition.component_ids)?;
        composite_definitions.append(item)?;
    }
    result.set_item("composite_curve_definitions", composite_definitions)?;

    let sfig_references = PyList::empty_bound(py);
    for reference in &model.sfig_references {
        let item = PyDict::new_bound(py);
        item.set_item("placement_id", reference.placement_id)?;
        item.set_item("definition_id", reference.definition_id)?;
        sfig_references.append(item)?;
    }
    result.set_item("sfig_references", sfig_references)?;

    let hatch_references = PyList::empty_bound(py);
    for reference in &model.hatch_references {
        let item = PyDict::new_bound(py);
        item.set_item("hatch_id", reference.hatch_id)?;
        item.set_item("outer_definition_id", reference.outer_definition_id)?;
        item.set_item("inner_definition_ids", &reference.inner_definition_ids)?;
        hatch_references.append(item)?;
    }
    result.set_item("hatch_references", hatch_references)?;

    Ok(result)
}

fn output_to_python<'py>(py: Python<'py>, output: &ParseOutput) -> PyResult<Bound<'py, PyDict>> {
    let root = PyDict::new_bound(py);
    root.set_item("format", output.document.format.as_str())?;

    let header = PyDict::new_bound(py);
    let header_entities = PyList::empty_bound(py);
    for entity in &output.document.header.entities {
        header_entities.append(record_to_python(py, entity)?)?;
    }
    header.set_item("entities", header_entities)?;

    if let Some(record) = output.document.header.find_keyword("FILE_DESCRIPTION") {
        header.set_item("file_description", record_to_python(py, record)?)?;
    }
    if let Some(record) = output.document.header.find_keyword("FILE_NAME") {
        header.set_item("file_name", record_to_python(py, record)?)?;
    }
    if let Some(record) = output.document.header.find_keyword("FILE_SCHEMA") {
        header.set_item("file_schema", record_to_python(py, record)?)?;
    }
    root.set_item("header", header)?;

    let entities = PyList::empty_bound(py);
    for entity in &output.document.entities {
        let d = PyDict::new_bound(py);
        d.set_item("id", entity.id)?;
        d.set_item("sfc_version", entity.sfc_version.map(SfcVersionTag::as_str))?;
        match &entity.body {
            EntityBody::Simple(record) => {
                d.set_item("body_type", "simple")?;
                d.set_item("record", record_to_python(py, record)?)?;
            }
            EntityBody::Complex(records) => {
                d.set_item("body_type", "complex")?;
                let record_list = PyList::empty_bound(py);
                for record in records {
                    record_list.append(record_to_python(py, record)?)?;
                }
                d.set_item("records", record_list)?;
            }
        }
        entities.append(d)?;
    }
    root.set_item("entities", entities)?;

    let warnings = PyList::empty_bound(py);
    for warning in &output.warnings {
        let d = PyDict::new_bound(py);
        d.set_item("code", &warning.code)?;
        d.set_item("message", &warning.message)?;
        warnings.append(d)?;
    }
    root.set_item("warnings", warnings)?;

    let typed_features = PyList::empty_bound(py);
    for feature in &output.document.typed_features {
        typed_features.append(typed_feature_instance_to_python(py, feature)?)?;
    }
    root.set_item("typed_features", typed_features)?;
    match &output.document.sfc_model {
        Some(model) => root.set_item("model", sfc_model_to_python(py, model)?)?,
        None => root.set_item("model", py.None())?,
    }

    Ok(root)
}

fn parse_for_python(
    py: Python<'_>,
    format: FileFormat,
    input: &Bound<'_, PyAny>,
    strict: Option<bool>,
) -> PyResult<Py<PyDict>> {
    let bytes = resolve_python_input(input)?;
    let parsed =
        parse_from_bytes(format, &bytes, strict.unwrap_or(true)).map_err(parse_error_to_py)?;
    let output = output_to_python(py, &parsed)?;
    Ok(output.unbind())
}

#[pyfunction]
fn hello_from_bin() -> String {
    "Hello from ezsxf!".to_string()
}

#[allow(clippy::useless_conversion)]
#[pyfunction]
#[pyo3(signature = (input, strict=None))]
fn parse_p21(
    py: Python<'_>,
    input: &Bound<'_, PyAny>,
    strict: Option<bool>,
) -> PyResult<Py<PyDict>> {
    parse_for_python(py, FileFormat::P21, input, strict)
}

#[allow(clippy::useless_conversion)]
#[pyfunction]
#[pyo3(signature = (input, strict=None))]
fn parse_sfc(
    py: Python<'_>,
    input: &Bound<'_, PyAny>,
    strict: Option<bool>,
) -> PyResult<Py<PyDict>> {
    parse_for_python(py, FileFormat::Sfc, input, strict)
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(hello_from_bin, m)?)?;
    m.add_function(wrap_pyfunction!(parse_p21, m)?)?;
    m.add_function(wrap_pyfunction!(parse_sfc, m)?)?;
    Ok(())
}
