//! Structured batch authoring. Each input builds an isolated fragment; accepted
//! fragments share one document clone and one strict commit, including skip mode.
use crate::complex_editor::authored_record;
use crate::editor::{instance, number, numeric_text, record, string, SfcDocument};
use crate::features::{additional_sfc_code_references, SfcCodeKind};
use crate::model::*;
use crate::writer::{validate_authored_record, WriteError};
use std::collections::{BTreeMap, HashMap, HashSet};

pub(crate) type Fields = BTreeMap<String, Value>;
pub(crate) type Leaf = (String, Fields);

#[derive(Clone, Debug)]
pub(crate) struct Placement {
    pub position: (f64, f64),
    pub angle: f64,
    pub scale: (f64, f64),
    pub layer: i64,
}
impl Default for Placement {
    fn default() -> Self {
        Self {
            position: (0.0, 0.0),
            angle: 0.0,
            scale: (1.0, 1.0),
            layer: 1,
        }
    }
}
impl Placement {
    fn record(&self, name: &str) -> Result<Record, WriteError> {
        crate::features::validate_placement_scale(self.scale.0, self.scale.1).map_err(error)?;
        Ok(record(
            "sfig_locate_feature",
            vec![
                string(self.layer),
                string(name),
                number(self.position.0),
                number(self.position.1),
                number(self.angle),
                number(self.scale.0),
                number(self.scale.1),
            ],
        ))
    }
}

#[derive(Clone, Debug)]
pub(crate) enum BatchElement {
    Feature(Leaf),
    Composite {
        boundary: Vec<Leaf>,
        codes: [i64; 3],
        visible: bool,
    },
    Fill {
        outer: Vec<Leaf>,
        holes: Vec<Vec<Leaf>>,
        layer: i64,
        color: i64,
        patterns: Option<Vec<Vec<Value>>>,
    },
    Figure {
        name: String,
        kind: i64,
        elements: Vec<BatchElement>,
        placements: Vec<Placement>,
    },
    Placement {
        name: String,
        placement: Placement,
    },
}
#[derive(Debug, PartialEq)]
pub(crate) enum BatchId {
    Single(i64),
    Placements(Vec<i64>),
}
#[derive(Debug)]
pub(crate) struct BatchResult {
    pub ids: Vec<Option<BatchId>>,
    pub rejected: Vec<(usize, String)>,
}

fn error(s: impl Into<String>) -> WriteError {
    WriteError(s.into())
}

struct Fragment<'a> {
    next: i64,
    curve_code: i64,
    definitions: Vec<EntityInstance>,
    own_names: HashMap<String, i64>,
    known_names: &'a HashMap<String, i64>,
    tables: &'a SfcCodeTables,
    target_p21: bool,
}
impl Fragment<'_> {
    fn append(
        &mut self,
        r: Record,
        destination: &mut Vec<EntityInstance>,
    ) -> Result<(i64, TypedFeature), WriteError> {
        let id = self.next;
        let feature =
            validate_authored_record(&r).map_err(|e| error(format!("Entity #{id}: {e}")))?;
        self.codes(&feature)
            .map_err(|e| error(format!("Entity #{id}: {e}")))?;
        if self.target_p21 {
            if let Some(reason) = crate::p21_writer::p21_feature_reason(&feature) {
                return Err(error(format!("Entity #{id}: {reason}")));
            }
        }
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| error("Entity ID overflow"))?;
        destination.push(instance(id, r));
        Ok((id, feature))
    }
    fn codes(&self, feature: &TypedFeature) -> Result<(), WriteError> {
        let check = |code: Option<i64>, table: &[SfcCodeBinding], name: &str| {
            let predefined = match name {
                "colour" => COMMON_PREDEFINED_COLOR_MAX_CODE,
                "line type" => COMMON_PREDEFINED_LINE_TYPE_MAX_CODE,
                "line width" => COMMON_PREDEFINED_LINE_WIDTH_MAX_CODE,
                _ => 0,
            };
            if let Some(code) = code {
                if code > predefined && !table.iter().any(|v| v.code == code) {
                    return Err(error(format!("Undefined {name} code {code}")));
                }
            }
            Ok(())
        };
        if let Some(s) = feature.style() {
            // Match strict SFC's legacy default-code behavior when optional
            // layer/font tables are absent. Dimension font references below
            // still require a definition, as in the full model validator.
            if !self.tables.layers.is_empty() {
                check(s.layer_code, &self.tables.layers, "layer")?;
            }
            check(s.color_code, &self.tables.colors, "colour")?;
            check(s.line_type_code, &self.tables.line_types, "line type")?;
            check(s.line_width_code, &self.tables.line_widths, "line width")?;
            if !self.tables.text_fonts.is_empty() {
                check(s.font_code, &self.tables.text_fonts, "font")?;
            }
        }
        for (kind, code) in additional_sfc_code_references(feature) {
            let (table, name) = match kind {
                SfcCodeKind::Color => (&self.tables.colors, "colour"),
                SfcCodeKind::LineType => (&self.tables.line_types, "line type"),
                SfcCodeKind::LineWidth => (&self.tables.line_widths, "line width"),
                SfcCodeKind::TextFont => (&self.tables.text_fonts, "font"),
            };
            check(Some(code), table, name)?;
        }
        Ok(())
    }
    fn boundary(
        &mut self,
        leaves: &[Leaf],
        codes: [i64; 3],
        visible: bool,
        closed: bool,
    ) -> Result<(i64, i64, Vec<Point2>), WriteError> {
        if leaves.is_empty() {
            return Err(error("A boundary needs at least one curve"));
        }
        let mut records = Vec::new();
        let mut features = Vec::new();
        for (kind, fields) in leaves {
            let r = authored_record(kind, fields)?;
            let (_, f) = self.append(r, &mut records)?;
            if !crate::features::is_composite_curve_component(&f) {
                return Err(error(
                    "Boundary components must be polyline, arc, ellipse_arc or spline",
                ));
            }
            features.push(f);
        }
        let polygon = if closed {
            crate::p21_writer::check_inline_boundary(&features)?
        } else {
            Vec::new()
        };
        let p = codes
            .into_iter()
            .map(string)
            .chain(std::iter::once(string(i64::from(visible))))
            .collect();
        let (id, _) = self.append(record("composite_curve_feature", p), &mut records)?;
        self.curve_code = self
            .curve_code
            .checked_add(1)
            .ok_or_else(|| error("Too many composite curves"))?;
        self.definitions.extend(records);
        Ok((id, self.curve_code, polygon))
    }
    fn build(
        &mut self,
        element: &BatchElement,
        depth: usize,
        roots: &mut Vec<EntityInstance>,
    ) -> Result<BatchId, WriteError> {
        if depth > 32 {
            return Err(error("Nested batch elements exceed depth 32"));
        }
        match element {
            BatchElement::Feature((kind, fields)) => {
                let (id, _) = self.append(authored_record(kind, fields)?, roots)?;
                Ok(BatchId::Single(id))
            }
            BatchElement::Composite {
                boundary,
                codes,
                visible,
            } => Ok(BatchId::Single(
                self.boundary(boundary, *codes, *visible, false)?.0,
            )),
            BatchElement::Fill {
                outer,
                holes,
                layer,
                color,
                patterns,
            } => {
                let (_, outer, polygon) = self.boundary(outer, [*color, 1, 1], false, true)?;
                let mut polygons = vec![polygon];
                let mut inner = Vec::new();
                for hole in holes {
                    let (_, code, polygon) = self.boundary(hole, [*color, 1, 1], false, true)?;
                    inner.push(code);
                    polygons.push(polygon);
                }
                crate::p21_writer::check_inline_fill(&polygons)
                    .map_err(|e| error(format!("Entity #{}: {e}", self.next)))?;
                let integers = string(format!(
                    "({})",
                    inner
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(",")
                ));
                let r = if let Some(patterns) = patterns {
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
                    p.extend([string(outer), string(inner.len()), integers]);
                    record("fill_area_style_hatching_feature", p)
                } else {
                    record(
                        "fill_area_style_colour_feature",
                        vec![
                            string(layer),
                            string(color),
                            string(outer),
                            string(inner.len()),
                            integers,
                        ],
                    )
                };
                Ok(BatchId::Single(self.append(r, roots)?.0))
            }
            BatchElement::Figure {
                name,
                kind,
                elements,
                placements,
            } => {
                if name.starts_with("$$ATR") {
                    return Err(error("Reserved attribute group name"));
                }
                if self.known_names.contains_key(name) || self.own_names.contains_key(name) {
                    return Err(error(format!("Duplicate composite figure name {name:?}")));
                }
                if !matches!(kind, 1 | 3 | 4) {
                    return Err(error(
                        "Batch figures support partial drawing (1), group (3), and part (4)",
                    ));
                }
                if depth > 0 && *kind == 1 {
                    return Err(error(
                        "A partial drawing cannot be nested inside another figure",
                    ));
                }
                if elements.is_empty() || placements.is_empty() {
                    return Err(error(
                        "A figure needs nonempty elements and explicit placements",
                    ));
                }
                if *kind != 4 && placements.len() != 1 {
                    return Err(error("Only drawing parts allow multiple placements"));
                }
                for (i, placement) in placements.iter().enumerate() {
                    crate::features::validate_placement_scale(placement.scale.0, placement.scale.1)
                        .map_err(|e| error(format!("placements[{i}]: {e}")))?;
                }
                if *kind == 3
                    && placements.iter().any(|p| {
                        p.position != (0.0, 0.0) || p.angle != 0.0 || p.scale != (1.0, 1.0)
                    })
                {
                    return Err(error("Drawing groups require identity placement; use a part or partial drawing for transforms"));
                }
                // Reserve the name before recursion, but do not allow recursive placements.
                self.own_names.insert(name.clone(), 0);
                let mut children = Vec::new();
                for (i, child) in elements.iter().enumerate() {
                    self.build(child, depth + 1, &mut children)
                        .map_err(|e| error(format!("elements[{i}]: {e}")))?;
                }
                self.append(
                    record("sfig_org_feature", vec![string(name), string(kind)]),
                    &mut children,
                )?;
                self.definitions.extend(children);
                self.own_names.insert(name.clone(), *kind);
                let mut ids = Vec::new();
                for (i, p) in placements.iter().enumerate() {
                    ids.push(
                        self.append(p.record(name)?, roots)
                            .map_err(|e| error(format!("placements[{i}]: {e}")))?
                            .0,
                    );
                }
                if *kind == 4 {
                    Ok(BatchId::Placements(ids))
                } else {
                    Ok(BatchId::Single(ids[0]))
                }
            }
            BatchElement::Placement { name, placement } => {
                let kind = self
                    .own_names
                    .get(name)
                    .or_else(|| self.known_names.get(name));
                if kind != Some(&4) {
                    return Err(error(format!(
                        "Placement needs an earlier, nonrecursive drawing part {name:?}"
                    )));
                }
                Ok(BatchId::Single(
                    self.append(placement.record(name)?, roots)?.0,
                ))
            }
        }
    }
}

impl SfcDocument {
    pub(crate) fn extend_structured(
        &mut self,
        elements: &[Result<BatchElement, String>],
        skip: bool,
        into: Option<i64>,
    ) -> Result<BatchResult, WriteError> {
        let model = self.output.document.sfc_model.as_ref().unwrap();
        let target = if let Some(id) = into {
            let definition_id = model
                .sfig_references
                .iter()
                .find(|r| r.placement_id == id)
                .map_or(id, |r| r.definition_id);
            Some(
                model
                    .sfig_definitions
                    .iter()
                    .find(|d| d.entity_id == definition_id)
                    .ok_or_else(|| {
                        error("into must name an ordinary figure definition or placement")
                    })?
                    .entity_id,
            )
        } else {
            None
        };
        let mut names: HashMap<_, _> = model
            .sfig_definitions
            .iter()
            .map(|d| (d.name.clone(), d.kind_flag))
            .chain(
                model
                    .attribute_attachments
                    .iter()
                    .map(|a| (a.name.clone(), 3)),
            )
            .collect();
        let mut next = self.next_id()?;
        let definition_names: HashMap<_, _> = model
            .sfig_definitions
            .iter()
            .map(|d| (d.entity_id, d.name.as_str()))
            .collect();
        let definition_positions: HashMap<_, _> = self
            .output
            .document
            .entities
            .iter()
            .enumerate()
            .filter_map(|(position, entity)| {
                definition_names
                    .get(&entity.id)
                    .map(|name| (*name, position))
            })
            .collect();
        let target_position = target.map(|id| {
            self.output
                .document
                .entities
                .iter()
                .position(|e| e.id == id)
                .unwrap()
        });
        let mut curve_code = model.composite_curve_definitions.len() as i64;
        let mut definitions = Vec::new();
        let mut roots = Vec::new();
        let mut ids = Vec::with_capacity(elements.len());
        let mut rejected = Vec::new();
        let mut owners = HashMap::new();
        for (index, element) in elements.iter().enumerate() {
            let mut fragment = Fragment {
                next,
                curve_code,
                definitions: Vec::new(),
                own_names: HashMap::new(),
                known_names: &names,
                tables: &model.code_tables,
                target_p21: self.target_p21,
            };
            let mut direct = Vec::new();
            let built = element.as_ref().map_err(|s| error(s.clone())).and_then(|e| {
                if let (Some(position), BatchElement::Placement { name, .. }) = (target_position, e) {
                    if definition_positions.get(name.as_str()).is_some_and(|p| *p >= position) {
                        return Err(error("into would create a forward or recursive part reference"));
                    }
                }
                fragment.build(e, 0, &mut direct)
            }).and_then(|id| {
                if target.is_some() && !fragment.definitions.is_empty() {
                    Err(error("into accepts leaf features and existing part placements; use an inline figure for new definitions"))
                } else { Ok(id) }
            });
            match built {
                Ok(id) => {
                    for entity in fragment.definitions.iter().chain(&direct) {
                        owners.insert(entity.id, index);
                    }
                    next = fragment.next;
                    curve_code = fragment.curve_code;
                    definitions.extend(fragment.definitions);
                    roots.extend(direct);
                    names.extend(fragment.own_names);
                    ids.push(Some(id));
                }
                Err(e) => {
                    if !skip {
                        return Err(error(format!("Element {index}: {e}")));
                    }
                    rejected.push((index, e.to_string()));
                    ids.push(None);
                }
            }
        }
        if !definitions.is_empty() || !roots.is_empty() {
            let mut candidate = self.output.document.clone();
            if let Some(target) = target {
                let position = candidate
                    .entities
                    .iter()
                    .position(|e| e.id == target)
                    .unwrap();
                candidate.entities.splice(position..position, roots);
            } else {
                // Keep existing definition order (composite codes are positional),
                // and detach sheet items so new markers cannot capture them.
                let sheet = model.sheet.as_ref().unwrap();
                let direct: HashSet<_> = sheet.component_ids.iter().copied().collect();
                let mut old_roots = Vec::new();
                candidate.entities.retain(|e| {
                    if direct.contains(&e.id) {
                        old_roots.push(e.clone());
                        false
                    } else {
                        true
                    }
                });
                let position = candidate
                    .entities
                    .iter()
                    .position(|e| e.id == sheet.entity_id)
                    .unwrap();
                definitions.extend(old_roots);
                definitions.extend(roots);
                candidate.entities.splice(position..position, definitions);
            }
            self.commit(candidate).map_err(|e| {
                // Document-level ownership/forward-reference failures remain atomic.
                // Every newly generated entity carries its originating input index.
                let owner =
                    e.0.split('#')
                        .skip(1)
                        .find_map(|v| {
                            v.chars()
                                .take_while(char::is_ascii_digit)
                                .collect::<String>()
                                .parse::<i64>()
                                .ok()
                                .and_then(|id| owners.get(&id))
                        })
                        .copied();
                error(format!(
                    "Element {}: {e}",
                    owner.map_or_else(|| "batch".into(), |i| i.to_string())
                ))
            })?;
        }
        Ok(BatchResult { ids, rejected })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn doc() -> SfcDocument {
        SfcDocument::new("drawing.sfc", "drawing", 297, 210, "2026-10-08").unwrap()
    }
    fn circle(radius: f64) -> BatchElement {
        BatchElement::Feature((
            "circle".into(),
            BTreeMap::from([("radius".into(), Value::Real(radius))]),
        ))
    }
    #[test]
    fn skip_rejects_invalid_fragment_without_leaking_its_name_or_ids() {
        let mut document = doc();
        let part = |radius| BatchElement::Figure {
            name: "part".into(),
            kind: 4,
            elements: vec![circle(radius)],
            placements: vec![Placement::default()],
        };
        let result = document
            .extend_structured(
                &[Ok(part(-1.0)), Ok(part(2.0)), Ok(circle(3.0))],
                true,
                None,
            )
            .unwrap();
        assert_eq!(result.ids[0], None);
        assert_eq!(result.rejected.len(), 1);
        assert!(result.rejected[0].1.contains("Entity #7:"));
        assert_eq!(
            document
                .output
                .document
                .sfc_model
                .as_ref()
                .unwrap()
                .sfig_definitions
                .len(),
            1
        );
        assert!(document.output.warnings.is_empty());
    }
    #[test]
    fn raise_is_atomic_and_preserves_index_for_semantic_failure() {
        let mut document = doc();
        let before = document.output.clone();
        let message = document
            .extend_structured(&[Ok(circle(1.0)), Ok(circle(-1.0))], false, None)
            .unwrap_err()
            .to_string();
        assert!(message.starts_with("Element 1: Entity #8:"));
        assert_eq!(document.output, before);
    }
    #[test]
    fn p21_target_checks_new_features_before_commit() {
        let mut document = doc();
        document.target_p21 = true;
        let before = document.output.clone();
        let e = BatchElement::Feature(("line".into(), BTreeMap::new()));
        let message = document
            .extend_structured(&[Ok(e)], false, None)
            .unwrap_err()
            .to_string();
        assert!(message.contains("Element 0: Entity #7: P21 cannot emit a zero-length line"));
        assert_eq!(document.output, before);
    }
}
