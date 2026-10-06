//! SAF authoring and validation of the SXF 3.1 DTD grammar (attribute spec §§3-2..3-4).
//! External DTDs are never fetched or evaluated. Legacy 3.0 inline sets are upgraded.

use crate::writer::WriteError;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafAttribute {
    pub name: String,
    pub value: String,
    pub attribute_type: Option<String>,
    pub unit: Option<String>,
    pub group: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafSet {
    pub id: String,
    pub name: String,
    pub version: String,
    pub designed_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafFigure {
    pub id: String,
    pub name: String,
    pub sets: BTreeMap<String, Vec<SafAttribute>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafDocument {
    pub drawing: String,
    pub date: String,
    pub application: String,
    pub sets: Vec<SafSet>,
    pub figures: Vec<SafFigure>,
}

fn error(message: impl std::fmt::Display) -> WriteError {
    WriteError(message.to_string())
}
fn text(value: &str) -> Result<(), WriteError> {
    if value.chars().any(|c| !matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')) {
        return Err(error("Invalid XML 1.0 character in SAF"));
    }
    Ok(())
}
fn required(node: roxmltree::Node<'_, '_>, name: &str) -> Result<String, WriteError> {
    node.attribute(name)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| error(format!("SAF {} requires {name}", node.tag_name().name())))
}
fn fields(node: roxmltree::Node<'_, '_>, allowed: &[&str]) -> Result<(), WriteError> {
    if node.tag_name().namespace().is_some()
        || node
            .attributes()
            .any(|a| a.namespace().is_some() || !allowed.contains(&a.name()))
    {
        return Err(error(format!(
            "Unsupported SAF fields on {}",
            node.tag_name().name()
        )));
    }
    Ok(())
}
fn container(node: roxmltree::Node<'_, '_>) -> Result<(), WriteError> {
    if node
        .children()
        .any(|n| n.is_text() && !n.text().unwrap_or("").trim().is_empty())
    {
        return Err(error("SAF container has unexpected text"));
    }
    Ok(())
}
fn leaf(node: roxmltree::Node<'_, '_>) -> Result<String, WriteError> {
    if node.children().any(|n| n.is_element()) {
        return Err(error("SAF text element has child elements"));
    }
    Ok(node
        .children()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect())
}

impl SafAttribute {
    pub fn validate(&self) -> Result<(), WriteError> {
        if self.name.is_empty() || self.group.len() > 2 || self.group.iter().any(String::is_empty) {
            return Err(error(
                "SAF requires an attribute name and at most two named group levels",
            ));
        }
        for value in std::iter::once(&self.name)
            .chain(std::iter::once(&self.value))
            .chain(self.group.iter())
            .chain(self.attribute_type.iter())
            .chain(self.unit.iter())
        {
            text(value)?;
        }
        if self
            .attribute_type
            .as_ref()
            .is_some_and(|s| s.len() != 3 || !s.bytes().all(|c| c.is_ascii_uppercase()))
        {
            return Err(error(
                "SAF attribute type must be a three-letter uppercase code",
            ));
        }
        Ok(())
    }
}

impl SafDocument {
    pub fn new(drawing: String, date: String) -> Self {
        Self {
            drawing,
            date,
            application: "ezsxf".into(),
            sets: vec![],
            figures: vec![],
        }
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, WriteError> {
        let (xml, _) = crate::bundle::decode_xml(bytes).map_err(error)?;
        if xml.contains("<!ENTITY")
            || xml.contains("<!DOCTYPE")
                && xml
                    .split("<!DOCTYPE")
                    .nth(1)
                    .unwrap()
                    .split('>')
                    .next()
                    .unwrap_or("")
                    .contains('[')
        {
            return Err(error(
                "Internal SAF DTD/entity declarations are not supported",
            ));
        }
        let document = roxmltree::Document::parse_with_options(
            &xml,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                nodes_limit: 1_000_000,
                entity_resolver: None,
            },
        )
        .map_err(error)?;
        let root = document.root_element();
        // Validate the DOCTYPE spelling; internal entities and remote DTDs are rejected.
        crate::bundle::external_dtd(&xml[..root.range().start]).map_err(error)?;
        if !root.has_tag_name("SxfAttributeXML") {
            return Err(error("Invalid SAF root"));
        }
        fields(root, &["version", "date", "sxfFile", "application"])?;
        container(root)?;
        let version = required(root, "version")?;
        if !matches!(version.as_str(), "3.0" | "3.1") {
            return Err(error("SAF version must be 3.0 or 3.1"));
        }
        let mut result = Self::new(required(root, "sxfFile")?, required(root, "date")?);
        result.application = required(root, "application")?;
        for node in root.children().filter(|n| n.is_element()) {
            if node.has_tag_name("AttributeSet") {
                fields(node, &["name", "version", "designedBy"])?;
                result.sets.push(SafSet {
                    id: leaf(node)?.trim().into(),
                    name: required(node, "name")?,
                    version: required(node, "version")?,
                    designed_by: required(node, "designedBy")?,
                });
            } else if !node.has_tag_name("Figure") {
                return Err(error("Invalid child of SAF root"));
            }
        }
        for node in root.children().filter(|n| n.has_tag_name("Figure")) {
            fields(node, &["id", "name"])?;
            container(node)?;
            let mut figure = SafFigure {
                id: required(node, "id")?,
                name: required(node, "name")?,
                sets: BTreeMap::new(),
            };
            for child in node.children().filter(|n| n.is_element()) {
                let set_id = if child.has_tag_name("AttrSetRef") {
                    fields(child, &["id"])?;
                    required(child, "id")?
                } else if version == "3.0" && child.has_tag_name("AttributeSet") {
                    fields(child, &["name", "version", "designedBy"])?;
                    let name = required(child, "name")?;
                    let version = required(child, "version")?;
                    let designed_by = required(child, "designedBy")?;
                    match result.sets.iter().find(|s| {
                        s.name == name && s.version == version && s.designed_by == designed_by
                    }) {
                        Some(set) => set.id.clone(),
                        None => {
                            let mut next = result.sets.len() + 1;
                            while result.sets.iter().any(|s| s.id == next.to_string()) {
                                next += 1;
                            }
                            let id = next.to_string();
                            result.sets.push(SafSet {
                                id: id.clone(),
                                name,
                                version,
                                designed_by,
                            });
                            id
                        }
                    }
                } else {
                    return Err(error("SAF Figure requires AttrSetRef children (inline sets are only supported for 3.0)"));
                };
                let mut values = Vec::new();
                read_attributes(child, &[], &mut values)?;
                if figure.sets.insert(set_id, values).is_some() {
                    return Err(error("Duplicate SAF AttrSetRef in Figure"));
                }
            }
            result.figures.push(figure);
        }
        result.validate()?;
        Ok(result)
    }
    pub fn validate(&self) -> Result<(), WriteError> {
        for value in [&self.drawing, &self.date, &self.application] {
            text(value)?;
            if value.is_empty() {
                return Err(error("Empty SAF root metadata"));
            }
        }
        let mut set_ids = BTreeSet::new();
        let mut set_metadata = BTreeSet::new();
        for set in &self.sets {
            for value in [&set.id, &set.name, &set.version, &set.designed_by] {
                text(value)?;
                if value.is_empty() {
                    return Err(error("Empty SAF AttributeSet field"));
                }
            }
            if !set_ids.insert(&set.id)
                || !set_metadata.insert((&set.name, &set.version, &set.designed_by))
            {
                return Err(error("Duplicate SAF AttributeSet ID or metadata"));
            }
        }
        let mut figure_ids = BTreeSet::new();
        for figure in &self.figures {
            text(&figure.id)?;
            text(&figure.name)?;
            if figure.id.is_empty() || figure.name.is_empty() || !figure_ids.insert(&figure.id) {
                return Err(error("Empty or duplicate SAF Figure ID/name"));
            }
            if figure.sets.is_empty() {
                return Err(error("SAF Figure requires at least one AttrSetRef"));
            }
            for (id, values) in &figure.sets {
                if !set_ids.contains(id) || values.is_empty() {
                    return Err(error("SAF AttrSetRef is unresolved or empty"));
                }
                for value in values {
                    value.validate()?;
                }
            }
        }
        if self.sets.is_empty() && self.figures.is_empty() {
            return Err(error("SAF root requires AttributeSet or Figure children"));
        }
        Ok(())
    }
    pub fn set(
        &mut self,
        figure_id: &str,
        figure_name: &str,
        metadata: (&str, &str, &str),
        attribute: SafAttribute,
    ) -> Result<(), WriteError> {
        attribute.validate()?;
        let set_id = match self
            .sets
            .iter()
            .find(|s| (s.name.as_str(), s.version.as_str(), s.designed_by.as_str()) == metadata)
        {
            Some(set) => set.id.clone(),
            None => {
                let mut id = 1usize;
                while self.sets.iter().any(|s| s.id == id.to_string()) {
                    id += 1;
                }
                self.sets.push(SafSet {
                    id: id.to_string(),
                    name: metadata.0.into(),
                    version: metadata.1.into(),
                    designed_by: metadata.2.into(),
                });
                id.to_string()
            }
        };
        let index = match self.figures.iter().position(|f| f.id == figure_id) {
            Some(index) => index,
            None => {
                self.figures.push(SafFigure {
                    id: figure_id.into(),
                    name: figure_name.into(),
                    sets: BTreeMap::new(),
                });
                self.figures.len() - 1
            }
        };
        let values = self.figures[index].sets.entry(set_id).or_default();
        let indices: Vec<_> = values
            .iter()
            .enumerate()
            .filter(|(_, a)| a.name == attribute.name && a.group == attribute.group)
            .map(|(i, _)| i)
            .collect();
        match indices.as_slice() {
            [] => values.push(attribute),
            [index] => values[*index] = attribute,
            _ => {
                return Err(error(
                    "Ambiguous repeated SAF attribute; cannot update by name",
                ))
            }
        }
        self.validate()
    }
    pub fn remove(
        &mut self,
        figure_id: &str,
        metadata: (&str, &str, &str),
        name: &str,
        group: &[String],
    ) -> Result<bool, WriteError> {
        let set_id = self
            .sets
            .iter()
            .find(|s| (s.name.as_str(), s.version.as_str(), s.designed_by.as_str()) == metadata)
            .ok_or_else(|| error("Unknown SAF attribute set"))?
            .id
            .clone();
        let index = self
            .figures
            .iter()
            .position(|f| f.id == figure_id)
            .ok_or_else(|| error("Unknown SAF Figure"))?;
        let values = self.figures[index]
            .sets
            .get_mut(&set_id)
            .ok_or_else(|| error("Figure has no such attribute set"))?;
        let matches: Vec<_> = values
            .iter()
            .enumerate()
            .filter(|(_, a)| a.name == name && a.group == group)
            .map(|(i, _)| i)
            .collect();
        if matches.len() != 1 {
            return Err(error("Unknown or ambiguous SAF attribute"));
        }
        values.remove(matches[0]);
        if values.is_empty() {
            self.figures[index].sets.remove(&set_id);
        }
        let empty = self.figures[index].sets.is_empty();
        if empty {
            self.figures.remove(index);
        }
        Ok(empty)
    }
    pub fn dependencies(&self) -> BTreeSet<String> {
        self.figures
            .iter()
            .flat_map(|f| f.sets.values())
            .flatten()
            .filter(|a| matches!(a.name.as_str(), "画像" | "ファイル名"))
            .map(|a| a.value.clone())
            .collect()
    }
    pub fn to_bytes(&self, drawing: &str) -> Result<Vec<u8>, WriteError> {
        self.validate()?;
        text(drawing)?;
        let mut xml = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<SxfAttributeXML version=\"3.1\" date=\"{}\" sxfFile=\"{}\" application=\"{}\">\r\n", escape(&self.date), escape(drawing), escape(&self.application));
        for set in &self.sets {
            writeln!(
                xml,
                "<AttributeSet name=\"{}\" version=\"{}\" designedBy=\"{}\">{}</AttributeSet>\r",
                escape(&set.name),
                escape(&set.version),
                escape(&set.designed_by),
                escape(&set.id)
            )
            .unwrap();
        }
        for figure in &self.figures {
            writeln!(
                xml,
                "<Figure id=\"{}\" name=\"{}\">\r",
                escape(&figure.id),
                escape(&figure.name)
            )
            .unwrap();
            for (id, values) in &figure.sets {
                writeln!(xml, "<AttrSetRef id=\"{}\">\r", escape(id)).unwrap();
                let mut path: Vec<String> = Vec::new();
                for a in values {
                    let same = path
                        .iter()
                        .zip(&a.group)
                        .take_while(|(l, r)| l == r)
                        .count();
                    for _ in same..path.len() {
                        xml.push_str("</AttrGroup>\r\n");
                    }
                    for name in &a.group[same..] {
                        writeln!(xml, "<AttrGroup name=\"{}\">\r", escape(name)).unwrap();
                    }
                    path.clone_from(&a.group);
                    write!(xml, "<Attr name=\"{}\"", escape(&a.name)).unwrap();
                    // These predefined file attributes have type STR even when
                    // omitted (attribute specification table 8, S-02/S-16).
                    // DynaCAD rejects the omission for legacy SAF image bundles.
                    // Preserve unknown omissions rather than guessing their type.
                    let kind = a.attribute_type.as_deref().or_else(|| {
                        matches!(a.name.as_str(), "画像" | "ファイル名").then_some("STR")
                    });
                    if let Some(kind) = kind {
                        write!(xml, " type=\"{}\"", escape(kind)).unwrap();
                    }
                    if let Some(unit) = &a.unit {
                        write!(xml, " unit=\"{}\"", escape(unit)).unwrap();
                    }
                    writeln!(xml, ">{}</Attr>\r", escape(&a.value)).unwrap();
                }
                for _ in path {
                    xml.push_str("</AttrGroup>\r\n");
                }
                xml.push_str("</AttrSetRef>\r\n");
            }
            xml.push_str("</Figure>\r\n");
        }
        xml.push_str("</SxfAttributeXML>\r\n");
        // Independently reparse the emitted grammar before it reaches any file.
        Self::parse(xml.as_bytes())?;
        Ok(xml.into_bytes())
    }
}

fn read_attributes(
    node: roxmltree::Node<'_, '_>,
    group: &[String],
    values: &mut Vec<SafAttribute>,
) -> Result<(), WriteError> {
    container(node)?;
    let mut count = 0;
    for child in node.children().filter(|n| n.is_element()) {
        count += 1;
        if child.has_tag_name("Attr") {
            fields(child, &["name", "type", "unit"])?;
            values.push(SafAttribute {
                name: required(child, "name")?,
                value: leaf(child)?,
                attribute_type: child.attribute("type").map(str::to_owned),
                unit: child.attribute("unit").map(str::to_owned),
                group: group.to_vec(),
            });
        } else if child.has_tag_name("AttrGroup") && group.len() < 2 {
            fields(child, &["name"])?;
            let mut path = group.to_vec();
            path.push(required(child, "name")?);
            read_attributes(child, &path, values)?;
        } else {
            return Err(error(
                "Invalid SAF attribute/group element or group depth exceeds two",
            ));
        }
    }
    if count == 0 {
        return Err(error("Empty SAF AttrSetRef/AttrGroup"));
    }
    Ok(())
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
        .replace('\r', "&#13;")
        .replace('\n', "&#10;")
        .replace('\t', "&#9;")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_file_attributes_emit_their_predefined_type() {
        let xml = r#"<SxfAttributeXML version="3.0" date="2009-2-25" sxfFile="x.sfc" application="legacy"><Figure id="1" name="TIFF"><AttributeSet name="set" version="0" designedBy="SCADEC"><Attr name="画像">x.tif</Attr><Attr name="ファイル名">note.txt</Attr><Attr name="ターゲット">2</Attr><Attr name="等高線">12</Attr><Attr name="custom">opaque</Attr><Attr name="画像" type="URL">explicit</Attr></AttributeSet></Figure></SxfAttributeXML>"#;
        let original = SafDocument::parse(xml.as_bytes()).unwrap();
        let bytes = original.to_bytes("x.sfc").unwrap();
        let written = String::from_utf8(bytes.clone()).unwrap();
        assert!(written.contains("<Attr name=\"画像\" type=\"STR\">x.tif</Attr>"));
        assert!(written.contains("<Attr name=\"ファイル名\" type=\"STR\">note.txt</Attr>"));
        assert!(written.contains("<Attr name=\"画像\" type=\"URL\">explicit</Attr>"));
        let restored = SafDocument::parse(&bytes).unwrap();
        assert_eq!(original.date, restored.date);
        assert_eq!(original.sets, restored.sets);
        assert_eq!(original.dependencies(), restored.dependencies());
        let mut expected = original.figures;
        for values in expected[0].sets.values_mut() {
            for attribute in values {
                if attribute.attribute_type.is_none()
                    && matches!(attribute.name.as_str(), "画像" | "ファイル名")
                {
                    attribute.attribute_type = Some("STR".into());
                }
            }
        }
        assert_eq!(expected, restored.figures);
    }

    #[test]
    fn grammar_round_trip_and_references() {
        let mut doc = SafDocument::new("日本語.sfc".into(), "2026-10-06".into());
        doc.set(
            "10",
            "circle",
            ("custom", "1", "test"),
            SafAttribute {
                name: "label".into(),
                value: "A<&\r\n\tB".into(),
                attribute_type: Some("STR".into()),
                unit: None,
                group: vec!["one".into(), "two".into()],
            },
        )
        .unwrap();
        assert_eq!(
            SafDocument::parse(&doc.to_bytes(&doc.drawing).unwrap()).unwrap(),
            doc
        );
        let bytes = String::from_utf8(doc.to_bytes(&doc.drawing).unwrap()).unwrap();
        assert!(SafDocument::parse(
            bytes
                .replace("<AttrSetRef id=\"1\">", "<AttrSetRef id=\"9\">")
                .as_bytes()
        )
        .is_err());
        assert!(SafDocument::parse(
            bytes
                .replace("</SxfAttributeXML>", "<Unknown/></SxfAttributeXML>")
                .as_bytes()
        )
        .is_err());
        assert!(SafDocument::parse(
            bytes
                .replace("<Figure", "<!ENTITY x 'bad'><Figure")
                .as_bytes()
        )
        .is_err());
    }
}
