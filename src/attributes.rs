//! Transactional attribute attachment and SAF/dependency authoring.

use crate::editor::{instance, number, record, string, SfcDocument};
use crate::model::*;
use crate::saf::{SafAttribute, SafDocument, SafSet};
use crate::writer::{encode_document, SfcWriteOptions, WriteError};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

fn error(message: impl std::fmt::Display) -> WriteError {
    WriteError(message.to_string())
}

const FEATURE_SET: (&str, &str, &str) = ("フィーチャ定義属性セット", "1.0", "SCADEC");

impl SfcDocument {
    pub(crate) fn attachment(&self, id: i64) -> Option<&SfcAttributeAttachment> {
        self.output
            .document
            .sfc_model
            .as_ref()?
            .attribute_attachments
            .iter()
            .find(|a| a.component_ids == [id])
    }
    fn drawing_name(&self) -> Result<&str, WriteError> {
        match self
            .output
            .document
            .header
            .find_keyword("FILE_NAME")
            .and_then(|r| r.parameters.first())
        {
            Some(Value::String(name)) => Ok(name),
            _ => Err(error("Missing FILE_NAME")),
        }
    }
    pub fn from_bundle(source: &Path) -> Result<Self, WriteError> {
        let mut files = crate::bundle::prepare_bundle(source, None, &[]).map_err(error)?;
        let name = source
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| error("Non-Unicode drawing filename"))?;
        let bytes = files.remove(name).unwrap();
        let output =
            crate::parser::parse_from_bytes(FileFormat::Sfc, &bytes, true).map_err(error)?;
        let mut result = Self::from_output(output)?;
        let saf_name = result
            .output
            .document
            .sfc_model
            .as_ref()
            .unwrap()
            .attribute_attachments
            .iter()
            .find_map(|a| a.resolved_attribute_file_name.clone());
        if let Some(name) = saf_name {
            result.saf = Some(SafDocument::parse(
                &files.remove(&name).ok_or_else(|| error("Missing SAF"))?,
            )?);
        }
        result.dependencies = files;
        result.validate_links()?;
        Ok(result)
    }
    fn figure_id(&self) -> String {
        let used: BTreeSet<_> = self
            .output
            .document
            .sfc_model
            .as_ref()
            .unwrap()
            .attribute_attachments
            .iter()
            .map(|a| crate::parser::sfc_attribute_figure_id(&a.mechanism))
            .collect();
        let mut id = 1usize;
        while used.contains(id.to_string().as_str()) {
            id += 1;
        }
        id.to_string()
    }
    fn wrap(&mut self, id: i64, name: String) -> Result<(), WriteError> {
        let target_index = self.editable_index(id)?;
        if self.attachment(id).is_some() {
            return self.rename_attachment(id, name);
        }
        let definition_id = self.next_id()?;
        let placement_id = definition_id
            .checked_add(1)
            .ok_or_else(|| error("SFC ID overflow"))?;
        let mut document = self.output.document.clone();
        let target = document.entities[target_index].clone();
        let mut definition = instance(
            definition_id,
            record("sfig_org_feature", vec![string(&name), string(3)]),
        );
        definition.sfc_version = Some(SfcVersionTag::V2);
        let mut placement = instance(
            placement_id,
            record(
                "sfig_locate_feature",
                vec![
                    string(0),
                    string(&name),
                    number(0.),
                    number(0.),
                    number(0.),
                    number(1.),
                    number(1.),
                ],
            ),
        );
        placement.sfc_version = Some(SfcVersionTag::V2);
        document.entities[target_index] = placement;
        // Definitions consume preceding geometry. Insert the single target before
        // all geometry so no unrelated sheet element enters the wrapper.
        let index = document
            .entities
            .iter()
            .position(|e| match &e.body {
                EntityBody::Simple(r) => !matches!(
                    r.keyword.to_ascii_lowercase().as_str(),
                    "layer_feature"
                        | "pre_defined_font_feature"
                        | "user_defined_font_feature"
                        | "pre_defined_colour_feature"
                        | "user_defined_colour_feature"
                        | "width_feature"
                        | "text_font_feature"
                        | "drawing_attribute_feature"
                ),
                _ => true,
            })
            .unwrap();
        document.entities.splice(index..index, [target, definition]);
        self.commit(document)
    }
    fn rename_attachment(&mut self, id: i64, name: String) -> Result<(), WriteError> {
        let a = self
            .attachment(id)
            .ok_or_else(|| error("Element has no attachment"))?;
        let ids = (a.definition_id, a.placement_ids[0]);
        let mut document = self.output.document.clone();
        for e in &mut document.entities {
            if e.id == ids.0 || e.id == ids.1 {
                let EntityBody::Simple(r) = &mut e.body else {
                    unreachable!()
                };
                r.parameters[usize::from(e.id == ids.1)] = string(&name);
            }
        }
        self.commit(document)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn set_attribute(
        &mut self,
        id: i64,
        figure_name: &str,
        metadata: (&str, &str, &str),
        attribute: SafAttribute,
    ) -> Result<String, WriteError> {
        self.editable_index(id)?;
        let figure_id = match self.attachment(id) {
            Some(a) => match &a.mechanism {
                SfcAttributeMechanism::AttributeFile { figure_id, .. } => {
                    if self.saf.is_none() {
                        return Err(error("Load the existing SAF with edit_sfc_bundle before editing its attributes"));
                    }
                    figure_id.clone()
                }
                _ => {
                    return Err(error(
                        "Remove the inline attachment before switching to SAF attributes",
                    ))
                }
            },
            None => self.figure_id(),
        };
        if matches!(attribute.name.as_str(), "画像" | "ファイル名") {
            crate::bundle::portable_name(&attribute.value).map_err(error)?;
        }
        if attribute.value == "$$$" && !self.is_text(id) {
            return Err(error("The $$$ attribute value requires a text feature"));
        }
        let loaded_legacy_image_set = metadata == ("フィーチャ定義属性セット", "0", "SCADEC")
            && self.attachment(id).is_some()
            && self.saf.as_ref().is_some_and(|s| {
                s.sets.iter().any(|set| {
                    (
                        set.name.as_str(),
                        set.version.as_str(),
                        set.designed_by.as_str(),
                    ) == metadata
                })
            });
        if attribute.name == "画像" && metadata != FEATURE_SET && !loaded_legacy_image_set {
            return Err(error(
                "Image attributes require the standard feature-definition set",
            ));
        }
        let mut next = self.clone();
        if next.saf.is_none() {
            if next
                .output
                .document
                .sfc_model
                .as_ref()
                .unwrap()
                .attribute_attachments
                .iter()
                .any(|a| matches!(a.mechanism, SfcAttributeMechanism::AttributeFile { .. }))
            {
                return Err(error(
                    "Existing SAF must be loaded before adding attributes",
                ));
            }
            let timestamp = next
                .output
                .document
                .header
                .find_keyword("FILE_NAME")
                .and_then(|r| r.parameters.get(1))
                .and_then(|v| {
                    if let Value::String(s) = v {
                        Some(s.as_str())
                    } else {
                        None
                    }
                })
                .unwrap_or("1970-01-01");
            next.saf = Some(SafDocument::new(
                next.drawing_name()?.into(),
                timestamp.chars().take(10).collect(),
            ));
        }
        next.saf
            .as_mut()
            .unwrap()
            .set(&figure_id, figure_name, metadata, attribute)?;
        next.wrap(id, format!("$$ATRF$${figure_id}"))?;
        next.validate_links()?;
        *self = next;
        Ok(figure_id)
    }
    fn is_text(&self, id: i64) -> bool {
        self.output
            .document
            .typed_features
            .iter()
            .any(|f| f.id == id && matches!(f.feature, TypedFeature::Text(_)))
    }
    pub fn attributes(&self, id: i64) -> Result<Vec<(SafSet, SafAttribute)>, WriteError> {
        self.editable_index(id)?;
        let Some(a) = self.attachment(id) else {
            return Ok(vec![]);
        };
        let SfcAttributeMechanism::AttributeFile { figure_id, .. } = &a.mechanism else {
            return Err(error("Use to_dict for inline attribute metadata"));
        };
        let saf = self
            .saf
            .as_ref()
            .ok_or_else(|| error("Load the existing SAF with edit_sfc_bundle"))?;
        let f = saf
            .figures
            .iter()
            .find(|f| &f.id == figure_id)
            .ok_or_else(|| error("Missing SAF Figure"))?;
        Ok(f.sets
            .iter()
            .flat_map(|(id, attrs)| {
                let set = saf.sets.iter().find(|s| &s.id == id).unwrap();
                attrs.iter().map(move |a| (set.clone(), a.clone()))
            })
            .collect())
    }
    pub fn remove_attribute(
        &mut self,
        id: i64,
        metadata: (&str, &str, &str),
        name: &str,
        group: &[String],
    ) -> Result<(), WriteError> {
        self.editable_index(id)?;
        let a = self
            .attachment(id)
            .ok_or_else(|| error("No attribute attachment"))?;
        let SfcAttributeMechanism::AttributeFile { figure_id, .. } = &a.mechanism else {
            return Err(error("Use remove_attachment for inline attributes"));
        };
        let mut next = self.clone();
        let empty = next
            .saf
            .as_mut()
            .ok_or_else(|| error("Load the existing SAF with edit_sfc_bundle"))?
            .remove(figure_id, metadata, name, group)?;
        if empty {
            next.remove_attachment(id)?;
        }
        next.validate_links()?;
        *self = next;
        Ok(())
    }
    pub fn remove_attachment(&mut self, id: i64) -> Result<(), WriteError> {
        self.editable_index(id)?;
        let a = self
            .attachment(id)
            .ok_or_else(|| error("No attribute attachment"))?
            .clone();
        let mut next = self.clone();
        if let SfcAttributeMechanism::AttributeFile { figure_id, .. } = a.mechanism {
            let saf = next.saf.as_mut().ok_or_else(|| {
                error("Load the existing SAF with edit_sfc_bundle before removing attributes")
            })?;
            if saf
                .figures
                .iter()
                .flat_map(|f| f.sets.values())
                .flatten()
                .any(|a| a.name == "ターゲット" && a.value == figure_id)
            {
                return Err(error("Another SAF attribute targets this Figure"));
            }
            saf.figures.retain(|f| f.id != figure_id);
            if saf.figures.is_empty() {
                next.saf = None;
            }
        }
        let mut document = next.output.document.clone();
        let target = document
            .entities
            .iter()
            .find(|e| e.id == id)
            .unwrap()
            .clone();
        document
            .entities
            .retain(|e| e.id != id && e.id != a.definition_id);
        let placement = document
            .entities
            .iter_mut()
            .find(|e| e.id == a.placement_ids[0])
            .unwrap();
        *placement = target;
        next.commit(document)?;
        *self = next;
        Ok(())
    }
    pub fn set_single_attribute(
        &mut self,
        id: i64,
        figure_name: &str,
        attribute: SafAttribute,
    ) -> Result<String, WriteError> {
        attribute.validate()?;
        if attribute.value == "$$$" && !self.is_text(id) {
            return Err(error("The $$$ attribute value requires a text feature"));
        }
        if matches!(attribute.name.as_str(), "画像" | "ファイル名") {
            crate::bundle::portable_name(&attribute.value).map_err(error)?;
        }
        if !attribute.group.is_empty() {
            return Err(error("Inline attributes cannot have groups"));
        }
        let mut fields = vec![figure_name.to_string(), attribute.name, attribute.value];
        if let Some(kind) = attribute.attribute_type {
            fields.push(kind);
        }
        if let Some(unit) = attribute.unit {
            if fields.len() < 4 {
                return Err(error("Inline unit requires type"));
            }
            fields.push(unit);
        }
        self.set_inline(id, "ATRU", fields)
    }
    pub fn set_text_attribute(
        &mut self,
        id: i64,
        name: &str,
        attribute_type: Option<&str>,
        unit: Option<&str>,
    ) -> Result<String, WriteError> {
        SafAttribute {
            name: name.into(),
            value: String::new(),
            attribute_type: attribute_type.map(str::to_owned),
            unit: unit.map(str::to_owned),
            group: vec![],
        }
        .validate()?;
        if !self.is_text(id) {
            return Err(error("ATRS requires a text feature"));
        }
        let mut fields = vec![name.to_string()];
        if let Some(kind) = attribute_type {
            fields.push(kind.into());
        }
        if let Some(unit) = unit {
            if fields.len() < 2 {
                return Err(error("Inline unit requires type"));
            }
            fields.push(unit.into());
        }
        self.set_inline(id, "ATRS", fields)
    }
    fn set_inline(
        &mut self,
        id: i64,
        mechanism: &str,
        fields: Vec<String>,
    ) -> Result<String, WriteError> {
        self.editable_index(id)?;
        if fields.iter().any(|s| s.is_empty() || s.contains("$$")) {
            return Err(error(
                "Inline attribute fields must be nonempty and cannot contain $$",
            ));
        }
        let figure_id = match self.attachment(id) {
            Some(a) => {
                if matches!(a.mechanism, SfcAttributeMechanism::AttributeFile { .. }) {
                    return Err(error(
                        "Remove SAF attachment before switching to inline attributes",
                    ));
                }
                crate::parser::sfc_attribute_figure_id(&a.mechanism).to_string()
            }
            None => self.figure_id(),
        };
        let mut next = self.clone();
        next.wrap(
            id,
            format!("$${mechanism}$${figure_id}$${}", fields.join("$$")),
        )?;
        *self = next;
        Ok(figure_id)
    }
    pub fn add_dependency(
        &mut self,
        source: &Path,
        file_name: Option<&str>,
    ) -> Result<String, WriteError> {
        if !std::fs::symlink_metadata(source).map_err(error)?.is_file() {
            return Err(error("Dependency must be a regular file"));
        }
        let name = file_name
            .or_else(|| source.file_name().and_then(|s| s.to_str()))
            .ok_or_else(|| error("Non-Unicode dependency filename"))?;
        crate::bundle::portable_name(name).map_err(error)?;
        let bytes = std::fs::read(source).map_err(error)?;
        if self.dependencies.iter().any(|(key, value)| {
            key.to_uppercase() == name.to_uppercase() && (key != name || value != &bytes)
        }) {
            return Err(error(
                "Dependency filename collision; use a distinct file_name",
            ));
        }
        self.dependencies.insert(name.into(), bytes);
        Ok(name.into())
    }
    pub fn saf_bytes(&self) -> Result<Option<Vec<u8>>, WriteError> {
        self.validate_links()?;
        self.saf
            .as_ref()
            .map(|s| s.to_bytes(self.drawing_name()?))
            .transpose()
    }
    fn validate_links(&self) -> Result<(), WriteError> {
        let model = self.output.document.sfc_model.as_ref().unwrap();
        let ids: BTreeSet<_> = model
            .attribute_attachments
            .iter()
            .filter_map(|a| {
                if let SfcAttributeMechanism::AttributeFile { figure_id, .. } = &a.mechanism {
                    Some(figure_id.as_str())
                } else {
                    None
                }
            })
            .collect();
        if !ids.is_empty() {
            let saf = self
                .saf
                .as_ref()
                .ok_or_else(|| error("Load the existing SAF with edit_sfc_bundle"))?;
            saf.validate()?;
            if ids != saf.figures.iter().map(|f| f.id.as_str()).collect() {
                return Err(error("SAF Figure IDs do not match drawing ATRF IDs"));
            }
        } else if self.saf.is_some() {
            return Err(error("SAF is present without ATRF references"));
        }
        self.validate_image_links(&self.output)
    }
    pub fn save_bundle(
        &self,
        destination: &Path,
        file_name: Option<&str>,
    ) -> std::io::Result<crate::bundle::SfcBundleReport> {
        self.save_bundle_with_options(destination, file_name, SfcWriteOptions::default())
    }

    /// Save a validated bundle with an explicit SFC spelling option. Dependencies
    /// are validated by this operation, so external references are permitted.
    pub fn save_bundle_with_options(
        &self,
        destination: &Path,
        file_name: Option<&str>,
        mut options: SfcWriteOptions,
    ) -> std::io::Result<crate::bundle::SfcBundleReport> {
        options.allow_external_references = true;
        self.validate_links()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()))?;
        let invalid =
            |e: WriteError| std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string());
        let name = file_name.unwrap_or(self.drawing_name().map_err(invalid)?);
        crate::bundle::portable_name(name)?;
        if !name.to_ascii_lowercase().ends_with(".sfc") {
            return Err(invalid(error("Drawing filename must end in .sfc")));
        }
        let saf_name = Path::new(name)
            .with_extension("SAF")
            .to_str()
            .unwrap()
            .to_string();
        let mut document = self.output.document.clone();
        document
            .header
            .entities
            .iter_mut()
            .find(|r| r.keyword.eq_ignore_ascii_case("FILE_NAME"))
            .unwrap()
            .parameters[0] = string(name);
        let replacements: BTreeMap<_, _> = document
            .sfc_model
            .as_ref()
            .unwrap()
            .attribute_attachments
            .iter()
            .filter_map(|a| match &a.mechanism {
                SfcAttributeMechanism::AttributeFile { figure_id, .. } => {
                    Some((a.name.clone(), format!("$$ATRF$${figure_id}$${saf_name}")))
                }
                _ => None,
            })
            .collect();
        for e in &mut document.entities {
            if let EntityBody::Simple(r) = &mut e.body {
                let field = if r.keyword.eq_ignore_ascii_case("sfig_org_feature") {
                    Some(0)
                } else if r.keyword.eq_ignore_ascii_case("sfig_locate_feature") {
                    Some(1)
                } else {
                    None
                };
                if let Some(field) = field {
                    if let Value::String(value) = &mut r.parameters[field] {
                        if let Some(new) = replacements.get(value) {
                            *value = new.clone();
                        }
                    }
                }
            }
        }
        let mut files = BTreeMap::new();
        let mut required = self
            .saf
            .as_ref()
            .map_or_else(BTreeSet::new, SafDocument::dependencies);
        for a in &self
            .output
            .document
            .sfc_model
            .as_ref()
            .unwrap()
            .attribute_attachments
        {
            if let SfcAttributeMechanism::SingleAttribute {
                attribute_name: Some(n),
                attribute_value: Some(v),
                ..
            } = &a.mechanism
            {
                if matches!(n.as_str(), "画像" | "ファイル名") {
                    required.insert(v.clone());
                }
            }
        }
        for dependency in required {
            crate::bundle::portable_name(&dependency)?;
            let candidates: Vec<_> = self
                .dependencies
                .iter()
                .filter(|(n, _)| n.to_uppercase() == dependency.to_uppercase())
                .collect();
            if candidates.len() != 1 {
                return Err(invalid(error(format!(
                    "Missing or ambiguous bundle dependency {dependency}"
                ))));
            }
            files.insert(dependency, candidates[0].1.clone());
        }
        if let Some(saf) = &self.saf {
            if files
                .insert(saf_name, saf.to_bytes(name).map_err(invalid)?)
                .is_some()
            {
                return Err(invalid(error("Dependency collides with SAF")));
            }
        }
        let (bytes, _) = encode_document(&document, options).map_err(invalid)?;
        if files.insert(name.into(), bytes).is_some() {
            return Err(invalid(error("Dependency collides with SFC")));
        }
        crate::bundle::publish_bundle(destination, name, &files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn line(doc: &mut SfcDocument, x: f64) -> i64 {
        doc.add_element(
            "line_feature",
            vec![
                string(1),
                string(1),
                string(1),
                string(1),
                number(x),
                number(0.),
                number(x + 1.),
                number(1.),
            ],
        )
        .unwrap()
    }
    fn attr(name: &str, value: &str) -> SafAttribute {
        SafAttribute {
            name: name.into(),
            value: value.into(),
            attribute_type: Some("STR".into()),
            unit: None,
            group: vec![],
        }
    }
    #[test]
    fn wrapping_does_not_consume_unrelated_geometry_and_detaches_in_place() {
        let mut doc =
            SfcDocument::new("x.sfc", "drawing", 297, 210, "2026-10-06T00:00:00").unwrap();
        let a = line(&mut doc, 0.);
        let b = line(&mut doc, 2.);
        let c = line(&mut doc, 4.);
        let before = doc.snapshot().document.clone();
        doc.set_attribute(b, "line", ("custom", "1", "test"), attr("label", "v"))
            .unwrap();
        let model = doc.snapshot().document.sfc_model.as_ref().unwrap();
        let attachment = &model.attribute_attachments[0];
        assert_eq!(attachment.component_ids, vec![b]);
        assert_eq!(
            model.sheet.as_ref().unwrap().component_ids,
            vec![a, attachment.placement_ids[0], c]
        );
        doc.set_attribute(a, "line", ("custom", "1", "test"), attr("label", "other"))
            .unwrap();
        doc.remove_attachment(b).unwrap();
        doc.remove_attachment(a).unwrap();
        assert_eq!(doc.snapshot().document, before);
        assert!(doc.saf.is_none());
    }
    #[test]
    fn attribute_failure_preserves_both_document_and_saf() {
        let mut doc =
            SfcDocument::new("x.sfc", "drawing", 297, 210, "2026-10-06T00:00:00").unwrap();
        let id = line(&mut doc, 0.);
        doc.set_attribute(id, "line", ("custom", "1", "test"), attr("label", "v"))
            .unwrap();
        let before = doc.snapshot().clone();
        let saf = doc.saf.clone();
        assert!(doc
            .set_attribute(id, "line", ("custom", "1", "test"), attr("label", "\0"))
            .is_err());
        assert_eq!(doc.snapshot(), &before);
        assert_eq!(doc.saf, saf);
        assert!(doc
            .set_attribute(id, "line", ("custom", "1", "test"), attr("label", "$$$"))
            .is_err());
        assert_eq!(doc.snapshot(), &before);
        assert_eq!(doc.saf, saf);
    }
}
