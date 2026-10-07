//! Named-field authoring and structural editing, preserving SFC ownership/order.
use crate::editor::{instance, number, numeric_text, record, string, SfcDocument};
use crate::model::*;
use crate::writer::WriteError;
use std::collections::{BTreeMap, BTreeSet};

fn error(message: impl Into<String>) -> WriteError {
    WriteError(message.into())
}

pub(crate) fn feature_keyword(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "point_marker" | "point_marker_feature" => "point_marker_feature",
        "ellipse" | "ellipse_feature" => "ellipse_feature",
        "ellipse_arc" | "ellipse_arc_feature" => "ellipse_arc_feature",
        "spline" | "spline_feature" => "spline_feature",
        "clothoid" | "clothoid_feature" => "clothoid_feature",
        "linear_dimension" | "linear_dim_feature" => "linear_dim_feature",
        "curve_dimension" | "curve_dim_feature" => "curve_dim_feature",
        "angular_dimension" | "angular_dim_feature" => "angular_dim_feature",
        "radius_dimension" | "radius_dim_feature" => "radius_dim_feature",
        "diameter_dimension" | "diameter_dim_feature" => "diameter_dim_feature",
        "label" | "label_feature" => "label_feature",
        "balloon" | "balloon_feature" => "balloon_feature",
        _ => return None,
    })
}

pub(crate) fn fields(keyword: &str) -> Vec<String> {
    let mut names: Vec<String> = match keyword {
        "point_marker_feature" => "layer color x y marker angle scale",
        "ellipse_feature" => "layer color line_type line_width center_x center_y radius_x radius_y angle",
        "ellipse_arc_feature" => "layer color line_type line_width center_x center_y radius_x radius_y direction angle start_angle end_angle",
        "spline_feature" => "layer color line_type line_width open_close number xs ys",
        "clothoid_feature" => "layer color line_type line_width base_x base_y parameter direction angle start_length end_length",
        "linear_dim_feature" | "radius_dim_feature" | "diameter_dim_feature" => "layer color line_type line_width start_x start_y end_x end_y",
        "curve_dim_feature" | "angular_dim_feature" => "layer color line_type line_width center_x center_y radius start_angle end_angle",
        "label_feature" => "layer color line_type line_width number xs ys arrow_code arrow_scale",
        "balloon_feature" => "layer color line_type line_width number xs ys center_x center_y radius arrow_code arrow_scale",
        "sfig_locate_feature" => "layer name x y angle scale_x scale_y",
        "composite_curve_feature" => "color line_type line_width visible",
        "fill_area_style_colour_feature" => "layer color outer number holes",
        "fill_area_style_hatching_feature" => "layer",
        _ => "",
    }.split_whitespace().map(str::to_owned).collect();
    if matches!(
        keyword,
        "linear_dim_feature" | "curve_dim_feature" | "angular_dim_feature"
    ) {
        for n in 1..=2 {
            for field in [
                "present", "base_x", "base_y", "start_x", "start_y", "end_x", "end_y",
            ] {
                names.push(format!("extension{n}_{field}"));
            }
        }
    }
    if matches!(
        keyword,
        "linear_dim_feature"
            | "curve_dim_feature"
            | "angular_dim_feature"
            | "radius_dim_feature"
            | "diameter_dim_feature"
    ) {
        let count = if keyword == "radius_dim_feature" {
            1
        } else {
            2
        };
        for n in 1..=count {
            for field in ["code", "direction", "x", "y", "scale"] {
                names.push(format!("arrow{n}_{field}"));
            }
        }
    }
    if keyword.ends_with("_dim_feature") || matches!(keyword, "label_feature" | "balloon_feature") {
        names.extend("text_present font text text_x text_y text_height text_width text_spacing text_angle text_slant text_base_point text_direction".split_whitespace().map(str::to_owned));
    }
    names
}

pub(crate) fn update_fields(
    r: &mut Record,
    changes: &BTreeMap<String, Value>,
) -> Result<(), WriteError> {
    let keyword = r.keyword.to_ascii_lowercase();
    let names = fields(&keyword);
    for (key, value) in changes {
        if matches!(key.as_str(), "points" | "vertices")
            && matches!(
                keyword.as_str(),
                "spline_feature" | "label_feature" | "balloon_feature"
            )
        {
            let Value::List(points) = value else {
                return Err(error("points must be coordinate pairs"));
            };
            let minimum = if keyword == "spline_feature" { 2 } else { 1 };
            if points.len() < minimum {
                return Err(error("Not enough points"));
            }
            let mut xs = Vec::new();
            let mut ys = Vec::new();
            for p in points {
                let Value::List(pair) = p else {
                    return Err(error("Each point must be a coordinate pair"));
                };
                if pair.len() != 2 {
                    return Err(error("Each point must have two coordinates"));
                }
                xs.push(numeric_text(&pair[0])?);
                ys.push(numeric_text(&pair[1])?);
            }
            let offset = if keyword == "spline_feature" { 5 } else { 4 };
            r.parameters[offset] = string(points.len());
            r.parameters[offset + 1] = string(format!("({})", xs.join(",")));
            r.parameters[offset + 2] = string(format!("({})", ys.join(",")));
        } else {
            let index = names
                .iter()
                .position(|n| n == key)
                .ok_or_else(|| error(format!("Unsupported {} field {key}", r.keyword)))?;
            // Aggregate counts and code-based references have dedicated operations.
            if matches!(
                key.as_str(),
                "number" | "xs" | "ys" | "outer" | "holes" | "name"
            ) {
                return Err(error("Use the structural editing method for this field"));
            }
            r.parameters[index] = if key == "text" {
                if !matches!(value, Value::String(_)) {
                    return Err(error("text must be a string"));
                }
                value.clone()
            } else {
                string(numeric_text(value)?)
            };
        }
    }
    Ok(())
}

pub(crate) fn authored_record(
    kind: &str,
    changes: &BTreeMap<String, Value>,
) -> Result<Record, WriteError> {
    if let Some(record) = crate::authoring::basic_record(kind, changes)? {
        return Ok(record);
    }
    let keyword =
        feature_keyword(kind).ok_or_else(|| error("Unsupported authored feature kind"))?;
    let names = fields(keyword);
    let mut p: Vec<Value> = names
        .iter()
        .map(|name| {
            if name == "text" {
                return string("");
            }
            if matches!(name.as_str(), "xs" | "ys") {
                return string("(0.0,10.0)");
            }
            if name == "number" {
                return string(2);
            }
            if matches!(
                name.as_str(),
                "layer"
                    | "color"
                    | "line_type"
                    | "line_width"
                    | "font"
                    | "open_close"
                    | "text_base_point"
                    | "text_direction"
                    | "marker"
                    | "arrow_code"
            ) || name.ends_with("scale")
            {
                return string(1);
            }
            if matches!(
                name.as_str(),
                "radius" | "radius_x" | "radius_y" | "parameter"
            ) {
                return number(10.0);
            }
            if matches!(name.as_str(), "text_height" | "text_width") {
                return number(3.5);
            }
            string(0)
        })
        .collect();
    if changes.contains_key("text") {
        if let Some(i) = names.iter().position(|n| n == "text_present") {
            p[i] = string(1);
        }
    }
    let mut r = record(keyword, p);
    update_fields(&mut r, changes)?;
    Ok(r)
}

impl SfcDocument {
    pub fn add_feature(
        &mut self,
        kind: &str,
        changes: &BTreeMap<String, Value>,
    ) -> Result<i64, WriteError> {
        self.add_authored_record(authored_record(kind, changes)?)
    }

    /// Add every element to the sheet in one transaction. One clone, ID scan,
    /// splice and strict validation; failed input leaves the snapshot unchanged.
    pub fn extend(
        &mut self,
        elements: &[(String, BTreeMap<String, Value>)],
    ) -> Result<Vec<i64>, WriteError> {
        let records = elements
            .iter()
            .enumerate()
            .map(|(index, (kind, fields))| {
                authored_record(kind, fields).map_err(|e| error(format!("Element {index}: {e}")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if records.is_empty() {
            return Ok(Vec::new());
        }
        let first = self.next_id()?;
        let count = i64::try_from(records.len()).map_err(|_| error("Too many elements"))?;
        first
            .checked_add(count - 1)
            .ok_or_else(|| error("Entity ID overflow"))?;
        let sheet = self
            .output
            .document
            .sfc_model
            .as_ref()
            .unwrap()
            .sheet
            .as_ref()
            .unwrap()
            .entity_id;
        let mut document = self.output.document.clone();
        let index = document
            .entities
            .iter()
            .position(|e| e.id == sheet)
            .unwrap();
        let ids: Vec<_> = (0..count).map(|offset| first + offset).collect();
        let entities = records
            .into_iter()
            .zip(&ids)
            .map(|(r, id)| instance(*id, r));
        document.entities.splice(index..index, entities);
        self.commit(document)?;
        Ok(ids)
    }
    pub(crate) fn add_authored_record(&mut self, r: Record) -> Result<i64, WriteError> {
        let id = self.next_id()?;
        let sheet = self
            .output
            .document
            .sfc_model
            .as_ref()
            .unwrap()
            .sheet
            .as_ref()
            .unwrap()
            .entity_id;
        let mut document = self.output.document.clone();
        let index = document
            .entities
            .iter()
            .position(|e| e.id == sheet)
            .unwrap();
        document.entities.insert(index, instance(id, r));
        self.commit(document)?;
        Ok(id)
    }
    pub(crate) fn geometry_index(&self, id: i64) -> Result<usize, WriteError> {
        let index = self
            .output
            .document
            .entities
            .iter()
            .position(|e| e.id == id)
            .ok_or_else(|| error(format!("Unknown entity #{id}")))?;
        let EntityBody::Simple(r) = &self.output.document.entities[index].body else {
            return Err(error("Not an editable feature"));
        };
        if !crate::editor::basic(&r.keyword)
            && feature_keyword(&r.keyword.to_ascii_lowercase()).is_none()
            && !matches!(
                r.keyword.to_ascii_lowercase().as_str(),
                "sfig_locate_feature"
                    | "composite_curve_feature"
                    | "fill_area_style_colour_feature"
                    | "fill_area_style_hatching_feature"
            )
        {
            return Err(error("Not an editable geometry or placement"));
        }
        Ok(index)
    }
    fn selected_sheet_components(&self, ids: &[i64]) -> Result<BTreeSet<i64>, WriteError> {
        let selected: BTreeSet<_> = ids.iter().copied().collect();
        if selected.is_empty() || selected.len() != ids.len() {
            return Err(error("Components must be nonempty and unique"));
        }
        let model = self.output.document.sfc_model.as_ref().unwrap();
        if !selected
            .iter()
            .all(|id| model.sheet.as_ref().unwrap().component_ids.contains(id))
        {
            return Err(error("Grouping requires components directly on the sheet"));
        }
        // Attribute wrappers carry independent SAF identity/dependencies: leave them intact.
        if model
            .attribute_attachments
            .iter()
            .any(|a| a.placement_ids.iter().any(|id| selected.contains(id)))
        {
            return Err(error("Attribute attachment placements cannot be regrouped"));
        }
        Ok(selected)
    }
    fn wrap_sheet_components(
        &mut self,
        selected: &BTreeSet<i64>,
        marker: EntityInstance,
        placements: &[EntityInstance],
    ) -> Result<(), WriteError> {
        let mut document = self.output.document.clone();
        let start = document.entities.iter().rposition(|e| {
            matches!(&e.body, EntityBody::Simple(r) if matches!(r.keyword.to_ascii_lowercase().as_str(), "sfig_org_feature" | "composite_curve_feature"))
        }).map_or_else(|| {
            document.entities.iter().position(|e| {
                document.sfc_model.as_ref().unwrap().sheet.as_ref().unwrap().component_ids.contains(&e.id)
            }).unwrap()
        }, |i| i + 1);
        let original = document.entities.clone();
        let children: Vec<_> = original
            .iter()
            .filter(|e| selected.contains(&e.id))
            .cloned()
            .collect();
        document.entities.clear();
        let mut placed = false;
        for (i, e) in original.into_iter().enumerate() {
            if i == start {
                document.entities.extend(children.clone());
                document.entities.push(marker.clone());
            }
            if selected.contains(&e.id) {
                if !placed {
                    document.entities.extend_from_slice(placements);
                    placed = true;
                }
            } else {
                document.entities.push(e);
            }
        }
        self.commit(document)
    }
    /// Move sheet components into one drawing group/part/partial drawing atomically.
    pub fn group_elements(
        &mut self,
        name: &str,
        ids: &[i64],
        kind: i64,
        placement: &[Value],
    ) -> Result<i64, WriteError> {
        if placement.len() != 6 {
            return Err(error(
                "A placement requires layer, x, y, angle, scale_x and scale_y",
            ));
        }
        if name.starts_with("$$ATR") {
            return Err(error("Reserved attribute group name"));
        }
        let selected = self.selected_sheet_components(ids)?;
        let def_id = self.next_id()?;
        let placement_id = def_id
            .checked_add(1)
            .ok_or_else(|| error("Entity ID overflow"))?;
        let marker = instance(
            def_id,
            record("sfig_org_feature", vec![string(name), string(kind)]),
        );
        let mut p = vec![placement[0].clone(), string(name)];
        p.extend_from_slice(&placement[1..]);
        self.wrap_sheet_components(
            &selected,
            marker,
            &[instance(placement_id, record("sfig_locate_feature", p))],
        )?;
        Ok(placement_id)
    }

    /// Define a shared part and only the explicitly requested placements.
    /// A complete transaction avoids both a phantom origin placement and an
    /// invalid, unreferenced definition between separate builder operations.
    pub fn create_part(
        &mut self,
        name: &str,
        ids: &[i64],
        placements: &[Vec<Value>],
    ) -> Result<Vec<i64>, WriteError> {
        if placements.is_empty() {
            return Err(error("A part needs at least one explicit placement"));
        }
        if name.starts_with("$$ATR") {
            return Err(error("Reserved attribute group name"));
        }
        let selected = self.selected_sheet_components(ids)?;
        let definition_id = self.next_id()?;
        let mut instances = Vec::new();
        let mut placement_ids = Vec::new();
        for (index, placement) in placements.iter().enumerate() {
            if placement.len() != 6 {
                return Err(error("Invalid placement fields"));
            }
            let offset = i64::try_from(index)
                .ok()
                .and_then(|v| v.checked_add(1))
                .ok_or_else(|| error("Entity ID overflow"))?;
            let id = definition_id
                .checked_add(offset)
                .ok_or_else(|| error("Entity ID overflow"))?;
            let mut parameters = vec![placement[0].clone(), string(name)];
            parameters.extend_from_slice(&placement[1..]);
            instances.push(instance(id, record("sfig_locate_feature", parameters)));
            placement_ids.push(id);
        }
        self.wrap_sheet_components(
            &selected,
            instance(
                definition_id,
                record("sfig_org_feature", vec![string(name), string(4)]),
            ),
            &instances,
        )?;
        Ok(placement_ids)
    }
    pub fn ungroup(&mut self, placement_id: i64) -> Result<(), WriteError> {
        let model = self.output.document.sfc_model.as_ref().unwrap();
        let reference = model
            .sfig_references
            .iter()
            .find(|r| r.placement_id == placement_id)
            .ok_or_else(|| error("Not an ordinary group placement"))?;
        let definition = model
            .sfig_definitions
            .iter()
            .find(|d| d.entity_id == reference.definition_id)
            .unwrap();
        if definition.kind_flag != 3 {
            return Err(error("Only identity drawing groups can be ungrouped"));
        }
        let children: BTreeSet<_> = definition.component_ids.iter().copied().collect();
        let original = self.output.document.entities.clone();
        let members: Vec<_> = original
            .iter()
            .filter(|e| children.contains(&e.id))
            .cloned()
            .collect();
        let mut document = self.output.document.clone();
        document.entities.clear();
        for e in original {
            if e.id == placement_id {
                document.entities.extend(members.clone());
            } else if e.id != definition.entity_id && !children.contains(&e.id) {
                document.entities.push(e);
            }
        }
        self.commit(document)
    }
    /// Move newly authored sheet elements into an existing definition, retaining IDs.
    pub fn add_to_group(&mut self, placement_id: i64, ids: &[i64]) -> Result<(), WriteError> {
        let selected = self.selected_sheet_components(ids)?;
        let model = self.output.document.sfc_model.as_ref().unwrap();
        let reference = model
            .sfig_references
            .iter()
            .find(|r| r.placement_id == placement_id)
            .ok_or_else(|| error("Not an ordinary group placement"))?;
        if selected.contains(&placement_id) {
            return Err(error("A group cannot contain itself"));
        }
        let definition_id = reference.definition_id;
        let members: Vec<_> = self
            .output
            .document
            .entities
            .iter()
            .filter(|e| selected.contains(&e.id))
            .cloned()
            .collect();
        let mut document = self.output.document.clone();
        document.entities.clear();
        for e in &self.output.document.entities {
            if e.id == definition_id {
                document.entities.extend(members.clone());
            }
            if !selected.contains(&e.id) {
                document.entities.push(e.clone());
            }
        }
        // Forward/nesting restrictions are checked before committing the candidate.
        self.commit(document)
    }
    /// Additional placements are allowed for drawing parts (kind 4) only.
    pub fn place_part(
        &mut self,
        placement_id: i64,
        placement: &[Value],
    ) -> Result<i64, WriteError> {
        if placement.len() != 6 {
            return Err(error("Invalid placement fields"));
        }
        let model = self.output.document.sfc_model.as_ref().unwrap();
        let reference = model
            .sfig_references
            .iter()
            .find(|r| r.placement_id == placement_id)
            .ok_or_else(|| error("Not an ordinary part placement"))?;
        let definition = model
            .sfig_definitions
            .iter()
            .find(|d| d.entity_id == reference.definition_id)
            .unwrap();
        if definition.kind_flag != 4 {
            return Err(error("Only drawing parts allow multiple placements"));
        }
        let mut p = vec![placement[0].clone(), string(&definition.name)];
        p.extend_from_slice(&placement[1..]);
        self.add_authored_record(record("sfig_locate_feature", p))
    }
    pub fn rename_group(&mut self, placement_id: i64, name: &str) -> Result<(), WriteError> {
        if name.starts_with("$$ATR") {
            return Err(error("Reserved attribute group name"));
        }
        let model = self.output.document.sfc_model.as_ref().unwrap();
        let reference = model
            .sfig_references
            .iter()
            .find(|r| r.placement_id == placement_id)
            .ok_or_else(|| error("Not an ordinary group placement"))?;
        let definition_id = reference.definition_id;
        let placements: BTreeSet<_> = model
            .sfig_references
            .iter()
            .filter(|r| r.definition_id == definition_id)
            .map(|r| r.placement_id)
            .collect();
        let mut document = self.output.document.clone();
        for e in &mut document.entities {
            let EntityBody::Simple(r) = &mut e.body else {
                continue;
            };
            if e.id == definition_id {
                r.parameters[0] = string(name);
            } else if placements.contains(&e.id) {
                r.parameters[1] = string(name);
            }
        }
        self.commit(document)
    }
    /// Return an entity ID, never an order-dependent composite-curve code.
    pub fn add_composite_curve(
        &mut self,
        ids: &[i64],
        codes: [i64; 3],
        visible: bool,
    ) -> Result<i64, WriteError> {
        let selected = self.selected_sheet_components(ids)?;
        let id = self.next_id()?;
        let mut p: Vec<_> = codes.into_iter().map(string).collect();
        p.push(string(i64::from(visible)));
        self.wrap_sheet_components(
            &selected,
            instance(id, record("composite_curve_feature", p)),
            &[],
        )?;
        Ok(id)
    }
    fn curve_codes(&self, outer: i64, holes: &[i64]) -> Result<(i64, Vec<i64>), WriteError> {
        let curves = &self
            .output
            .document
            .sfc_model
            .as_ref()
            .unwrap()
            .composite_curve_definitions;
        let code = |id| {
            curves
                .iter()
                .find(|d| d.entity_id == id)
                .map(|d| d.code)
                .ok_or_else(|| error("Unknown composite-curve entity ID"))
        };
        let selected: BTreeSet<_> = std::iter::once(outer)
            .chain(holes.iter().copied())
            .collect();
        if selected.len() != holes.len() + 1 {
            return Err(error("Hatch boundaries must be unique"));
        }
        Ok((
            code(outer)?,
            holes.iter().map(|id| code(*id)).collect::<Result<_, _>>()?,
        ))
    }
    pub fn add_fill(
        &mut self,
        outer: i64,
        holes: &[i64],
        layer: i64,
        color: i64,
    ) -> Result<i64, WriteError> {
        let (outer, holes) = self.curve_codes(outer, holes)?;
        self.add_authored_record(record(
            "fill_area_style_colour_feature",
            vec![
                string(layer),
                string(color),
                string(outer),
                string(holes.len()),
                integer_array(&holes),
            ],
        ))
    }
    pub fn release_composite_curve(&mut self, id: i64) -> Result<(), WriteError> {
        let model = self.output.document.sfc_model.as_ref().unwrap();
        let curve = model
            .composite_curve_definitions
            .iter()
            .find(|d| d.entity_id == id)
            .ok_or_else(|| error("Unknown composite-curve entity ID"))?;
        if model
            .hatch_references
            .iter()
            .any(|r| r.outer_definition_id == id || r.inner_definition_ids.contains(&id))
        {
            return Err(error("The boundary is still referenced by a hatch"));
        }
        let children: BTreeSet<_> = curve.component_ids.iter().copied().collect();
        let members: Vec<_> = self
            .output
            .document
            .entities
            .iter()
            .filter(|e| children.contains(&e.id))
            .cloned()
            .collect();
        let sheet = model.sheet.as_ref().unwrap().entity_id;
        let mut document = self.output.document.clone();
        document.entities.clear();
        for e in &self.output.document.entities {
            if e.id == sheet {
                document.entities.extend(members.clone());
            }
            if e.id != id && !children.contains(&e.id) {
                document.entities.push(e.clone());
            }
        }
        // Composite-curve codes are assigned by declaration order. Releasing a
        // definition shifts later codes, so retarget every hatch by its original
        // resolved entity identity, rather than allowing a valid but wrong code.
        let new_codes: BTreeMap<_, _> = model
            .composite_curve_definitions
            .iter()
            .filter(|d| d.entity_id != id)
            .enumerate()
            .map(|(i, d)| (d.entity_id, (i + 1) as i64))
            .collect();
        for reference in &model.hatch_references {
            let entity = document
                .entities
                .iter_mut()
                .find(|e| e.id == reference.hatch_id)
                .unwrap();
            let EntityBody::Simple(r) = &mut entity.body else {
                unreachable!()
            };
            let offset = match r.keyword.to_ascii_lowercase().as_str() {
                "externally_defined_hatch_feature" | "fill_area_style_colour_feature" => 2,
                "fill_area_style_hatching_feature" => {
                    2 + r.parameters[1].as_i64().unwrap() as usize
                }
                "fill_area_style_tiles_hatching_feature" => 12,
                _ => return Err(error("Unsupported hatch reference mechanism")),
            };
            r.parameters[offset] = string(new_codes[&reference.outer_definition_id]);
            let holes: Vec<_> = reference
                .inner_definition_ids
                .iter()
                .map(|id| new_codes[id])
                .collect();
            r.parameters[offset + 2] = integer_array(&holes);
        }
        self.commit(document)
    }
    pub fn add_hatch(
        &mut self,
        outer: i64,
        holes: &[i64],
        layer: i64,
        patterns: &[Vec<Value>],
    ) -> Result<i64, WriteError> {
        let (outer, holes) = self.curve_codes(outer, holes)?;
        let mut p = vec![string(layer), string(patterns.len())];
        for pattern in patterns {
            p.push(string(format!(
                "({})",
                pattern
                    .iter()
                    .map(numeric_text)
                    .collect::<Result<Vec<_>, _>>()?
                    .join(",")
            )));
        }
        p.extend([string(outer), string(holes.len()), integer_array(&holes)]);
        self.add_authored_record(record("fill_area_style_hatching_feature", p))
    }
    pub fn update_hatch_patterns(
        &mut self,
        id: i64,
        patterns: &[Vec<Value>],
    ) -> Result<(), WriteError> {
        let index = self.geometry_index(id)?;
        let mut document = self.output.document.clone();
        let EntityBody::Simple(r) = &mut document.entities[index].body else {
            unreachable!()
        };
        if !r
            .keyword
            .eq_ignore_ascii_case("fill_area_style_hatching_feature")
        {
            return Err(error("Not a user-defined hatch"));
        }
        let count = r.parameters[1].as_i64().unwrap() as usize;
        let tail = r.parameters[2 + count..].to_vec();
        let mut p = vec![r.parameters[0].clone(), string(patterns.len())];
        for pattern in patterns {
            p.push(string(format!(
                "({})",
                pattern
                    .iter()
                    .map(numeric_text)
                    .collect::<Result<Vec<_>, _>>()?
                    .join(",")
            )));
        }
        p.extend(tail);
        r.parameters = p;
        self.commit(document)
    }
    pub fn update_hatch_boundaries(
        &mut self,
        id: i64,
        outer: i64,
        holes: &[i64],
    ) -> Result<(), WriteError> {
        let index = self.geometry_index(id)?;
        let (outer, holes) = self.curve_codes(outer, holes)?;
        let mut document = self.output.document.clone();
        let EntityBody::Simple(r) = &mut document.entities[index].body else {
            unreachable!()
        };
        let offset = match r.keyword.to_ascii_lowercase().as_str() {
            "fill_area_style_colour_feature" => 2,
            "fill_area_style_hatching_feature" => 2 + r.parameters[1].as_i64().unwrap() as usize,
            _ => return Err(error("Not an editable fill or hatch")),
        };
        r.parameters[offset] = string(outer);
        r.parameters[offset + 1] = string(holes.len());
        r.parameters[offset + 2] = integer_array(&holes);
        self.commit(document)
    }
}
fn integer_array(values: &[i64]) -> Value {
    string(format!(
        "({})",
        values
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn document() -> SfcDocument {
        SfcDocument::new("drawing.sfc", "drawing", 297, 210, "2026-10-06T00:00:00").unwrap()
    }
    fn ellipse(doc: &mut SfcDocument, x: f64) -> i64 {
        doc.add_feature(
            "ellipse",
            &BTreeMap::from([("center_x".into(), Value::Real(x))]),
        )
        .unwrap()
    }
    fn identity() -> Vec<Value> {
        vec![
            string(0),
            number(0.0),
            number(0.0),
            number(0.0),
            number(1.0),
            number(1.0),
        ]
    }
    #[test]
    fn batch_validation_rolls_back_and_preserves_existing_definition() {
        let mut doc = document();
        let child = ellipse(&mut doc, 20.0);
        let group = doc
            .group_elements("group", &[child], 3, &identity())
            .unwrap();
        let before = doc.output.clone();
        let valid = ("circle".into(), BTreeMap::new());
        let invalid = (
            "circle".into(),
            BTreeMap::from([("radius".into(), Value::Real(-1.0))]),
        );
        assert!(doc.extend(&[valid.clone(), invalid]).is_err());
        assert_eq!(doc.output, before);
        assert!(doc.extend(&[]).unwrap().is_empty());
        let ids = doc.extend(&[valid.clone(), valid]).unwrap();
        let model = doc.output.document.sfc_model.as_ref().unwrap();
        assert_eq!(
            model.sheet.as_ref().unwrap().component_ids,
            [group, ids[0], ids[1]]
        );
        assert_eq!(
            model.sfig_definitions,
            before.document.sfc_model.unwrap().sfig_definitions
        );
    }
    #[test]
    fn explicit_shared_part_is_atomic_and_has_no_origin_placement() {
        let mut doc = document();
        let child = ellipse(&mut doc, 20.0);
        let before = doc.output.clone();
        let mut placement = identity();
        placement[0] = string(1);
        placement[1] = number(100.0);
        let mut bad = placement.clone();
        bad[4] = number(0.0);
        assert!(doc
            .create_part("part", &[child], &[placement.clone(), bad])
            .is_err());
        assert_eq!(doc.output, before);
        let ids = doc.create_part("part", &[child], &[placement]).unwrap();
        let model = doc.output.document.sfc_model.as_ref().unwrap();
        assert_eq!(model.sheet.as_ref().unwrap().component_ids, ids);
        assert_eq!(model.sfig_references.len(), 1);
        assert_eq!(model.sfig_definitions[0].component_ids, [child]);
    }
    #[test]
    fn complex_geometry_preserves_version_and_rollback() {
        let mut doc = document();
        for kind in [
            "ellipse",
            "ellipse_arc",
            "spline",
            "point_marker",
            "clothoid",
            "linear_dimension",
            "angular_dimension",
            "curve_dimension",
            "radius_dimension",
            "diameter_dimension",
            "label",
            "balloon",
        ] {
            let id = doc.add_feature(kind, &BTreeMap::new()).unwrap();
            assert_eq!(
                doc.output
                    .document
                    .entities
                    .iter()
                    .find(|e| e.id == id)
                    .unwrap()
                    .sfc_version,
                if matches!(kind, "clothoid" | "curve_dimension") {
                    Some(SfcVersionTag::V31)
                } else {
                    Some(SfcVersionTag::V2)
                }
            );
        }
        let id = ellipse(&mut doc, 1.0);
        let before = doc.output.clone();
        assert!(doc
            .update_element(
                id,
                &BTreeMap::from([("radius_x".into(), Value::Real(-1.0))])
            )
            .is_err());
        assert_eq!(doc.output, before);
    }
    #[test]
    fn nested_groups_do_not_capture_unrelated_elements() {
        let mut doc = document();
        let a = ellipse(&mut doc, 1.0);
        let b = ellipse(&mut doc, 2.0);
        let c = ellipse(&mut doc, 3.0);
        let inner = doc.group_elements("inner", &[b], 3, &identity()).unwrap();
        assert_eq!(
            doc.output
                .document
                .sfc_model
                .as_ref()
                .unwrap()
                .sheet
                .as_ref()
                .unwrap()
                .component_ids,
            vec![a, inner, c]
        );
        let outer = doc
            .group_elements("outer", &[inner, c], 3, &identity())
            .unwrap();
        doc.update_element(b, &BTreeMap::from([("center_x".into(), Value::Real(25.0))]))
            .unwrap();
        let d = ellipse(&mut doc, 4.0);
        doc.add_to_group(outer, &[d]).unwrap();
        let before = doc.output.clone();
        assert!(doc.add_to_group(outer, &[outer]).is_err());
        assert_eq!(doc.output, before);
        doc.ungroup(inner).unwrap();
        doc.ungroup(outer).unwrap();
        assert_eq!(
            doc.output
                .document
                .sfc_model
                .as_ref()
                .unwrap()
                .sheet
                .as_ref()
                .unwrap()
                .component_ids,
            vec![a, b, c, d]
        );
    }
    #[test]
    fn parts_can_be_reused_but_groups_cannot_be_transformed() {
        let mut doc = document();
        let a = ellipse(&mut doc, 1.0);
        let group = doc.group_elements("group", &[a], 3, &identity()).unwrap();
        let before = doc.output.clone();
        assert!(doc
            .update_element(group, &BTreeMap::from([("x".into(), Value::Real(20.0))]))
            .is_err());
        assert_eq!(doc.output, before);
        doc.ungroup(group).unwrap();
        let part = doc.group_elements("part", &[a], 4, &identity()).unwrap();
        let mut placement = identity();
        placement[1] = number(20.0);
        let second = doc.place_part(part, &placement).unwrap();
        doc.rename_group(part, "renamed").unwrap();
        assert_eq!(
            doc.output
                .document
                .sfc_model
                .as_ref()
                .unwrap()
                .sfig_references
                .len(),
            2
        );
        doc.remove_element(second).unwrap();
        let before = doc.output.clone();
        assert!(doc.remove_element(part).is_err());
        assert_eq!(doc.output, before);
    }
    #[test]
    fn hatch_boundaries_use_stable_entity_ids_and_reject_invalid_patterns() {
        let mut doc = document();
        let polygon = doc
            .add_polyline([1, 1, 1, 1], &[(0., 0.), (10., 0.), (10., 10.), (0., 0.)])
            .unwrap();
        let curve = doc
            .add_composite_curve(&[polygon], [1, 1, 1], false)
            .unwrap();
        let fill = doc.add_fill(curve, &[], 1, 2).unwrap();
        let hatch = doc
            .add_hatch(
                curve,
                &[],
                1,
                &[vec![
                    Value::Integer(1),
                    Value::Integer(1),
                    Value::Integer(1),
                    Value::Real(0.0),
                    Value::Real(0.0),
                    Value::Real(2.0),
                    Value::Real(45.0),
                ]],
            )
            .unwrap();
        doc.update_hatch_boundaries(fill, curve, &[]).unwrap();
        let before = doc.output.clone();
        assert!(doc.add_fill(curve, &[curve], 1, 1).is_err());
        assert!(doc
            .add_hatch(curve, &[], 1, &[vec![Value::Integer(1)]])
            .is_err());
        assert!(doc.remove_element(curve).is_err());
        assert_eq!(doc.output, before);
        doc.remove_element(hatch).unwrap();
        doc.remove_element(fill).unwrap();
    }
    #[test]
    fn releasing_earlier_boundary_retargets_later_hatches_by_entity_identity() {
        let mut doc = document();
        let mut curves = Vec::new();
        for x in [0., 20., 40.] {
            let edge = doc
                .add_polyline(
                    [1, 1, 1, 1],
                    &[(x, 0.), (x + 10., 0.), (x + 10., 10.), (x, 0.)],
                )
                .unwrap();
            curves.push(doc.add_composite_curve(&[edge], [1, 1, 1], false).unwrap());
        }
        let fill = doc.add_fill(curves[1], &[], 1, 2).unwrap();
        let before = doc
            .output
            .document
            .sfc_model
            .as_ref()
            .unwrap()
            .hatch_references
            .clone();
        doc.release_composite_curve(curves[0]).unwrap();
        let after = &doc
            .output
            .document
            .sfc_model
            .as_ref()
            .unwrap()
            .hatch_references;
        assert_eq!(after, &before);
        assert_eq!(after[0].hatch_id, fill);
        assert_eq!(after[0].outer_definition_id, curves[1]);
    }
    #[test]
    fn uppercase_existing_records_can_be_regrouped_and_retargeted() {
        let mut doc = document();
        let grouped = ellipse(&mut doc, 1.0);
        doc.group_elements("existing", &[grouped], 3, &identity())
            .unwrap();
        let edge = doc
            .add_polyline([1, 1, 1, 1], &[(0., 0.), (10., 0.), (10., 10.), (0., 0.)])
            .unwrap();
        let curve = doc.add_composite_curve(&[edge], [1, 1, 1], false).unwrap();
        let fill = doc.add_fill(curve, &[], 1, 2).unwrap();
        let sheet_element = ellipse(&mut doc, 20.0);
        let bytes = doc.to_bytes(crate::SfcWriteOptions::default()).unwrap();
        let mut text = encoding_rs::SHIFT_JIS.decode(&bytes).0.into_owned();
        for e in &doc.output.document.entities {
            if let EntityBody::Simple(r) = &e.body {
                text = text.replace(&r.keyword, &r.keyword.to_ascii_uppercase());
            }
        }
        let output = crate::parse_sfc_text(&text, true).unwrap();
        let mut edited = SfcDocument::from_output(output).unwrap();
        edited.update_hatch_boundaries(fill, curve, &[]).unwrap();
        edited
            .group_elements("new", &[sheet_element], 3, &identity())
            .unwrap();
        assert_eq!(
            edited
                .output
                .document
                .sfc_model
                .as_ref()
                .unwrap()
                .sfig_definitions[0]
                .component_ids,
            vec![grouped]
        );
    }
}
