//! Transactional editing of basic, direct drawing-sheet elements.

use crate::model::*;
use crate::writer::{encode_document, serialize_sfc, SfcWriteOptions, WriteError};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct SfcDocument {
    pub(crate) output: ParseOutput,
    pub(crate) saf: Option<crate::saf::SafDocument>,
    pub(crate) dependencies: BTreeMap<String, Vec<u8>>,
}

fn error(message: impl Into<String>) -> WriteError {
    WriteError(message.into())
}
pub(crate) fn string(value: impl ToString) -> Value {
    Value::String(value.to_string())
}
pub(crate) fn number(value: f64) -> Value {
    let mut text = value.to_string();
    if !text.contains('.') {
        text.push_str(".0");
    }
    Value::String(text)
}
pub(crate) fn record(keyword: &str, parameters: Vec<Value>) -> Record {
    Record {
        keyword: keyword.into(),
        parameters,
    }
}
pub(crate) fn instance(id: i64, record: Record) -> EntityInstance {
    EntityInstance {
        id,
        sfc_version: Some(SfcVersionTag::V2),
        body: EntityBody::Simple(record),
    }
}
fn validate(document: &ParsedDocument) -> Result<ParseOutput, WriteError> {
    encode_document(
        document,
        SfcWriteOptions {
            allow_external_references: true,
            ..SfcWriteOptions::default()
        },
    )
    .map(|(_, output)| output)
}
pub(crate) fn basic(keyword: &str) -> bool {
    matches!(
        keyword.to_ascii_lowercase().as_str(),
        "line_feature"
            | "circle_feature"
            | "arc_feature"
            | "polyline_feature"
            | "text_string_feature"
    )
}

impl SfcDocument {
    /// Create a free-size sheet in millimetres with layer/font code 1.
    pub fn new(
        file_name: &str,
        name: &str,
        width_mm: i64,
        height_mm: i64,
        timestamp: &str,
    ) -> Result<Self, WriteError> {
        let document = ParsedDocument {
            format: FileFormat::Sfc,
            header: HeaderSection {
                entities: vec![
                    record(
                        "FILE_DESCRIPTION",
                        vec![
                            Value::List(vec![string("SCADEC level2 feature_mode")]),
                            string("2;1"),
                        ],
                    ),
                    record(
                        "FILE_NAME",
                        vec![
                            string(file_name),
                            string(timestamp),
                            Value::List(vec![string("")]),
                            Value::List(vec![string("")]),
                            string("ezsxf$$3.1"),
                            string("ezsxf"),
                            string(""),
                        ],
                    ),
                    record(
                        "FILE_SCHEMA",
                        vec![Value::List(vec![string("ASSOCIATIVE_DRAUGHTING")])],
                    ),
                ],
            },
            entities: vec![
                instance(
                    1,
                    record("layer_feature", vec![string("default"), string(1)]),
                ),
                instance(
                    2,
                    record("pre_defined_font_feature", vec![string("continuous")]),
                ),
                instance(
                    3,
                    record("pre_defined_colour_feature", vec![string("black")]),
                ),
                instance(4, record("width_feature", vec![number(0.13)])),
                instance(
                    5,
                    record("text_font_feature", vec![string("ＭＳ ゴシック")]),
                ),
                instance(
                    6,
                    record(
                        "drawing_sheet_feature",
                        vec![
                            string(name),
                            string(9),
                            string(1),
                            string(width_mm),
                            string(height_mm),
                        ],
                    ),
                ),
            ],
            typed_features: vec![],
            sfc_model: None,
        };
        Ok(Self {
            output: validate(&document)?,
            saf: None,
            dependencies: BTreeMap::new(),
        })
    }

    pub fn from_output(output: ParseOutput) -> Result<Self, WriteError> {
        serialize_sfc(
            &output,
            SfcWriteOptions {
                allow_external_references: true,
                ..SfcWriteOptions::default()
            },
        )?;
        Ok(Self {
            output,
            saf: None,
            dependencies: BTreeMap::new(),
        })
    }
    pub fn snapshot(&self) -> &ParseOutput {
        &self.output
    }
    pub fn to_bytes(&self, options: SfcWriteOptions) -> Result<Vec<u8>, WriteError> {
        serialize_sfc(&self.output, options)
    }
    pub(crate) fn commit(&mut self, document: ParsedDocument) -> Result<(), WriteError> {
        let output = validate(&document)?;
        self.validate_image_links(&output)?;
        self.output = output;
        Ok(())
    }
    pub(crate) fn next_id(&self) -> Result<i64, WriteError> {
        self.output
            .document
            .entities
            .iter()
            .map(|e| e.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| error("SFC entity ID overflow"))
    }
    /// Add a basic primitive immediately before the sheet, without entering a group.
    pub fn add_element(
        &mut self,
        keyword: &str,
        parameters: Vec<Value>,
    ) -> Result<i64, WriteError> {
        if !basic(keyword) {
            return Err(error("Only basic primitives can be added"));
        }
        let id = self.next_id()?;
        let sheet_id = self
            .output
            .document
            .sfc_model
            .as_ref()
            .and_then(|m| m.sheet.as_ref())
            .ok_or_else(|| error("Missing sheet"))?
            .entity_id;
        let mut document = self.output.document.clone();
        let index = document
            .entities
            .iter()
            .position(|e| e.id == sheet_id)
            .unwrap();
        document
            .entities
            .insert(index, instance(id, record(keyword, parameters)));
        self.commit(document)?;
        Ok(id)
    }

    pub fn add_polyline(
        &mut self,
        codes: [i64; 4],
        points: &[(f64, f64)],
    ) -> Result<i64, WriteError> {
        if points.len() < 2 {
            return Err(error("A polyline needs at least two points"));
        }
        let mut parameters: Vec<_> = codes.into_iter().map(string).collect();
        let text = |axis: usize| {
            format!(
                "({})",
                points
                    .iter()
                    .map(|p| {
                        let Value::String(value) = number(if axis == 0 { p.0 } else { p.1 }) else {
                            unreachable!()
                        };
                        value
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        parameters.extend([string(points.len()), string(text(0)), string(text(1))]);
        self.add_element("polyline_feature", parameters)
    }
    pub(crate) fn editable_index(&self, id: i64) -> Result<usize, WriteError> {
        let index = self
            .output
            .document
            .entities
            .iter()
            .position(|e| e.id == id)
            .ok_or_else(|| error(format!("Unknown entity #{id}")))?;
        let EntityBody::Simple(record) = &self.output.document.entities[index].body else {
            return Err(error("Not a basic primitive"));
        };
        let direct = self
            .output
            .document
            .sfc_model
            .as_ref()
            .and_then(|m| m.sheet.as_ref())
            .is_some_and(|s| s.component_ids.contains(&id));
        let attached = self.attachment(id).is_some_and(|a| {
            self.output
                .document
                .sfc_model
                .as_ref()
                .unwrap()
                .sheet
                .as_ref()
                .unwrap()
                .component_ids
                .contains(&a.placement_ids[0])
        });
        if !basic(&record.keyword) || !(direct || attached) {
            return Err(error(
                "Editing requires a basic primitive directly on the drawing sheet",
            ));
        }
        Ok(index)
    }
    /// Change named parameters. Unspecified parameters, IDs and order are kept.
    pub fn update_element(
        &mut self,
        id: i64,
        changes: &BTreeMap<String, Value>,
    ) -> Result<(), WriteError> {
        let index = self.editable_index(id)?;
        let mut document = self.output.document.clone();
        let EntityBody::Simple(record) = &mut document.entities[index].body else {
            unreachable!()
        };
        let fields: &[&str] = match record.keyword.to_ascii_lowercase().as_str() {
            "line_feature" => &[
                "layer",
                "color",
                "line_type",
                "line_width",
                "start_x",
                "start_y",
                "end_x",
                "end_y",
            ],
            "circle_feature" => &[
                "layer",
                "color",
                "line_type",
                "line_width",
                "center_x",
                "center_y",
                "radius",
            ],
            "arc_feature" => &[
                "layer",
                "color",
                "line_type",
                "line_width",
                "center_x",
                "center_y",
                "radius",
                "direction",
                "start_angle",
                "end_angle",
            ],
            "polyline_feature" => &["layer", "color", "line_type", "line_width"],
            "text_string_feature" => &[
                "layer",
                "color",
                "font",
                "text",
                "x",
                "y",
                "height",
                "width",
                "spacing",
                "angle",
                "slant",
                "base_point",
                "direction",
            ],
            _ => unreachable!(),
        };
        for (key, value) in changes {
            if key == "points" && record.keyword.eq_ignore_ascii_case("polyline_feature") {
                let Value::List(points) = value else {
                    return Err(error("points must be a list of coordinate pairs"));
                };
                if points.len() < 2 {
                    return Err(error("A polyline needs at least two points"));
                }
                let mut xs = Vec::new();
                let mut ys = Vec::new();
                for point in points {
                    let Value::List(pair) = point else {
                        return Err(error("Each point must be a coordinate pair"));
                    };
                    if pair.len() != 2 {
                        return Err(error("Each point must have two coordinates"));
                    }
                    xs.push(numeric_text(&pair[0])?);
                    ys.push(numeric_text(&pair[1])?);
                }
                record.parameters[4] = string(points.len());
                record.parameters[5] = string(format!("({})", xs.join(",")));
                record.parameters[6] = string(format!("({})", ys.join(",")));
            } else {
                let field = fields
                    .iter()
                    .position(|field| field == key)
                    .ok_or_else(|| error(format!("Unsupported {} field {key}", record.keyword)))?;
                record.parameters[field] = if key == "text" {
                    match value {
                        Value::String(_) => value.clone(),
                        _ => return Err(error("text must be a string")),
                    }
                } else {
                    string(numeric_text(value)?)
                };
            }
        }
        self.commit(document)
    }
    pub fn remove_element(&mut self, id: i64) -> Result<(), WriteError> {
        let index = self.editable_index(id)?;
        if self.attachment(id).is_some() {
            let mut next = self.clone();
            next.remove_attachment(id)?;
            next.remove_element(id)?;
            *self = next;
            return Ok(());
        }
        let mut document = self.output.document.clone();
        document.entities.remove(index);
        self.commit(document)
    }
    pub fn add_layer(&mut self, name: &str, visible: bool) -> Result<i64, WriteError> {
        let code = self
            .output
            .document
            .sfc_model
            .as_ref()
            .unwrap()
            .code_tables
            .layers
            .len()
            + 1;
        self.add_definition(record(
            "layer_feature",
            vec![string(name), string(i64::from(visible))],
        ))?;
        Ok(code as i64)
    }
    /// Reuse an exact-name font rather than creating duplicate table entries.
    pub fn add_font(&mut self, name: &str) -> Result<i64, WriteError> {
        let document = &self.output.document;
        let fonts = &document.sfc_model.as_ref().unwrap().code_tables.text_fonts;
        for binding in fonts {
            if document.typed_features.iter().any(|entry| {
                entry.id == binding.entity_id
                    && matches!(&entry.feature, TypedFeature::TextFont(font) if font.name == name)
            }) {
                return Ok(binding.code);
            }
        }
        let code = fonts.len() + 1;
        self.add_definition(record("text_font_feature", vec![string(name)]))?;
        Ok(code as i64)
    }
    fn add_definition(&mut self, record: Record) -> Result<(), WriteError> {
        // Append to this definition table; inserting before existing table
        // entries would silently renumber every existing code after insertion.
        let index = self.output.document.entities.iter().rposition(|e| matches!(&e.body, EntityBody::Simple(r) if r.keyword.eq_ignore_ascii_case(&record.keyword))).map(|i| i + 1).unwrap_or(0);
        let mut document = self.output.document.clone();
        document
            .entities
            .insert(index, instance(self.next_id()?, record));
        self.commit(document)
    }
    pub fn rename_layer(&mut self, code: i64, name: &str) -> Result<(), WriteError> {
        let id = self
            .output
            .document
            .sfc_model
            .as_ref()
            .unwrap()
            .code_tables
            .layers
            .iter()
            .find(|b| b.code == code)
            .ok_or_else(|| error("Unknown layer code"))?
            .entity_id;
        let mut document = self.output.document.clone();
        let entity = document.entities.iter_mut().find(|e| e.id == id).unwrap();
        let EntityBody::Simple(record) = &mut entity.body else {
            unreachable!()
        };
        record.parameters[0] = string(name);
        self.commit(document)
    }
}

fn numeric_text(value: &Value) -> Result<String, WriteError> {
    match value {
        Value::Integer(v) => Ok(v.to_string()),
        Value::Real(v) if v.is_finite() => {
            let Value::String(text) = number(*v) else {
                unreachable!()
            };
            Ok(text)
        }
        _ => Err(error("Numeric editing fields require finite numbers")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_fonts_reuse_existing_codes_without_mutation() {
        let mut doc =
            SfcDocument::new("new.sfc", "drawing", 297, 210, "2026-10-06T00:00:00").unwrap();
        let original = doc.snapshot().clone();
        assert_eq!(doc.add_font("ＭＳ ゴシック").unwrap(), 1);
        assert_eq!(*doc.snapshot(), original);
        assert_eq!(doc.add_font("Arial").unwrap(), 2);
        let with_arial = doc.snapshot().clone();
        assert_eq!(doc.add_font("Arial").unwrap(), 2);
        assert_eq!(doc.add_font("ＭＳ ゴシック").unwrap(), 1);
        assert_eq!(*doc.snapshot(), with_arial);
        assert_eq!(doc.add_font("Times New Roman").unwrap(), 3);
        assert_eq!(doc.add_font("Arial").unwrap(), 2);
    }

    #[test]
    fn edit_is_transactional_and_preserves_ids() {
        let mut doc = SfcDocument::new("new.sfc", "図面", 297, 210, "2026-10-05T00:00:00").unwrap();
        let id = doc
            .add_element(
                "circle_feature",
                vec![
                    string(1),
                    string(1),
                    string(1),
                    string(1),
                    number(0.0),
                    number(0.0),
                    number(2.0),
                ],
            )
            .unwrap();
        let before = doc.snapshot().clone();
        assert!(doc
            .update_element(id, &BTreeMap::from([("radius".into(), Value::Real(-1.0))]))
            .is_err());
        assert_eq!(*doc.snapshot(), before);
        doc.update_element(id, &BTreeMap::from([("radius".into(), Value::Real(3.0))]))
            .unwrap();
        assert!(doc.snapshot().document.entities.iter().any(|e| e.id == id));
        doc.remove_element(id).unwrap();
        assert_eq!(
            *doc.snapshot(),
            SfcDocument::new("new.sfc", "図面", 297, 210, "2026-10-05T00:00:00")
                .unwrap()
                .output
        );
    }
}
