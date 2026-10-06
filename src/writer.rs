//! Lossless-in-model SFC reserialization (SXF Ver.3.1 SFC §§1-1 and 1-3).
//!
//! Entity order, IDs, parameter values and header metadata remain unchanged.
//! Comments and the original byte spelling/encoding are not retained by the reader.

use std::fmt::Write as _;

use encoding_rs::SHIFT_JIS;

use crate::features::{required_sfc_version, validate_sfc_semantic_string};
use crate::model::*;
use crate::parser::parse_from_bytes;

#[derive(Debug, Clone, Copy, Default)]
pub struct SfcWriteOptions {
    /// Preserve SAF references without copying or validating the external files.
    pub allow_external_references: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteError(pub String);

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for WriteError {}

fn fail(message: impl Into<String>) -> WriteError {
    WriteError(message.into())
}

/// Validate first, then atomically replace the drawing file. External files are
/// not copied. Header FILE_NAME and external-reference names remain unchanged.
pub fn write_sfc(
    output: &ParseOutput,
    path: &std::path::Path,
    options: SfcWriteOptions,
) -> std::io::Result<()> {
    let bytes = serialize_sfc(output, options)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error))?;
    write_bytes_atomic(path, &bytes)
}

pub(crate) fn write_bytes_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FILE: AtomicU64 = AtomicU64::new(0);
    let name = path.file_name().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "Output must name a file")
    })?;
    let permissions = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Some(metadata.permissions()),
        Ok(_) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Output must be a regular file, not a directory or symlink",
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    // create_new makes a collision safe; only remove a temporary file we own.
    let (temporary, mut file) = loop {
        let mut temporary_name = std::ffi::OsString::from(".");
        temporary_name.push(name);
        temporary_name.push(format!(
            ".ezsxf-{}-{}.tmp",
            std::process::id(),
            NEXT_FILE.fetch_add(1, Ordering::Relaxed)
        ));
        let temporary = parent.join(temporary_name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => break (temporary, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    let result = (|| {
        if let Some(permissions) = permissions {
            file.set_permissions(permissions)?;
        }
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        // Windows refuses to remove a read-only temporary file. Only clear
        // attributes on our own staging file, never on the existing target.
        #[cfg(windows)]
        if let Ok(metadata) = fs::symlink_metadata(&temporary) {
            let mut permissions = metadata.permissions();
            if permissions.readonly() {
                #[allow(clippy::permissions_set_readonly_false)]
                // Windows read-only attribute only.
                permissions.set_readonly(false);
                let _ = fs::set_permissions(&temporary, permissions);
            }
        }
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Encode a complete, warning-free parse result as Shift-JIS SFC bytes.
/// Derived features and hierarchy must still agree with the raw entities.
pub fn serialize_sfc(
    output: &ParseOutput,
    options: SfcWriteOptions,
) -> Result<Vec<u8>, WriteError> {
    if !output.warnings.is_empty() {
        return Err(fail("Cannot save an SFC parse result containing warnings"));
    }
    let (bytes, validated) = encode_document(&output.document, options)?;
    if validated.document != output.document {
        return Err(fail(
            "SFC entities, typed_features and model disagree; reparse the source before saving",
        ));
    }
    Ok(bytes)
}

/// Used by the Python boundary before checking its duplicated dictionary views.
pub(crate) fn encode_document(
    document: &ParsedDocument,
    options: SfcWriteOptions,
) -> Result<(Vec<u8>, ParseOutput), WriteError> {
    if document.format != FileFormat::Sfc {
        return Err(fail("SFC serialization requires an SFC document"));
    }
    let mut text = String::from("ISO-10303-21;\r\nHEADER;\r\n");
    for record in &document.header.entities {
        keyword(&record.keyword)?;
        text.push_str(&record.keyword);
        text.push('(');
        emit_values(&mut text, &record.parameters, false, 0)?;
        text.push_str(");\r\n");
    }
    text.push_str("ENDSEC;\r\nDATA;\r\n");
    for entity in &document.entities {
        if entity.id < 0 {
            return Err(fail(format!("Invalid SFC entity ID #{}", entity.id)));
        }
        let EntityBody::Simple(record) = &entity.body else {
            return Err(fail(format!("SFC entity #{} must be simple", entity.id)));
        };
        let tag = required_sfc_version(&record.keyword)
            .ok_or_else(|| fail(format!("Unsupported SFC feature {}", record.keyword)))?;
        if entity.sfc_version != Some(tag) {
            return Err(fail(format!("Wrong SFC version marker for #{}", entity.id)));
        }
        let roles = parameter_roles(record)?;
        if roles.len() != record.parameters.len() {
            return Err(fail(format!("Invalid parameter count for #{}", entity.id)));
        }
        write!(
            text,
            "/*{}\r\n#{} = {}(",
            tag.marker(),
            entity.id,
            record.keyword
        )
        .unwrap();
        for (index, (value, role)) in record.parameters.iter().zip(roles.chars()).enumerate() {
            if index != 0 {
                text.push(',');
            }
            validate_parameter(value, role).map_err(|error| {
                fail(format!(
                    "#{} {} parameter {}: {error}",
                    entity.id,
                    record.keyword,
                    index + 1
                ))
            })?;
            emit_value(&mut text, value, role == 'T', 0)?;
        }
        validate_vertex_counts(record)?;
        write!(text, ")\r\n{}*/\r\n", tag.marker()).unwrap();
    }
    text.push_str("ENDSEC;\r\nEND-ISO-10303-21;\r\n");
    let (bytes, _, had_errors) = SHIFT_JIS.encode(&text);
    if had_errors {
        return Err(fail(
            "SFC contains characters not representable in Shift-JIS/CP932",
        ));
    }
    let (decoded, had_errors) = SHIFT_JIS.decode_without_bom_handling(&bytes);
    if had_errors || decoded != text {
        return Err(fail("Shift-JIS encoding would change SFC character values"));
    }
    let validated = parse_from_bytes(FileFormat::Sfc, &bytes, true)
        .map_err(|error| fail(format!("SFC output validation failed: {error}")))?;
    if let Some(warning) = validated.warnings.first() {
        return Err(fail(format!(
            "SFC output validation failed: {}: {}",
            warning.code, warning.message
        )));
    }
    if validated.document.entities != document.entities
        || validated.document.header != document.header
        || validated.document.typed_features.len() != document.entities.len()
    {
        return Err(fail("SFC output changed or omitted entity/header values"));
    }
    if !options.allow_external_references
        && validated.document.sfc_model.as_ref().is_some_and(|model| {
            model.attribute_attachments.iter().any(|attachment| {
                matches!(
                    attachment.mechanism,
                    SfcAttributeMechanism::AttributeFile { .. }
                )
            })
        })
    {
        return Err(fail(
            "SFC references an external SAF file; use allow_external_references=True to preserve references without copying SAF/images",
        ));
    }
    Ok((bytes.into_owned(), validated))
}

// I: integer, L: length (<=6 fractional digits), A: angle/scale (<=15 digits),
// T: semantic string, X/J: real/integer aggregate, H: seven-field hatch pattern.
// Keep field positions aligned with features.rs and SFC spec §§1-2-1..1-2-34.
fn parameter_roles(record: &Record) -> Result<String, WriteError> {
    let text = "IITLLLLLAAII";
    let extensions = "ILLLLLLILLLLLL";
    let arrows = "IILLAIILLA";
    Ok(match record.keyword.to_ascii_lowercase().as_str() {
        "drawing_attribute_feature" => "TTTTTTTIIITT".into(),
        "drawing_sheet_feature" => "TIIII".into(),
        "layer_feature" | "sfig_org_feature" => "TI".into(),
        "pre_defined_font_feature" | "pre_defined_colour_feature" | "text_font_feature" => {
            "T".into()
        }
        "user_defined_font_feature" => "TIX".into(),
        "user_defined_colour_feature" => "III".into(),
        "width_feature" => "L".into(),
        "point_marker_feature" => "IILLIAA".into(),
        "line_feature" => "IIIILLLL".into(),
        "polyline_feature" => "IIIIIXX".into(),
        "circle_feature" => "IIIILLL".into(),
        "arc_feature" => "IIIILLLIAA".into(),
        "ellipse_feature" => "IIIILLLLA".into(),
        "ellipse_arc_feature" => "IIIILLLLIAAA".into(),
        "text_string_feature" => "IIITLLLLLAAII".into(),
        "spline_feature" => "IIIIIIXX".into(),
        "clothoid_feature" => "IIIILLLIALL".into(),
        "sfig_locate_feature" => "ITLLAAA".into(),
        "symbol_externally_defined_feature" | "externally_defined_symbol_feature" => {
            "IIITLLAA".into()
        }
        "linear_dim_feature" => format!("IIIILLLL{extensions}{arrows}{text}"),
        "curve_dim_feature" | "angular_dim_feature" => {
            format!("IIIILLLAA{extensions}{arrows}{text}")
        }
        "radius_dim_feature" => format!("IIIILLLLIILLA{text}"),
        "diameter_dim_feature" => format!("IIIILLLL{arrows}{text}"),
        "label_feature" => format!("IIIIIXXIA{text}"),
        "balloon_feature" => format!("IIIIIXXLLLIA{text}"),
        "externally_defined_hatch_feature" => "ITIIJ".into(),
        "fill_area_style_colour_feature" => "IIIIJ".into(),
        "fill_area_style_hatching_feature" => {
            let count = record
                .parameters
                .get(1)
                .and_then(Value::as_i64)
                .filter(|count| (1..=4).contains(count))
                .ok_or_else(|| fail("Hatch pattern count must be in 1..=4"))?;
            format!("II{}IIJ", "H".repeat(count as usize))
        }
        "fill_area_style_tiles_hatching_feature" | "fill_area_style_tiles_feature" => {
            "ITILLLALAAAAIIJ".into()
        }
        "composite_curve_feature" | "composite_curve_org_feature" => "IIII".into(),
        _ => return Err(fail(format!("Unsupported SFC feature {}", record.keyword))),
    })
}

fn validate_parameter(value: &Value, role: char) -> Result<(), WriteError> {
    match role {
        'T' => match value {
            Value::String(text) => validate_sfc_semantic_string(text, "string", true).map_err(fail),
            _ => Err(fail("Expected a string")),
        },
        'X' | 'J' | 'H' => {
            let values = match value {
                Value::List(values) => values.clone(),
                Value::String(text) => {
                    let inner = text
                        .trim()
                        .strip_prefix('(')
                        .and_then(|text| text.strip_suffix(')'))
                        .ok_or_else(|| fail("Expected a parenthesized numeric aggregate"))?;
                    if inner.trim().is_empty() {
                        Vec::new()
                    } else {
                        inner
                            .split(',')
                            .map(|text| Value::String(text.trim().into()))
                            .collect()
                    }
                }
                _ => return Err(fail("Expected a numeric aggregate")),
            };
            if role == 'H' && values.len() != 7 {
                return Err(fail("Hatch pattern must contain seven values"));
            }
            for (index, item) in values.iter().enumerate() {
                let item_role = match role {
                    'J' => 'I',
                    'H' => "IIILLLA".chars().nth(index).unwrap(),
                    _ => 'L',
                };
                validate_parameter(item, item_role)?;
            }
            Ok(())
        }
        _ => {
            let text = match value {
                Value::String(text) => text.clone(),
                Value::Integer(number) => number.to_string(),
                Value::Real(number) if number.is_finite() && role != 'I' => real_literal(*number),
                _ => return Err(fail("Expected a finite numeric value")),
            };
            if text != text.trim() || text.contains(['e', 'E']) {
                return Err(fail(
                    "Numeric whitespace and exponent notation are not allowed",
                ));
            }
            if role == 'I' {
                text.parse::<i64>()
                    .map_err(|_| fail("Expected a decimal integer"))?;
            } else if value.as_f64().filter(|number| number.is_finite()).is_none() {
                return Err(fail("Expected a finite decimal real"));
            }
            if role == 'L'
                && text
                    .split_once('.')
                    .is_some_and(|(_, fraction)| fraction.len() > 6)
            {
                return Err(fail(
                    "Length exceeds six fractional digits; refusing to round",
                ));
            }
            if role == 'A' && text.chars().filter(char::is_ascii_digit).count() > 15 {
                return Err(fail("Angle/scale exceeds 15 digits; refusing to round"));
            }
            Ok(())
        }
    }
}

fn validate_vertex_counts(record: &Record) -> Result<(), WriteError> {
    if matches!(
        record.keyword.to_ascii_lowercase().as_str(),
        "label_feature" | "balloon_feature"
    ) {
        let count = record.parameters[4]
            .as_i64()
            .and_then(|n| usize::try_from(n).ok());
        let xs = record.parameters[5].as_f64_list().unwrap();
        let ys = record.parameters[6].as_f64_list().unwrap();
        if count != Some(xs.len()) || xs.len() != ys.len() {
            return Err(fail(format!(
                "{} vertex count/list lengths disagree",
                record.keyword
            )));
        }
    }
    Ok(())
}

fn keyword(value: &str) -> Result<(), WriteError> {
    let mut chars = value.chars();
    if !chars
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        || !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Err(fail("Invalid record keyword"));
    }
    Ok(())
}

// Keep an integral real distinguishable from Value::Integer after reparsing.
// Validate this exact spelling too, including the extra fractional digit.
fn real_literal(value: f64) -> String {
    let mut number = value.to_string();
    if !number.contains('.') {
        number.push_str(".0");
    }
    number
}

fn emit_values(
    text: &mut String,
    values: &[Value],
    semantic: bool,
    depth: usize,
) -> Result<(), WriteError> {
    for (index, value) in values.iter().enumerate() {
        if index != 0 {
            text.push(',');
        }
        emit_value(text, value, semantic, depth + 1)?;
    }
    Ok(())
}

fn emit_value(
    text: &mut String,
    value: &Value,
    semantic: bool,
    depth: usize,
) -> Result<(), WriteError> {
    if depth > 128 {
        return Err(fail("SFC value nesting exceeds 128 levels"));
    }
    match value {
        Value::String(value) => {
            if value.contains('\0') {
                return Err(fail("SFC string contains NUL"));
            }
            if semantic {
                text.push('\\');
            }
            text.push('\'');
            for ch in value.chars() {
                if ch == '\\' || (ch == '\'' && !semantic) {
                    text.push(ch);
                }
                text.push(ch);
            }
            if semantic {
                text.push('\\');
            }
            text.push('\'');
        }
        Value::Integer(value) => write!(text, "{value}").unwrap(),
        Value::Real(value) if value.is_finite() => text.push_str(&real_literal(*value)),
        Value::Real(_) => return Err(fail("SFC real must be finite")),
        Value::List(values) => {
            text.push('(');
            emit_values(text, values, semantic, depth)?;
            text.push(')');
        }
        // These are valid Part 21 header values, not SFC feature parameters.
        Value::Unset => text.push('$'),
        Value::Omitted => text.push('*'),
        Value::Reference(id) if *id >= 0 => write!(text, "#{id}").unwrap(),
        Value::Reference(_) => return Err(fail("Negative reference ID")),
        Value::Enum(value) => {
            keyword(value)?;
            write!(text, ".{value}.").unwrap();
        }
        Value::Binary(value) => {
            if !value.starts_with(['0', '1', '2', '3'])
                || !value.chars().all(|ch| ch.is_ascii_hexdigit())
            {
                return Err(fail("Invalid binary literal"));
            }
            write!(text, "\"{value}\"").unwrap();
        }
        Value::Typed {
            keyword: name,
            parameters,
        } => {
            keyword(name)?;
            write!(text, "{name}(").unwrap();
            emit_values(text, parameters, false, depth)?;
            text.push(')');
        }
    }
    Ok(())
}
