//! Parsed-document validation and resolved SFC model construction.

use std::collections::{HashMap, HashSet};

use crate::features::*;
use crate::model::*;
use crate::parser::{
    parse_sfc_attribute_mechanism, resolve_sfc_attribute_file_name, sfc_attribute_figure_id, Parser,
};

impl<'a> Parser<'a> {
    pub(crate) fn validate_document(
        &mut self,
        document: &ParsedDocument,
    ) -> Result<(), ParseError> {
        self.validate_required_header(document)?;
        self.validate_file_schema(document)?;
        if document.format == FileFormat::Sfc {
            self.validate_sfc_header(document)?;
        }
        self.validate_duplicate_ids(document)?;
        self.validate_references(document)?;
        if document.format == FileFormat::Sfc {
            self.validate_sfc_model_rules(document)?;
        }
        Ok(())
    }

    pub(crate) fn validate_required_header(
        &mut self,
        document: &ParsedDocument,
    ) -> Result<(), ParseError> {
        for required in ["FILE_DESCRIPTION", "FILE_NAME", "FILE_SCHEMA"] {
            let count = document
                .header
                .entities
                .iter()
                .filter(|record| record.keyword.eq_ignore_ascii_case(required))
                .count();
            if count == 0 {
                self.issue_or_error(
                    "missing-header-entity",
                    format!("Required header entity {required} is missing"),
                )?;
            } else if count > 1 {
                self.issue_or_error(
                    "duplicate-header-entity",
                    format!("Header entity {required} appears {count} times"),
                )?;
            }
        }
        Ok(())
    }

    pub(crate) fn validate_file_schema(
        &mut self,
        document: &ParsedDocument,
    ) -> Result<(), ParseError> {
        let Some(record) = document.header.find_keyword("FILE_SCHEMA") else {
            return Ok(());
        };

        let mut found_schema = false;
        if let Some(Value::List(items)) = record.parameters.first() {
            for item in items {
                if let Value::String(value) = item {
                    if value.eq_ignore_ascii_case("ASSOCIATIVE_DRAUGHTING") {
                        found_schema = true;
                        break;
                    }
                }
            }
        }

        if !found_schema {
            self.issue_or_error(
                "unexpected-schema",
                "FILE_SCHEMA should include ASSOCIATIVE_DRAUGHTING".to_string(),
            )?;
        }
        Ok(())
    }

    pub(crate) fn validate_sfc_header(
        &mut self,
        document: &ParsedDocument,
    ) -> Result<(), ParseError> {
        let Some(description) = document.header.find_keyword("FILE_DESCRIPTION") else {
            return Ok(());
        };
        let description_valid = matches!(
            description.parameters.as_slice(),
            [Value::List(items), Value::String(level)]
                if items.len() == 1
                    && matches!(
                        &items[0],
                        Value::String(mode)
                            if mode.eq_ignore_ascii_case("SCADEC level2 feature_mode")
                    )
                    && level == "2;1"
        );
        if !description_valid {
            self.issue_or_error(
                "sfc-file-description",
                "SFC FILE_DESCRIPTION must be (('SCADEC level2 feature_mode'),'2;1')".to_string(),
            )?;
        }

        let Some(file_name) = document.header.find_keyword("FILE_NAME") else {
            return Ok(());
        };
        let file_name_shape_valid = matches!(
            file_name.parameters.as_slice(),
            [
                Value::String(_),
                Value::String(_),
                Value::List(authors),
                Value::List(organizations),
                Value::String(_),
                Value::String(_),
                Value::String(_)
            ] if authors.iter().all(|value| matches!(value, Value::String(_)))
                && organizations
                    .iter()
                    .all(|value| matches!(value, Value::String(_)))
        );
        if !file_name_shape_valid {
            self.issue_or_error(
                "sfc-file-name",
                "SFC FILE_NAME must contain name, timestamp, author list, organization list, preprocessor, system and authorization"
                    .to_string(),
            )?;
            return Ok(());
        }

        let Value::String(preprocessor_version) = &file_name.parameters[4] else {
            return Ok(());
        };
        let declared_version = preprocessor_version
            .rsplit_once("$$")
            .map(|(_, version)| version.trim());
        let declared_rank = match declared_version {
            Some("2.0") | Some("2") => Some(20),
            Some("3.0") | Some("3") => Some(30),
            Some("3.1") => Some(31),
            _ => {
                self.issue_or_error(
                    "sfc-version-header",
                    format!(
                        "SFC FILE_NAME preprocessor version {:?} must end in $$2.0, $$3.0 or $$3.1",
                        preprocessor_version
                    ),
                )?;
                None
            }
        };

        if let Some(declared_rank) = declared_rank {
            for entity in &document.entities {
                let Some(tag) = entity.sfc_version else {
                    continue;
                };
                let required_rank = match tag {
                    SfcVersionTag::V2 => 20,
                    SfcVersionTag::V3 => 30,
                    SfcVersionTag::V31 => 31,
                };
                if required_rank > declared_rank {
                    self.issue_or_error(
                        "sfc-version-header",
                        format!(
                            "Entity #{} uses {} but FILE_NAME declares {:?}",
                            entity.id,
                            tag.marker(),
                            declared_version.unwrap_or_default()
                        ),
                    )?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn validate_duplicate_ids(
        &mut self,
        document: &ParsedDocument,
    ) -> Result<(), ParseError> {
        let mut ids = HashSet::new();
        for entity in &document.entities {
            if !ids.insert(entity.id) {
                self.issue_or_error(
                    "duplicate-entity-id",
                    format!("Entity id #{} is duplicated", entity.id),
                )?;
            }
        }
        Ok(())
    }

    pub(crate) fn validate_references(
        &mut self,
        document: &ParsedDocument,
    ) -> Result<(), ParseError> {
        let ids: HashSet<i64> = document.entities.iter().map(|entity| entity.id).collect();
        for entity in &document.entities {
            for reference in entity.collect_references() {
                if !ids.contains(&reference) {
                    self.issue_or_error(
                        "undefined-reference",
                        format!(
                            "Entity #{} references undefined entity #{}",
                            entity.id, reference
                        ),
                    )?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn build_sfc_model(
        &mut self,
        document: &ParsedDocument,
    ) -> Result<SfcModel, ParseError> {
        let mut model = SfcModel {
            code_tables: self.build_sfc_code_tables(document)?,
            ..Default::default()
        };
        let typed_by_id: HashMap<i64, &TypedFeature> = document
            .typed_features
            .iter()
            .map(|item| (item.id, &item.feature))
            .collect();
        let mut component_buffer = Vec::new();
        let mut seen_sheet = false;
        // The optional index identifies attribute-attachment definitions, which use the
        // same SFC feature pair but are explicitly not ordinary drawing groups.
        let mut sfig_by_name = HashMap::<String, (i64, Option<usize>)>::new();

        for item in &document.typed_features {
            match &item.feature {
                TypedFeature::SfigOrg(feature) => {
                    if seen_sheet {
                        continue;
                    }
                    if sfig_by_name.contains_key(&feature.name) {
                        self.issue_or_error(
                            "sfc-duplicate-sfig-name",
                            format!(
                                "Entity #{} repeats composite figure name {:?}",
                                item.id, feature.name
                            ),
                        )?;
                        component_buffer.clear();
                        continue;
                    }
                    let attribute = match parse_sfc_attribute_mechanism(&feature.name) {
                        Ok(value) => value,
                        Err(message) => {
                            self.issue_or_error(
                                "sfc-attribute-name",
                                format!("Composite figure #{}: {message}", item.id),
                            )?;
                            None
                        }
                    };
                    let component_ids = std::mem::take(&mut component_buffer);
                    if let Some(mechanism) = attribute {
                        if feature.kind_flag != 3 {
                            self.issue_or_error(
                                "sfc-attribute-kind",
                                format!(
                                    "Attribute attachment #{} must use drawing-group kind flag 3",
                                    item.id
                                ),
                            )?;
                        }
                        let attachment_index = model.attribute_attachments.len();
                        let resolved_attribute_file_name =
                            resolve_sfc_attribute_file_name(document, &mechanism);
                        model.attribute_attachments.push(SfcAttributeAttachment {
                            definition_id: item.id,
                            name: feature.name.clone(),
                            kind_flag: feature.kind_flag,
                            component_ids,
                            placement_ids: Vec::new(),
                            mechanism,
                            resolved_attribute_file_name,
                        });
                        sfig_by_name
                            .insert(feature.name.clone(), (item.id, Some(attachment_index)));
                    } else {
                        model.sfig_definitions.push(SfcSfigDefinition {
                            entity_id: item.id,
                            name: feature.name.clone(),
                            kind_flag: feature.kind_flag,
                            component_ids,
                        });
                        sfig_by_name.insert(feature.name.clone(), (item.id, None));
                    }
                }
                TypedFeature::CompositeCurve(_) => {
                    if seen_sheet {
                        continue;
                    }
                    let mut valid = true;
                    for component_id in &component_buffer {
                        let Some(component) = typed_by_id.get(component_id) else {
                            continue;
                        };
                        if !is_composite_curve_component(component) {
                            self.issue_or_error(
                                "sfc-composite-curve-component",
                                format!(
                                    "Composite curve #{} contains unsupported component #{}",
                                    item.id, component_id
                                ),
                            )?;
                            valid = false;
                        }
                    }
                    let component_ids = std::mem::take(&mut component_buffer);
                    if valid {
                        let code = i64::try_from(model.composite_curve_definitions.len() + 1)
                            .unwrap_or(i64::MAX);
                        model
                            .composite_curve_definitions
                            .push(SfcCompositeCurveDefinition {
                                code,
                                entity_id: item.id,
                                component_ids,
                            });
                    }
                }
                TypedFeature::DrawingSheet(_) => {
                    if model.sheet.is_some() {
                        self.issue_or_error(
                            "sfc-duplicate-drawing-sheet",
                            format!("SFC contains more than one drawing sheet (#{})", item.id),
                        )?;
                        component_buffer.clear();
                        continue;
                    }
                    model.sheet = Some(SfcSheetModel {
                        entity_id: item.id,
                        component_ids: std::mem::take(&mut component_buffer),
                    });
                    seen_sheet = true;
                }
                feature if feature.requires_pre_sheet_order() => {
                    if seen_sheet {
                        continue;
                    }

                    if let TypedFeature::SfigLocate(placement) = feature {
                        let Some((definition_id, attachment_index)) =
                            sfig_by_name.get(&placement.name).copied()
                        else {
                            self.issue_or_error(
                                "sfc-sfig-reference",
                                format!(
                                    "Composite figure placement #{} references undefined or later definition {:?}",
                                    item.id, placement.name
                                ),
                            )?;
                            continue;
                        };
                        if let Some(attachment_index) = attachment_index {
                            model.attribute_attachments[attachment_index]
                                .placement_ids
                                .push(item.id);
                        } else {
                            model.sfig_references.push(SfcSfigReference {
                                placement_id: item.id,
                                definition_id,
                            });
                        }
                    }

                    if let Some((out_id, in_ids)) = hatch_composite_curve_codes(feature) {
                        let Some(outer) = resolve_composite_curve_definition(
                            &model.composite_curve_definitions,
                            out_id,
                        ) else {
                            self.issue_or_error(
                                "sfc-hatch-reference",
                                format!(
                                    "Hatch #{} references undefined or later outer composite curve code {}",
                                    item.id, out_id
                                ),
                            )?;
                            continue;
                        };
                        let mut inner_definition_ids = Vec::with_capacity(in_ids.len());
                        let mut all_inner_resolved = true;
                        for code in in_ids {
                            if let Some(definition) = resolve_composite_curve_definition(
                                &model.composite_curve_definitions,
                                *code,
                            ) {
                                inner_definition_ids.push(definition.entity_id);
                            } else {
                                self.issue_or_error(
                                    "sfc-hatch-reference",
                                    format!(
                                        "Hatch #{} references undefined or later inner composite curve code {}",
                                        item.id, code
                                    ),
                                )?;
                                all_inner_resolved = false;
                            }
                        }
                        if !all_inner_resolved {
                            continue;
                        }
                        model.hatch_references.push(SfcHatchReference {
                            hatch_id: item.id,
                            outer_definition_id: outer.entity_id,
                            inner_definition_ids,
                        });
                    }

                    component_buffer.push(item.id);
                }
                _ => {}
            }
        }

        let mut attribute_figure_ids = HashMap::<&str, i64>::new();
        for attachment in &model.attribute_attachments {
            if attachment.component_ids.len() != 1 {
                self.issue_or_error(
                    "sfc-attribute-component-count",
                    format!(
                        "Attribute attachment #{} must contain exactly one target feature, got {}",
                        attachment.definition_id,
                        attachment.component_ids.len()
                    ),
                )?;
            }
            if attachment.placement_ids.len() != 1 {
                self.issue_or_error(
                    "sfc-attribute-placement-count",
                    format!(
                        "Attribute attachment #{} must have exactly one paired placement, got {}",
                        attachment.definition_id,
                        attachment.placement_ids.len()
                    ),
                )?;
            }

            let figure_id = sfc_attribute_figure_id(&attachment.mechanism);
            if let Some(previous_id) =
                attribute_figure_ids.insert(figure_id, attachment.definition_id)
            {
                self.issue_or_error(
                    "sfc-duplicate-attribute-figure-id",
                    format!(
                        "Attribute attachment #{} repeats figure identifier {:?} from #{}",
                        attachment.definition_id, figure_id, previous_id
                    ),
                )?;
            }

            if matches!(
                attachment.mechanism,
                SfcAttributeMechanism::TextAttribute { .. }
            ) && !matches!(
                attachment
                    .component_ids
                    .first()
                    .and_then(|id| typed_by_id.get(id).copied()),
                Some(TypedFeature::Text(_))
            ) {
                self.issue_or_error(
                    "sfc-atrs-target",
                    format!(
                        "ATRS attribute attachment #{} must target one text feature",
                        attachment.definition_id
                    ),
                )?;
            }

            if let Some(target_id) = attachment.component_ids.first() {
                if matches!(
                    typed_by_id.get(target_id).copied(),
                    Some(TypedFeature::SfigLocate(_))
                ) {
                    let referenced_kind = model
                        .sfig_references
                        .iter()
                        .find(|reference| reference.placement_id == *target_id)
                        .and_then(|reference| {
                            model
                                .sfig_definitions
                                .iter()
                                .find(|definition| definition.entity_id == reference.definition_id)
                                .map(|definition| definition.kind_flag)
                        });
                    let targets_another_attribute =
                        model.attribute_attachments.iter().any(|candidate| {
                            candidate.definition_id != attachment.definition_id
                                && candidate.placement_ids.contains(target_id)
                        });
                    if matches!(referenced_kind, Some(1 | 2)) || targets_another_attribute {
                        self.issue_or_error(
                            "sfc-attribute-target",
                            format!(
                                "Attribute attachment #{} cannot target a partial drawing or another attribute wrapper",
                                attachment.definition_id
                            ),
                        )?;
                    }
                }
            }

            for placement_id in &attachment.placement_ids {
                let Some(TypedFeature::SfigLocate(placement)) =
                    typed_by_id.get(placement_id).copied()
                else {
                    continue;
                };
                if placement.position.x != 0.0
                    || placement.position.y != 0.0
                    || placement.angle_deg != 0.0
                    || placement.ratio_x != 1.0
                    || placement.ratio_y != 1.0
                {
                    self.issue_or_error(
                        "sfc-attribute-placement-transform",
                        format!(
                            "Attribute placement #{} must use position (0,0), angle 0 and scale (1,1)",
                            placement_id
                        ),
                    )?;
                }
            }
        }

        // Container codes: -1 = sheet, 0 = transparent attribute wrapper,
        // 1..=4 = the composite-figure kind flag of the containing definition.
        let mut component_owner = HashMap::<i64, i64>::new();
        if let Some(sheet) = &model.sheet {
            for component_id in &sheet.component_ids {
                component_owner.insert(*component_id, -1);
            }
        }
        for definition in &model.sfig_definitions {
            for component_id in &definition.component_ids {
                component_owner.insert(*component_id, definition.kind_flag);
            }
        }
        for attachment in &model.attribute_attachments {
            for component_id in &attachment.component_ids {
                component_owner.insert(*component_id, 0);
            }
        }

        for definition in &model.sfig_definitions {
            let placements = model
                .sfig_references
                .iter()
                .filter(|reference| reference.definition_id == definition.entity_id)
                .collect::<Vec<_>>();
            if placements.is_empty() {
                self.issue_or_error(
                    "sfc-unused-sfig-definition",
                    format!(
                        "Composite figure definition #{} is not used by a placement",
                        definition.entity_id
                    ),
                )?;
            }
            if definition.kind_flag != 4 && placements.len() > 1 {
                self.issue_or_error(
                    "sfc-sfig-placement-count",
                    format!(
                        "Composite figure definition #{} of kind {} may be placed only once, got {} placements",
                        definition.entity_id,
                        definition.kind_flag,
                        placements.len()
                    ),
                )?;
            }

            for reference in placements {
                let owner_kind = component_owner.get(&reference.placement_id).copied();
                let valid_parent = match definition.kind_flag {
                    // Mathematical/geodetic partial drawings can only be placed on the sheet.
                    1 | 2 => owner_kind == Some(-1),
                    // Drawing groups can be placed on the sheet, in a partial drawing or in
                    // another drawing group. Attribute wrappers are semantically transparent.
                    3 => matches!(owner_kind, Some(-1..=3)),
                    // Drawing parts can be placed in every SFC drawing container.
                    4 => matches!(owner_kind, Some(-1..=4)),
                    _ => false,
                };
                if !valid_parent {
                    self.issue_or_error(
                        "sfc-sfig-hierarchy",
                        format!(
                            "Placement #{} of composite figure #{} kind {} has an invalid parent container",
                            reference.placement_id, definition.entity_id, definition.kind_flag
                        ),
                    )?;
                }

                if definition.kind_flag == 3 {
                    let Some(TypedFeature::SfigLocate(placement)) =
                        typed_by_id.get(&reference.placement_id).copied()
                    else {
                        continue;
                    };
                    if placement.position.x != 0.0
                        || placement.position.y != 0.0
                        || placement.angle_deg != 0.0
                        || placement.ratio_x != 1.0
                        || placement.ratio_y != 1.0
                    {
                        self.issue_or_error(
                            "sfc-drawing-group-transform",
                            format!(
                                "Drawing-group placement #{} must use position (0,0), angle 0 and scale (1,1)",
                                reference.placement_id
                            ),
                        )?;
                    }
                }
            }
        }

        Ok(model)
    }

    pub(crate) fn build_sfc_code_tables(
        &mut self,
        document: &ParsedDocument,
    ) -> Result<SfcCodeTables, ParseError> {
        let mut tables = SfcCodeTables::default();
        let mut next_layer = 1_i64;
        let mut next_user_line_type = 17_i64;
        let mut next_user_color = 17_i64;
        let mut next_user_width = 11_i64;
        let mut next_text_font = 1_i64;

        for item in &document.typed_features {
            let binding = match &item.feature {
                TypedFeature::Layer(_) => {
                    let code = next_layer;
                    next_layer += 1;
                    Some((&mut tables.layers, code, 256, "layer"))
                }
                TypedFeature::PreDefinedFont(feature) => {
                    let Some(code) = predefined_line_type_code(&feature.name) else {
                        self.issue_or_error(
                            "sfc-predefined-line-type",
                            format!(
                                "Entity #{} has unknown predefined line type {:?}",
                                item.id, feature.name
                            ),
                        )?;
                        continue;
                    };
                    Some((&mut tables.line_types, code, 15, "line type"))
                }
                TypedFeature::UserDefinedFont(_) => {
                    let code = next_user_line_type;
                    next_user_line_type += 1;
                    Some((&mut tables.line_types, code, 32, "line type"))
                }
                TypedFeature::PreDefinedColour(feature) => {
                    let Some(code) = predefined_color_code(&feature.name) else {
                        self.issue_or_error(
                            "sfc-predefined-color",
                            format!(
                                "Entity #{} has unknown predefined color {:?}",
                                item.id, feature.name
                            ),
                        )?;
                        continue;
                    };
                    Some((&mut tables.colors, code, 16, "color"))
                }
                TypedFeature::UserDefinedColour(_) => {
                    let code = next_user_color;
                    next_user_color += 1;
                    Some((&mut tables.colors, code, 256, "color"))
                }
                TypedFeature::Width(feature) => {
                    let code = predefined_line_width_code(feature.width).unwrap_or_else(|| {
                        let code = next_user_width;
                        next_user_width += 1;
                        code
                    });
                    Some((&mut tables.line_widths, code, 16, "line width"))
                }
                TypedFeature::TextFont(_) => {
                    let code = next_text_font;
                    next_text_font += 1;
                    Some((&mut tables.text_fonts, code, 1024, "text font"))
                }
                _ => None,
            };

            if let Some((bindings, code, max, label)) = binding {
                if code > max {
                    self.issue_or_error(
                        "sfc-code-table-overflow",
                        format!(
                            "Entity #{} exceeds the maximum {} code {}",
                            item.id, label, max
                        ),
                    )?;
                    continue;
                }
                if bindings.iter().any(|entry| entry.code == code) {
                    self.issue_or_error(
                        "sfc-duplicate-code-definition",
                        format!("Entity #{} duplicates {} code {}", item.id, label, code),
                    )?;
                    continue;
                }
                bindings.push(SfcCodeBinding {
                    code,
                    entity_id: item.id,
                });
            }
        }

        Ok(tables)
    }

    pub(crate) fn validate_sfc_model_rules(
        &mut self,
        document: &ParsedDocument,
    ) -> Result<(), ParseError> {
        let entity_order: Vec<i64> = document.entities.iter().map(|entity| entity.id).collect();
        let mut entity_index_map = std::collections::HashMap::new();
        for (index, id) in entity_order.iter().enumerate() {
            entity_index_map.insert(*id, index);
        }

        let mut sheet_index: Option<usize> = None;
        let mut layer_code_to_index = std::collections::HashMap::new();
        let mut line_type_code_to_index = std::collections::HashMap::new();
        let mut color_code_to_index = std::collections::HashMap::new();
        let mut line_width_code_to_index = std::collections::HashMap::new();
        let mut text_font_code_to_index = std::collections::HashMap::new();

        let mut next_layer_code = 1_i64;
        let mut next_line_type_code = 17_i64;
        let mut next_color_code = 17_i64;
        let mut next_line_width_code = 11_i64;
        let mut next_text_font_code = 1_i64;
        for typed in &document.typed_features {
            match &typed.feature {
                TypedFeature::DrawingSheet(_) => {
                    let idx = *entity_index_map.get(&typed.id).unwrap_or(&usize::MAX);
                    sheet_index = Some(sheet_index.map_or(idx, |current| current.min(idx)));
                }
                TypedFeature::Layer(_) => {
                    let idx = *entity_index_map.get(&typed.id).unwrap_or(&usize::MAX);
                    layer_code_to_index.insert(next_layer_code, idx);
                    next_layer_code += 1;
                }
                TypedFeature::PreDefinedFont(feature) => {
                    if let Some(code) = predefined_line_type_code(&feature.name) {
                        let idx = *entity_index_map.get(&typed.id).unwrap_or(&usize::MAX);
                        line_type_code_to_index.insert(code, idx);
                    }
                }
                TypedFeature::UserDefinedFont(_) => {
                    let idx = *entity_index_map.get(&typed.id).unwrap_or(&usize::MAX);
                    line_type_code_to_index.insert(next_line_type_code, idx);
                    next_line_type_code += 1;
                }
                TypedFeature::PreDefinedColour(feature) => {
                    if let Some(code) = predefined_color_code(&feature.name) {
                        let idx = *entity_index_map.get(&typed.id).unwrap_or(&usize::MAX);
                        color_code_to_index.insert(code, idx);
                    }
                }
                TypedFeature::UserDefinedColour(_) => {
                    let idx = *entity_index_map.get(&typed.id).unwrap_or(&usize::MAX);
                    color_code_to_index.insert(next_color_code, idx);
                    next_color_code += 1;
                }
                TypedFeature::Width(feature) => {
                    let idx = *entity_index_map.get(&typed.id).unwrap_or(&usize::MAX);
                    if let Some(code) = predefined_line_width_code(feature.width) {
                        line_width_code_to_index.insert(code, idx);
                    } else {
                        line_width_code_to_index.insert(next_line_width_code, idx);
                        next_line_width_code += 1;
                    }
                }
                TypedFeature::TextFont(_) => {
                    let idx = *entity_index_map.get(&typed.id).unwrap_or(&usize::MAX);
                    text_font_code_to_index.insert(next_text_font_code, idx);
                    next_text_font_code += 1;
                }
                _ => {}
            }
        }

        if sheet_index.is_none() {
            self.issue_or_error(
                "sfc-missing-drawing-sheet",
                "SFC should contain at least one drawing_sheet_feature".to_string(),
            )?;
            return Ok(());
        }
        let sheet_index = sheet_index.unwrap_or(usize::MAX);

        for typed in &document.typed_features {
            let Some(idx) = entity_index_map.get(&typed.id).copied() else {
                continue;
            };
            if idx > sheet_index && typed.feature.requires_pre_sheet_order() {
                self.issue_or_error(
                    "sfc-order",
                    format!(
                        "Feature {} (#{}) should be defined before drawing_sheet_feature",
                        typed.keyword, typed.id
                    ),
                )?;
            }
        }

        let mut missing_code_once = HashSet::new();
        for typed in &document.typed_features {
            let Some(style) = typed.feature.style() else {
                continue;
            };
            let use_index = *entity_index_map.get(&typed.id).unwrap_or(&usize::MAX);

            let mut check_code = |code: Option<i64>,
                                  map: &std::collections::HashMap<i64, usize>,
                                  common_max_code: Option<i64>,
                                  require_prior_definition: bool,
                                  code_label: &str,
                                  warning_code: &str|
             -> Result<(), ParseError> {
                if map.is_empty() && common_max_code.is_none() {
                    return Ok(());
                }
                let Some(code) = code else {
                    return Ok(());
                };
                // `0` is treated as an implementation/default code in many real-world datasets.
                if code <= 0 {
                    return Ok(());
                }
                if let Some(definition_index) = map.get(&code) {
                    if !require_prior_definition || *definition_index < use_index {
                        return Ok(());
                    }
                    return self.issue_or_error(
                        "sfc-definition-order",
                        format!(
                            "Feature {} (#{}) references {} {} before its definition",
                            typed.keyword, typed.id, code_label, code
                        ),
                    );
                }
                if common_max_code.is_some_and(|max| code <= max) {
                    return Ok(());
                }
                if !missing_code_once.insert((warning_code.to_string(), code)) {
                    return Ok(());
                }
                self.issue_or_error(
                    warning_code,
                    format!(
                        "Feature {} (#{}) references undefined {} {}",
                        typed.keyword, typed.id, code_label, code
                    ),
                )
            };

            check_code(
                style.layer_code,
                &layer_code_to_index,
                None,
                false,
                "layer code",
                "sfc-layer-reference",
            )?;
            check_code(
                style.line_type_code,
                &line_type_code_to_index,
                Some(COMMON_PREDEFINED_LINE_TYPE_MAX_CODE),
                true,
                "line type code",
                "sfc-line-type-reference",
            )?;
            check_code(
                match &typed.feature {
                    TypedFeature::ExternallyDefinedSymbol(feature) if feature.color_flag == 0 => {
                        None
                    }
                    _ => style.color_code,
                },
                &color_code_to_index,
                Some(COMMON_PREDEFINED_COLOR_MAX_CODE),
                true,
                "color code",
                "sfc-color-reference",
            )?;
            check_code(
                style.line_width_code,
                &line_width_code_to_index,
                Some(COMMON_PREDEFINED_LINE_WIDTH_MAX_CODE),
                true,
                "line width code",
                "sfc-line-width-reference",
            )?;
            check_code(
                style.font_code,
                &text_font_code_to_index,
                None,
                true,
                "text font code",
                "sfc-text-font-reference",
            )?;
        }

        for typed in &document.typed_features {
            let use_index = *entity_index_map.get(&typed.id).unwrap_or(&usize::MAX);
            for (kind, code) in additional_sfc_code_references(&typed.feature) {
                if code <= 0 {
                    continue;
                }
                let (map, common_max, label, warning_code) = match kind {
                    SfcCodeKind::Color => (
                        &color_code_to_index,
                        Some(COMMON_PREDEFINED_COLOR_MAX_CODE),
                        "color code",
                        "sfc-color-reference",
                    ),
                    SfcCodeKind::LineType => (
                        &line_type_code_to_index,
                        Some(COMMON_PREDEFINED_LINE_TYPE_MAX_CODE),
                        "line type code",
                        "sfc-line-type-reference",
                    ),
                    SfcCodeKind::LineWidth => (
                        &line_width_code_to_index,
                        Some(COMMON_PREDEFINED_LINE_WIDTH_MAX_CODE),
                        "line width code",
                        "sfc-line-width-reference",
                    ),
                    SfcCodeKind::TextFont => (
                        &text_font_code_to_index,
                        None,
                        "text font code",
                        "sfc-text-font-reference",
                    ),
                };
                if let Some(definition_index) = map.get(&code) {
                    if *definition_index < use_index {
                        continue;
                    }
                    self.issue_or_error(
                        "sfc-definition-order",
                        format!(
                            "Feature {} (#{}) references {} {} before its definition",
                            typed.keyword, typed.id, label, code
                        ),
                    )?;
                    continue;
                }
                if common_max.is_some_and(|max| code <= max) {
                    continue;
                }
                if missing_code_once.insert((warning_code.to_string(), code)) {
                    self.issue_or_error(
                        warning_code,
                        format!(
                            "Feature {} (#{}) references undefined {} {}",
                            typed.keyword, typed.id, label, code
                        ),
                    )?;
                }
            }
        }

        Ok(())
    }

    pub(crate) fn extract_typed_features(
        &mut self,
        entities: &[EntityInstance],
    ) -> Result<Vec<TypedFeatureInstance>, ParseError> {
        let mut typed = Vec::new();
        for entity in entities {
            let EntityBody::Simple(record) = &entity.body else {
                continue;
            };

            if let Some(expected) = required_sfc_version(&record.keyword) {
                if entity.sfc_version != Some(expected) {
                    self.issue_or_error(
                        "sfc-version-tag",
                        format!(
                            "Entity #{} ({}) must use /*{} ... {}*/",
                            entity.id,
                            record.keyword,
                            expected.marker(),
                            expected.marker()
                        ),
                    )?;
                    continue;
                }
            }

            let Some(feature) = self.parse_typed_feature(record) else {
                if self.format == FileFormat::Sfc {
                    self.push_warning(
                        "sfc-unsupported-feature-skipped",
                        format!(
                            "Feature {} (#{}) is not implemented and was skipped",
                            record.keyword, entity.id
                        ),
                    );
                }
                continue;
            };

            match feature {
                Ok(feature) => match validate_typed_feature_values(&feature) {
                    Ok(()) => typed.push(TypedFeatureInstance {
                        id: entity.id,
                        keyword: record.keyword.clone(),
                        feature,
                    }),
                    Err(message) => self.push_warning(
                        "sfc-invalid-feature-skipped",
                        format!("Entity #{} ({}) {message}", entity.id, record.keyword),
                    ),
                },
                Err(message) => {
                    if self.format == FileFormat::Sfc {
                        self.push_warning(
                            "sfc-incomplete-feature-skipped",
                            format!("Entity #{} ({}) {message}", entity.id, record.keyword),
                        );
                    } else {
                        self.issue_or_error(
                            "typed-feature-parse",
                            format!("Entity #{} ({}) {message}", entity.id, record.keyword),
                        )?;
                    }
                }
            }
        }
        Ok(typed)
    }

    pub(crate) fn parse_typed_feature(
        &self,
        record: &Record,
    ) -> Option<Result<TypedFeature, String>> {
        let keyword = record.keyword.to_ascii_lowercase();
        let params = &record.parameters;

        match keyword.as_str() {
            "drawing_attribute_feature" => {
                Some(parse_drawing_attribute_feature(params).map(TypedFeature::DrawingAttribute))
            }
            "drawing_sheet_feature" => {
                Some(parse_drawing_sheet_feature(params).map(TypedFeature::DrawingSheet))
            }
            "layer_feature" => Some(parse_layer_feature(params).map(TypedFeature::Layer)),
            "pre_defined_font_feature" => {
                Some(parse_pre_defined_font_feature(params).map(TypedFeature::PreDefinedFont))
            }
            "user_defined_font_feature" => {
                Some(parse_user_defined_font_feature(params).map(TypedFeature::UserDefinedFont))
            }
            "pre_defined_colour_feature" => {
                Some(parse_pre_defined_colour_feature(params).map(TypedFeature::PreDefinedColour))
            }
            "user_defined_colour_feature" => {
                Some(parse_user_defined_colour_feature(params).map(TypedFeature::UserDefinedColour))
            }
            "width_feature" => Some(parse_width_feature(params).map(TypedFeature::Width)),
            "text_font_feature" => {
                Some(parse_text_font_feature(params).map(TypedFeature::TextFont))
            }
            "sfig_org_feature" => Some(parse_sfig_org_feature(params).map(TypedFeature::SfigOrg)),
            "sfig_locate_feature" => {
                Some(parse_sfig_locate_feature(params).map(TypedFeature::SfigLocate))
            }
            "symbol_externally_defined_feature" | "externally_defined_symbol_feature" => Some(
                parse_externally_defined_symbol_feature(params)
                    .map(TypedFeature::ExternallyDefinedSymbol),
            ),
            "linear_dim_feature" => {
                Some(parse_linear_dim_feature(params).map(TypedFeature::LinearDim))
            }
            "curve_dim_feature" => {
                Some(parse_curve_dim_feature(params).map(TypedFeature::CurveDim))
            }
            "angular_dim_feature" => {
                Some(parse_angular_dim_feature(params).map(TypedFeature::AngularDim))
            }
            "radius_dim_feature" => {
                Some(parse_radius_dim_feature(params).map(TypedFeature::RadiusDim))
            }
            "diameter_dim_feature" => {
                Some(parse_diameter_dim_feature(params).map(TypedFeature::DiameterDim))
            }
            "label_feature" => Some(parse_label_feature(params).map(TypedFeature::Label)),
            "balloon_feature" => Some(parse_balloon_feature(params).map(TypedFeature::Balloon)),
            "externally_defined_hatch_feature" => Some(
                parse_externally_defined_hatch_feature(params)
                    .map(TypedFeature::ExternallyDefinedHatch),
            ),
            "fill_area_style_colour_feature" => Some(
                parse_fill_area_style_colour_feature(params).map(TypedFeature::FillAreaStyleColour),
            ),
            "fill_area_style_hatching_feature" => Some(
                parse_fill_area_style_hatching_feature(params)
                    .map(TypedFeature::FillAreaStyleHatching),
            ),
            "fill_area_style_tiles_hatching_feature" | "fill_area_style_tiles_feature" => Some(
                parse_fill_area_style_tiles_feature(params).map(TypedFeature::FillAreaStyleTiles),
            ),
            "point_marker_feature" => {
                Some(parse_point_marker_feature(params).map(TypedFeature::PointMarker))
            }
            "line_feature" => Some(parse_line_feature(params).map(TypedFeature::Line)),
            "polyline_feature" => Some(parse_polyline_feature(params).map(TypedFeature::Polyline)),
            "circle_feature" => Some(parse_circle_feature(params).map(TypedFeature::Circle)),
            "arc_feature" => Some(parse_arc_feature(params).map(TypedFeature::Arc)),
            "ellipse_feature" => Some(parse_ellipse_feature(params).map(TypedFeature::Ellipse)),
            "ellipse_arc_feature" => {
                Some(parse_ellipse_arc_feature(params).map(TypedFeature::EllipseArc))
            }
            "text_string_feature" => Some(parse_text_feature(params).map(TypedFeature::Text)),
            "spline_feature" => Some(parse_spline_feature(params).map(TypedFeature::Spline)),
            "clothoid_feature" => Some(parse_clothoid_feature(params).map(TypedFeature::Clothoid)),
            "composite_curve_feature" | "composite_curve_org_feature" => {
                Some(parse_composite_curve_feature(params).map(TypedFeature::CompositeCurve))
            }
            _ => None,
        }
    }
}
