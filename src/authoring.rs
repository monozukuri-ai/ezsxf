//! Shared, format-independent authoring rules for bulk input and sheet/text APIs.
use crate::editor::{number, record, string};
use crate::model::{Record, Value};
use crate::writer::WriteError;
use encoding_rs::SHIFT_JIS;
use std::collections::BTreeMap;

fn error(message: impl Into<String>) -> WriteError {
    WriteError(message.into())
}

/// SFC Feature Specification, printed p.10: FREE X/Y are integer millimetres.
pub(crate) fn sheet_dimensions(
    paper: i64,
    orientation: i64,
    width: Option<f64>,
    height: Option<f64>,
) -> Result<(i64, i64), WriteError> {
    if !matches!(orientation, 0 | 1) {
        return Err(error("orientation must be 0 (portrait) or 1 (landscape)"));
    }
    let integer = |value: f64| {
        if !value.is_finite() || value <= 0.0 || value > f64::from(i32::MAX) {
            return Err(error(
                "Paper dimensions must be finite positive 32-bit integer millimetres",
            ));
        }
        if value.fract() != 0.0 {
            return Err(error("SFC paper dimensions require integer millimetres; round fractional dimensions explicitly"));
        }
        Ok(value as i64)
    };
    let supplied_x = width.map(integer).transpose()?;
    let supplied_y = height.map(integer).transpose()?;
    if paper == 9 {
        return Ok((supplied_x.unwrap_or(297), supplied_y.unwrap_or(210)));
    }
    let (short, long) = match paper {
        0 => (841, 1189),
        1 => (594, 841),
        2 => (420, 594),
        3 => (297, 420),
        4 => (210, 297),
        _ => return Err(error("paper must be A0..A4 (codes 0..4) or FREE (code 9)")),
    };
    let (x, y) = if orientation == 0 {
        (short, long)
    } else {
        (long, short)
    };
    if supplied_x.is_some_and(|v| v != x) || supplied_y.is_some_and(|v| v != y) {
        return Err(error(
            "Explicit dimensions conflict with standard paper and orientation",
        ));
    }
    Ok((x, y))
}

/// Monospaced horizontal box estimate, SXF Implementation Agreement §1-5.
/// Full-width glyphs occupy `height`; single-byte CP932 glyphs occupy half.
pub fn estimate_text_width(text: &str, height: f64, spacing: f64) -> Result<f64, WriteError> {
    crate::features::validate_sfc_semantic_string(text, "text", false).map_err(error)?;
    if !height.is_finite() || height <= 0.0 || !spacing.is_finite() || spacing < 0.0 {
        return Err(error(
            "Text height must be finite and positive; spacing must be finite and nonnegative",
        ));
    }
    if text.chars().any(char::is_control) {
        return Err(error(
            "Automatic text width requires a single line without control characters",
        ));
    }
    let (encoded, _, _) = SHIFT_JIS.encode(text);
    let (decoded, had_errors) = SHIFT_JIS.decode_without_bom_handling(&encoded);
    if had_errors || decoded != text {
        return Err(error("Text is not losslessly representable in CP932"));
    }
    // Each byte contributes half a full-width cell. Do not use byte count for gaps.
    let width = height * encoded.len() as f64 / 2.0
        + spacing * text.chars().count().saturating_sub(1) as f64;
    // Derived estimates are rounded to the SFC length precision, unlike user values.
    let width = (width * 1_000_000.0).round() / 1_000_000.0;
    if !width.is_finite() || width <= 0.0 || width >= 1.0e15 {
        return Err(error(
            "Estimated text width is outside the SXF length range",
        ));
    }
    Ok(width)
}

/// Construct basic records without cloning/validating the entire drawing.
/// Fields use update_element names, plus coordinate-pair conveniences.
pub(crate) fn basic_record(
    kind: &str,
    input: &BTreeMap<String, Value>,
) -> Result<Option<Record>, WriteError> {
    let (keyword, names): (&str, &str) = match kind {
        "line" | "line_feature" => ("line_feature", "layer color line_type line_width start_x start_y end_x end_y"),
        "circle" | "circle_feature" => ("circle_feature", "layer color line_type line_width center_x center_y radius"),
        "arc" | "arc_feature" => ("arc_feature", "layer color line_type line_width center_x center_y radius direction start_angle end_angle"),
        "polyline" | "polyline_feature" => ("polyline_feature", "layer color line_type line_width"),
        "text" | "text_string_feature" => ("text_string_feature", "layer color font text x y height width spacing angle slant base_point direction"),
        _ => return Ok(None),
    };
    let mut changes = input.clone();
    let pairs: &[(&str, &str, &str)] = match keyword {
        "line_feature" => &[("start", "start_x", "start_y"), ("end", "end_x", "end_y")],
        "circle_feature" | "arc_feature" => &[("center", "center_x", "center_y")],
        "text_string_feature" => &[("anchor", "x", "y")],
        _ => &[],
    };
    for (pair, x, y) in pairs {
        if let Some(value) = changes.remove(*pair) {
            let Value::List(values) = value else {
                return Err(error(format!("{pair} must be a coordinate pair")));
            };
            if values.len() != 2 || changes.contains_key(*x) || changes.contains_key(*y) {
                return Err(error(format!("Invalid or conflicting {pair} coordinates")));
            }
            changes.insert((*x).into(), values[0].clone());
            changes.insert((*y).into(), values[1].clone());
        }
    }
    let names: Vec<_> = names.split_whitespace().collect();
    let mut parameters = Vec::new();
    for name in &names {
        let default = match *name {
            "layer" | "color" | "line_type" | "line_width" | "font" | "base_point" => {
                Value::Integer(1)
            }
            "direction" if keyword == "text_string_feature" => Value::Integer(1),
            "height" | "width" => Value::Real(3.5),
            "radius" => Value::Real(10.0),
            "text" => string(""),
            _ => Value::Integer(0),
        };
        let value = changes.remove(*name).unwrap_or(default);
        parameters.push(if *name == "text" {
            if !matches!(value, Value::String(_)) {
                return Err(error("text must be a string"));
            }
            value
        } else {
            string(crate::editor::numeric_text(&value)?)
        });
    }
    if keyword == "polyline_feature" {
        let Some(Value::List(points)) = changes.remove("points") else {
            return Err(error("A polyline needs points"));
        };
        if points.len() < 2 {
            return Err(error("A polyline needs at least two points"));
        }
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        for point in &points {
            let Value::List(pair) = point else {
                return Err(error("Each point must be a coordinate pair"));
            };
            if pair.len() != 2 {
                return Err(error("Each point must have two coordinates"));
            }
            xs.push(crate::editor::numeric_text(&pair[0])?);
            ys.push(crate::editor::numeric_text(&pair[1])?);
        }
        parameters.extend([
            string(points.len()),
            string(format!("({})", xs.join(","))),
            string(format!("({})", ys.join(","))),
        ]);
    }
    if let Some(key) = changes.keys().next() {
        return Err(error(format!("Unsupported {keyword} field {key}")));
    }
    if keyword == "text_string_feature" && !input.contains_key("width") {
        if parameters[12].as_i64() != Some(1) {
            return Err(error(
                "Automatic text width supports horizontal text; supply width for vertical text",
            ));
        }
        let Value::String(text) = &parameters[3] else {
            unreachable!()
        };
        parameters[7] = number(estimate_text_width(
            text,
            parameters[6]
                .as_f64()
                .ok_or_else(|| error("Invalid text height"))?,
            parameters[8]
                .as_f64()
                .ok_or_else(|| error("Invalid text spacing"))?,
        )?);
    }
    Ok(Some(record(keyword, parameters)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standard_and_free_paper_keep_integer_contract() {
        assert_eq!(
            sheet_dimensions(1, 1, Some(841.0), None).unwrap(),
            (841, 594)
        );
        assert_eq!(sheet_dimensions(1, 0, None, None).unwrap(), (594, 841));
        assert!(sheet_dimensions(9, 1, Some(841.5), None).is_err());
        assert!(sheet_dimensions(1, 1, Some(297.0), None).is_err());
        assert!(sheet_dimensions(9, 1, Some(f64::NAN), None).is_err());
    }
    #[test]
    fn mixed_width_uses_character_gaps_and_cp932_cells() {
        assert_eq!(estimate_text_width("東京都", 3.5, 0.0).unwrap(), 10.5);
        assert_eq!(estimate_text_width("ＡＢＣABC", 10.0, 5.0).unwrap(), 70.0);
        assert_eq!(estimate_text_width("ｱA", 4.0, 1.0).unwrap(), 5.0);
        assert!(estimate_text_width("", 3.5, 0.0).is_err());
        assert!(estimate_text_width("😀", 3.5, 0.0).is_err());
    }
}
