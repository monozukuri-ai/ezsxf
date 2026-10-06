//! Creation of SXF style definitions without renumbering existing bindings.

use crate::editor::{number, record, string, SfcDocument};
use crate::features::{
    normalized_predefined_name, predefined_color_code, predefined_line_type_code,
};
use crate::model::{SfcCodeBinding, TypedFeature, Value};
use crate::writer::WriteError;

fn error(message: &str) -> WriteError {
    WriteError(message.into())
}

impl SfcDocument {
    fn existing_style(
        &self,
        bindings: &[SfcCodeBinding],
        matches: impl Fn(&TypedFeature) -> bool,
    ) -> Option<i64> {
        bindings.iter().find_map(|binding| {
            self.output
                .document
                .typed_features
                .iter()
                .find_map(|entry| {
                    (entry.id == binding.entity_id && matches(&entry.feature))
                        .then_some(binding.code)
                })
        })
    }

    /// Named predefined colour; names are normalized like the parser's names.
    pub fn add_named_color(&mut self, name: &str) -> Result<i64, WriteError> {
        let code = predefined_color_code(name).ok_or_else(|| error("Unknown predefined colour"))?;
        let bindings = &self
            .output
            .document
            .sfc_model
            .as_ref()
            .unwrap()
            .code_tables
            .colors;
        if bindings.iter().any(|binding| binding.code == code) {
            return Ok(code);
        }
        self.add_definition(record(
            "pre_defined_colour_feature",
            vec![string(normalized_predefined_name(name))],
        ))?;
        Ok(code)
    }

    /// Reuse the exact RGB triple, or append a user colour in codes 17..256.
    pub fn add_rgb_color(&mut self, rgb: [i64; 3]) -> Result<i64, WriteError> {
        if rgb.iter().any(|component| !(0..=255).contains(component)) {
            return Err(error("RGB components must be integers in 0..255"));
        }
        let bindings = &self
            .output
            .document
            .sfc_model
            .as_ref()
            .unwrap()
            .code_tables
            .colors;
        if let Some(code) = self.existing_style(bindings, |feature| {
            matches!(feature, TypedFeature::UserDefinedColour(c) if [c.red, c.green, c.blue] == rgb)
        }) {
            return Ok(code);
        }
        let code = 17 + bindings.iter().filter(|b| b.code >= 17).count() as i64;
        if code > 256 {
            return Err(error("SXF allows at most 240 user-defined colours"));
        }
        self.add_definition(record(
            "user_defined_colour_feature",
            rgb.into_iter().map(string).collect(),
        ))?;
        Ok(code)
    }

    /// Predefined line type, or an even 2..8 draw/gap pattern in paper mm.
    pub fn add_line_type(
        &mut self,
        name: &str,
        pattern: Option<&[f64]>,
    ) -> Result<i64, WriteError> {
        let bindings = &self
            .output
            .document
            .sfc_model
            .as_ref()
            .unwrap()
            .code_tables
            .line_types;
        let Some(pattern) = pattern else {
            let code = predefined_line_type_code(name)
                .ok_or_else(|| error("Unknown predefined line type"))?;
            if bindings.iter().any(|binding| binding.code == code) {
                return Ok(code);
            }
            self.add_definition(record(
                "pre_defined_font_feature",
                vec![string(normalized_predefined_name(name))],
            ))?;
            return Ok(code);
        };
        if name.trim().is_empty() || predefined_line_type_code(name).is_some() {
            return Err(error(
                "Custom line type requires a nonempty, non-predefined name",
            ));
        }
        if !(2..=8).contains(&pattern.len()) || pattern.len() % 2 != 0 {
            return Err(error("Line pattern requires 2, 4, 6 or 8 draw/gap lengths"));
        }
        if pattern
            .iter()
            .any(|p| !p.is_finite() || *p <= 0.0 || *p >= 1.0e15)
        {
            return Err(error(
                "Line pattern lengths must be finite and in 0 < mm < 1e15",
            ));
        }
        for entry in &self.output.document.typed_features {
            if let TypedFeature::UserDefinedFont(font) = &entry.feature {
                if font.name == name {
                    if font.pitch != pattern {
                        return Err(error(
                            "Custom line type name already has a different pattern",
                        ));
                    }
                    return Ok(bindings
                        .iter()
                        .find(|b| b.entity_id == entry.id)
                        .unwrap()
                        .code);
                }
            }
        }
        let code = 17 + bindings.iter().filter(|b| b.code >= 17).count() as i64;
        if code > 32 {
            return Err(error("SXF allows at most 16 user-defined line types"));
        }
        let pitches = pattern
            .iter()
            .map(|p| {
                let Value::String(text) = number(*p) else {
                    unreachable!()
                };
                text
            })
            .collect::<Vec<_>>()
            .join(",");
        self.add_definition(record(
            "user_defined_font_feature",
            vec![
                string(name),
                string(pattern.len()),
                string(format!("({pitches})")),
            ],
        ))?;
        Ok(code)
    }

    /// Reuse an exact width, or append one of six custom paper-mm widths.
    pub fn add_line_width(&mut self, width_mm: f64) -> Result<i64, WriteError> {
        if !width_mm.is_finite() || width_mm <= 0.0 || width_mm >= 1.0e15 {
            return Err(error("Line width must be finite and in 0 < mm < 1e15"));
        }
        let bindings = &self
            .output
            .document
            .sfc_model
            .as_ref()
            .unwrap()
            .code_tables
            .line_widths;
        if let Some(code) = self.existing_style(
            bindings,
            |feature| matches!(feature, TypedFeature::Width(width) if width.width == width_mm),
        ) {
            return Ok(code);
        }
        let id = self.add_definition(record("width_feature", vec![number(width_mm)]))?;
        // The validated model applies the specification's predefined/custom codes.
        Ok(self
            .output
            .document
            .sfc_model
            .as_ref()
            .unwrap()
            .code_tables
            .line_widths
            .iter()
            .find(|binding| binding.entity_id == id)
            .unwrap()
            .code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drawing() -> SfcDocument {
        SfcDocument::new("mvp.sfc", "styles", 297, 210, "2026-10-06T00:00:00").unwrap()
    }

    #[test]
    fn predefined_codes_and_exact_definitions_are_reused() {
        let mut doc = drawing();
        assert_eq!(doc.add_named_color(" Red ").unwrap(), 2);
        assert_eq!(doc.add_line_type(" DASHED ", None).unwrap(), 2);
        assert_eq!(doc.add_line_width(0.35).unwrap(), 4);
        assert_eq!(doc.add_rgb_color([12, 34, 56]).unwrap(), 17);
        assert_eq!(doc.add_line_type("独自", Some(&[5.0, 2.0])).unwrap(), 17);
        assert_eq!(doc.add_line_width(0.42).unwrap(), 11);
        let before = doc.snapshot().clone();
        assert_eq!(doc.add_named_color("red").unwrap(), 2);
        assert_eq!(doc.add_line_type("dashed", None).unwrap(), 2);
        assert_eq!(doc.add_line_width(0.35).unwrap(), 4);
        assert_eq!(doc.add_rgb_color([12, 34, 56]).unwrap(), 17);
        assert_eq!(doc.add_line_type("独自", Some(&[5.0, 2.0])).unwrap(), 17);
        assert_eq!(doc.add_line_width(0.42).unwrap(), 11);
        assert_eq!(*doc.snapshot(), before);
    }

    #[test]
    fn invalid_definitions_roll_back_including_precision_and_name_collisions() {
        let mut doc = drawing();
        doc.add_line_type("独自", Some(&[5.0, 2.0])).unwrap();
        let before = doc.snapshot().clone();
        for rgb in [[-1, 0, 0], [0, 256, 0]] {
            assert!(doc.add_rgb_color(rgb).is_err());
        }
        for name in ["unknown", "😀"] {
            assert!(doc.add_named_color(name).is_err());
        }
        for width in [0.0, -1.0, f64::NAN, f64::INFINITY, 1.0e15, 0.130000001] {
            assert!(doc.add_line_width(width).is_err(), "{width}");
            assert_eq!(*doc.snapshot(), before);
        }
        for pattern in [
            vec![],
            vec![1.0],
            vec![1.0, 2.0, 3.0],
            vec![0.0, 1.0],
            vec![1.1234567, 1.0],
            vec![f64::NAN, 1.0],
        ] {
            assert!(doc.add_line_type("invalid", Some(&pattern)).is_err());
        }
        assert!(doc.add_line_type("dashed", Some(&[1.0, 1.0])).is_err());
        assert!(doc.add_line_type("独自", Some(&[1.0, 1.0])).is_err());
        assert!(doc.add_line_type("😀", Some(&[1.0, 1.0])).is_err());
        assert_eq!(*doc.snapshot(), before);
    }

    #[test]
    fn style_table_limits_allow_reuse_but_reject_new_entries() {
        let mut doc = drawing();
        for i in 0..240 {
            assert_eq!(doc.add_rgb_color([i, 1, 2]).unwrap(), 17 + i);
        }
        for i in 0..16 {
            assert_eq!(
                doc.add_line_type(&format!("custom{i}"), Some(&[1.0, 1.0]))
                    .unwrap(),
                17 + i
            );
        }
        for i in 0..6 {
            assert_eq!(doc.add_line_width(3.0 + i as f64).unwrap(), 11 + i);
        }
        let before = doc.snapshot().clone();
        assert!(doc.add_rgb_color([240, 1, 2]).is_err());
        assert!(doc.add_line_type("overflow", Some(&[1.0, 1.0])).is_err());
        assert!(doc.add_line_width(9.0).is_err());
        assert_eq!(doc.add_rgb_color([0, 1, 2]).unwrap(), 17);
        assert_eq!(doc.add_line_type("custom0", Some(&[1.0, 1.0])).unwrap(), 17);
        assert_eq!(doc.add_line_width(3.0).unwrap(), 11);
        assert_eq!(doc.add_named_color("blue").unwrap(), 4);
        // Predefined definitions remain available after filling user tables.
        assert_eq!(doc.add_line_type("chain", None).unwrap(), 8);
        assert_eq!(doc.add_line_width(2.0).unwrap(), 9);
        for entity in &before.document.entities {
            assert!(doc.snapshot().document.entities.contains(entity));
        }
    }
}
