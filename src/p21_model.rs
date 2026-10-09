//! Resolve AP202 sheet and SXF subfigure metadata without replacing the STEP graph.

use std::collections::{HashMap, HashSet};

use crate::model::*;
use crate::parser::{parse_sfc_attribute_mechanism, Parser};

struct Graph<'a> {
    records: HashMap<i64, HashMap<String, &'a Record>>,
    order: Vec<i64>,
}

impl<'a> Graph<'a> {
    fn new(entities: &'a [EntityInstance]) -> Self {
        Self {
            records: entities
                .iter()
                .map(|entity| {
                    let records = match &entity.body {
                        EntityBody::Simple(record) => std::slice::from_ref(record),
                        EntityBody::Complex(records) => records.as_slice(),
                    };
                    (
                        entity.id,
                        records
                            .iter()
                            .map(|record| (record.keyword.to_ascii_uppercase(), record))
                            .collect(),
                    )
                })
                .collect(),
            order: entities.iter().map(|entity| entity.id).collect(),
        }
    }

    fn get(&self, id: i64, keyword: &str) -> Option<&'a [Value]> {
        Some(self.records.get(&id)?.get(keyword)?.parameters.as_slice())
    }

    fn find(&self, keyword: &'static str) -> impl Iterator<Item = (i64, &'a [Value])> + '_ {
        self.order
            .iter()
            .filter_map(move |id| self.get(*id, keyword).map(|params| (*id, params)))
    }

    // Include callout children and styled targets so the common model can identify
    // every rendered source_id. Subfigure contents stay behind definition references.
    fn components(&self, value: &Value) -> Vec<i64> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut stack: Vec<_> = references(value).into_iter().rev().collect();
        while let Some(id) = stack.pop() {
            if !seen.insert(id)
                || self.get(id, "AXIS2_PLACEMENT_2D").is_some()
                || self.get(id, "PLANAR_BOX").is_some()
            {
                continue;
            }
            out.push(id);
            if let Some(params) = self.get(id, "DRAUGHTING_CALLOUT") {
                if let Some(items) = params.first() {
                    stack.extend(references(items).into_iter().rev());
                }
            }
            if let Some(target) = self.styled_target(id) {
                stack.push(target);
            }
        }
        out
    }

    fn styled_target(&self, id: i64) -> Option<i64> {
        let params = self.get(id, "STYLED_ITEM")?;
        reference(params.last()?)
    }

    fn mapped(&self, id: i64) -> Option<&'a [Value]> {
        self.get(id, "MAPPED_ITEM").or_else(|| {
            self.styled_target(id)
                .and_then(|target| self.get(target, "MAPPED_ITEM"))
        })
    }

    fn axis(&self, id: Option<i64>) -> Option<[f64; 6]> {
        let Some(id) = id else {
            return Some([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        };
        let axis = self.get(id, "AXIS2_PLACEMENT_2D")?;
        let point = self.get(reference(axis.get(1)?)?, "CARTESIAN_POINT")?;
        let xy = point.get(1)?.as_f64_list()?;
        let (x, y) = (*xy.first()?, *xy.get(1)?);
        let (dx, dy) = if let Some(direction) = axis.get(2).and_then(reference) {
            let direction = self.get(direction, "DIRECTION")?;
            let xy = direction.get(1)?.as_f64_list()?;
            (*xy.first()?, *xy.get(1)?)
        } else {
            (1.0, 0.0)
        };
        let length = dx.hypot(dy);
        if length == 0.0 || ![x, y, length].iter().all(|v| v.is_finite()) {
            return None;
        }
        Some([dx / length, dy / length, -dy / length, dx / length, x, y])
    }

    fn placement(&self, params: &[Value], name: &str) -> Option<(i64, SfigLocateFeature)> {
        let offset = usize::from(matches!(params.first(), Some(Value::String(_))));
        let map = self.get(reference(params.get(offset)?)?, "SYMBOL_REPRESENTATION_MAP")?;
        let definition = reference(map.get(1)?)?;
        let source = self.axis(map.first().and_then(reference))?;
        let target = self.get(reference(params.get(offset + 1)?)?, "SYMBOL_TARGET")?;
        let mut target_axis = self.axis(target.get(1).and_then(reference))?;
        let rx = target.get(2)?.as_f64()?;
        let ry = target.get(3).map_or(Some(rx), Value::as_f64)?;
        target_axis[0] *= rx;
        target_axis[1] *= rx;
        target_axis[2] *= ry;
        target_axis[3] *= ry;
        // The source axis is orthonormal, so its inverse is its transpose.
        let inverse = [
            source[0],
            source[2],
            source[1],
            source[3],
            -source[0] * source[4] - source[1] * source[5],
            -source[2] * source[4] - source[3] * source[5],
        ];
        let [a, b, c, d, e, f] = compose(target_axis, inverse);
        let ratio_x = a.hypot(b);
        let ratio_y = c.hypot(d);
        if ![a, b, c, d, e, f, ratio_x, ratio_y]
            .iter()
            .all(|v| v.is_finite())
            || ratio_x == 0.0
            || ratio_y == 0.0
            || (a * c + b * d).abs() > 1e-9 * ratio_x * ratio_y
        {
            // A sheared mapping cannot be represented by SFC angle/X/Y scale.
            return None;
        }
        Some((
            definition,
            SfigLocateFeature {
                style: CommonStyle::default(),
                name: name.into(),
                position: Point2 { x: e, y: f },
                angle_deg: b.atan2(a).to_degrees().rem_euclid(360.0),
                ratio_x,
                ratio_y: ratio_y.copysign(a * d - b * c),
            },
        ))
    }

    fn sheet(&self, id: i64, params: &[Value]) -> Option<DrawingSheetFeature> {
        let paper_name = decoded(params.first()?)?;
        let mut box_ids = references(params.get(1)?);
        for (_, size) in self.find("PRESENTATION_SIZE") {
            if size.first().and_then(reference) == Some(id) {
                box_ids.extend(size.get(1).and_then(reference));
            }
        }
        let dimensions = box_ids.into_iter().find_map(|box_id| {
            let values = self.get(box_id, "PLANAR_BOX")?;
            let width = values.get(1)?.as_f64()?;
            let height = values.get(2)?.as_f64()?;
            (width > 0.0 && height > 0.0 && width.is_finite() && height.is_finite())
                .then_some((width, height))
        });
        let paper = standard_paper(&paper_name);
        let (sheet_type, orientation) = paper.or_else(|| {
            let (width, height) = dimensions?;
            let kind = if paper_name.eq_ignore_ascii_case("FREE") {
                9
            } else {
                STANDARD_PAPERS
                    .iter()
                    .position(|(short, long)| {
                        (width.min(height) - short).abs() <= 0.5
                            && (width.max(height) - long).abs() <= 0.5
                    })
                    .map_or(9, |n| n as i64)
            };
            Some((kind, i64::from(width >= height)))
        })?;
        let (width, height) = dimensions.or_else(|| {
            let (short, long) = *STANDARD_PAPERS.get(sheet_type as usize)?;
            Some(if orientation == 0 {
                (short, long)
            } else {
                (long, short)
            })
        })?;
        // Titles are associated with a drawing revision, not the paper identifier.
        let revision = self
            .find("DRAWING_SHEET_REVISION_USAGE")
            .find_map(|(_, usage)| {
                (usage.first().and_then(reference) == Some(id))
                    .then(|| usage.get(1).and_then(reference))
                    .flatten()
            });
        let name = self
            .find("DRAUGHTING_TITLE")
            .find_map(|(_, title)| {
                if let Some(revision) = revision {
                    if !references(title.first()?).contains(&revision) {
                        return None;
                    }
                }
                decoded(title.get(2)?)
            })
            .unwrap_or_default();
        Some(DrawingSheetFeature {
            name,
            sheet_type,
            orientation,
            free_x_mm: width,
            free_y_mm: height,
        })
    }
}

impl Parser<'_> {
    pub(crate) fn build_p21_model(&mut self, document: &mut ParsedDocument) {
        let graph = Graph::new(&document.entities);
        let mut model = SfcModel::default();
        let mut typed = HashMap::new();
        let mut names = HashMap::new();
        for (id, params) in graph.find("DRAUGHTING_SUBFIGURE_REPRESENTATION") {
            let Some((name, kind_flag)) = params.first().and_then(decoded).and_then(subfigure_name)
            else {
                continue;
            };
            let Some(items) = params.get(1) else {
                self.push_warning(
                    "p21-subfigure-model",
                    format!("Subfigure #{id} has no items"),
                );
                continue;
            };
            let component_ids = graph.components(items);
            match parse_sfc_attribute_mechanism(&name) {
                Ok(Some(mechanism)) if kind_flag == 3 => {
                    model.attribute_attachments.push(SfcAttributeAttachment {
                        definition_id: id,
                        name: name.clone(),
                        kind_flag,
                        component_ids,
                        placement_ids: Vec::new(),
                        mechanism,
                        resolved_attribute_file_name: None,
                    });
                }
                Ok(Some(_)) => {
                    self.push_warning(
                        "p21-subfigure-model",
                        format!("Attribute subfigure #{id} must use drawing-group kind 3"),
                    );
                    continue;
                }
                Err(reason) => {
                    self.push_warning("p21-subfigure-model", format!("Subfigure #{id}: {reason}"));
                    continue;
                }
                _ => model.sfig_definitions.push(SfcSfigDefinition {
                    entity_id: id,
                    name: name.clone(),
                    kind_flag,
                    component_ids,
                }),
            }
            names.insert(id, name.clone());
            typed.insert(
                id,
                TypedFeature::SfigOrg(SfigOrgFeature { name, kind_flag }),
            );
        }
        let wrapped: HashSet<_> = graph
            .order
            .iter()
            .filter_map(|id| graph.styled_target(*id))
            .collect();
        let direct: HashSet<_> = graph
            .find("DRAUGHTING_SUBFIGURE_REPRESENTATION")
            .chain(graph.find("DRAWING_SHEET_REVISION"))
            .filter_map(|(_, params)| params.get(1))
            .chain(
                graph
                    .find("DRAUGHTING_CALLOUT")
                    .filter_map(|(_, params)| params.first()),
            )
            .flat_map(references)
            .collect();
        for id in &graph.order {
            if wrapped.contains(id) && !direct.contains(id) {
                continue;
            }
            let Some(params) = graph.mapped(*id) else {
                continue;
            };
            let offset = usize::from(matches!(params.first(), Some(Value::String(_))));
            let definition = params.get(offset).and_then(reference).and_then(|map_id| {
                graph
                    .get(map_id, "SYMBOL_REPRESENTATION_MAP")?
                    .get(1)
                    .and_then(reference)
            });
            let Some(name) = definition.and_then(|id| names.get(&id)) else {
                continue;
            };
            let Some((definition_id, placement)) = graph.placement(params, name) else {
                self.push_warning(
                    "p21-placement-model",
                    format!(
                        "Placement #{id} has an invalid or sheared mapping; omitted from model"
                    ),
                );
                continue;
            };
            if let Some(attachment) = model
                .attribute_attachments
                .iter_mut()
                .find(|a| a.definition_id == definition_id)
            {
                attachment.placement_ids.push(*id);
            } else {
                model.sfig_references.push(SfcSfigReference {
                    placement_id: *id,
                    definition_id,
                });
            }
            typed.insert(*id, TypedFeature::SfigLocate(placement));
        }
        if let Some((id, params)) = graph.find("DRAWING_SHEET_REVISION").next() {
            if let Some(sheet) = graph.sheet(id, params) {
                model.sheet = Some(SfcSheetModel {
                    entity_id: id,
                    component_ids: graph.components(&params[1]),
                });
                typed.insert(id, TypedFeature::DrawingSheet(sheet));
            } else {
                self.push_warning(
                    "p21-sheet-model",
                    format!("Sheet #{id} has no valid paper size"),
                );
            }
            if graph.find("DRAWING_SHEET_REVISION").nth(1).is_some() {
                self.push_warning(
                    "p21-sheet-model",
                    "Multiple P21 sheets; model uses the first sheet".into(),
                );
            }
        }
        document.typed_features = graph
            .order
            .iter()
            .filter_map(|id| {
                let feature = typed.remove(id)?;
                let keyword = match &feature {
                    TypedFeature::DrawingSheet(_) => "drawing_sheet_feature",
                    TypedFeature::SfigOrg(_) => "sfig_org_feature",
                    TypedFeature::SfigLocate(_) => "sfig_locate_feature",
                    _ => unreachable!(),
                };
                Some(TypedFeatureInstance {
                    id: *id,
                    keyword: keyword.into(),
                    feature,
                })
            })
            .collect();
        if !document.typed_features.is_empty() {
            document.sfc_model = Some(model);
        }
    }
}

const STANDARD_PAPERS: [(f64, f64); 5] = [
    (841.0, 1189.0),
    (594.0, 841.0),
    (420.0, 594.0),
    (297.0, 420.0),
    (210.0, 297.0),
];

fn standard_paper(name: &str) -> Option<(i64, i64)> {
    let name = name.to_ascii_lowercase();
    let (paper, orientation) = name.split_once('_')?;
    let code = paper.strip_prefix('a')?.parse::<i64>().ok()?;
    if !(0..=4).contains(&code) {
        return None;
    }
    Some((
        code,
        match orientation {
            "vertical" => 0,
            "horizontal" => 1,
            _ => return None,
        },
    ))
}

fn subfigure_name(name: String) -> Option<(String, i64)> {
    for (prefix, kind) in [
        ("$$SXF_FM_", 1),
        ("$$SXF_FG_", 2),
        ("$$SXF_G_", 3),
        ("$$SXF_P_", 4),
    ] {
        if let Some(value) = name.strip_prefix(prefix) {
            return Some((value.into(), kind));
        }
    }
    None
}

fn reference(value: &Value) -> Option<i64> {
    if let Value::Reference(id) = value {
        Some(*id)
    } else {
        None
    }
}

fn references(value: &Value) -> Vec<i64> {
    match value {
        Value::Reference(id) => vec![*id],
        Value::List(values) => values.iter().flat_map(references).collect(),
        _ => Vec::new(),
    }
}

fn decoded(value: &Value) -> Option<String> {
    value.as_string().map(|name| decode_step_string(&name))
}

fn compose([a, b, c, d, e, f]: [f64; 6], [g, h, i, j, k, l]: [f64; 6]) -> [f64; 6] {
    [
        a * g + c * h,
        b * g + d * h,
        a * i + c * j,
        b * i + d * j,
        a * k + c * l + e,
        b * k + d * l + f,
    ]
}

fn decode_step_string(value: &str) -> String {
    let mut output = String::new();
    let mut rest = value;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("\\\\") {
            output.push('\\');
            rest = after;
            continue;
        }
        let width = if rest.starts_with("\\X2\\") {
            4
        } else if rest.starts_with("\\X4\\") {
            8
        } else {
            0
        };
        if width != 0 {
            if let Some(end) = rest[4..].find("\\X0\\") {
                let payload = &rest[4..4 + end];
                let decoded = (|| {
                    if !payload.is_ascii() || !payload.len().is_multiple_of(width) {
                        return None;
                    }
                    let values = payload
                        .as_bytes()
                        .chunks(width)
                        .map(|chunk| u32::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok())
                        .collect::<Option<Vec<_>>>()?;
                    if width == 4 {
                        String::from_utf16(&values.iter().map(|v| *v as u16).collect::<Vec<_>>())
                            .ok()
                    } else {
                        values
                            .into_iter()
                            .map(char::from_u32)
                            .collect::<Option<String>>()
                    }
                })();
                if let Some(decoded) = decoded {
                    output.push_str(&decoded);
                    rest = &rest[8 + end..];
                    continue;
                }
            }
        }
        let ch = rest.chars().next().unwrap();
        output.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_p21_text;

    fn parse(records: &str) -> ParseOutput {
        parse_p21_text(
            &format!(
                "ISO-10303-21;HEADER;FILE_DESCRIPTION(('SCADEC level2 AP202_mode'),'2;1');\
             FILE_NAME('test.p21','',(),(),'ezsxf$$3.1','','');\
             FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING'));ENDSEC;DATA;{records}\
             ENDSEC;END-ISO-10303-21;"
            ),
            true,
        )
        .unwrap()
    }

    const RECORDS: &str = r"
        #1=CARTESIAN_POINT('',(2.,3.));
        #2=DIRECTION('',(0.,5.));
        #3=AXIS2_PLACEMENT_2D('',#1,#2);
        #4=CARTESIAN_POINT('',(10.,20.));
        #5=AXIS2_PLACEMENT_2D('',#4,$);
        #6=DRAUGHTING_SUBFIGURE_REPRESENTATION('$$SXF_FG_\X2\6E2C5730\X0\',(#3),$);
        #7=SYMBOL_REPRESENTATION_MAP(#3,#6);
        #8=SYMBOL_TARGET('',#5,2.,2.);
        #9=(MAPPED_ITEM(#7,#8) REPRESENTATION_ITEM(''));
        #10=(ANNOTATION_SUBFIGURE_OCCURRENCE() STYLED_ITEM((),#9));
        #11=PLANAR_BOX('',420.,297.,#5);
        #12=DRAWING_SHEET_REVISION('A3_horizontal',(#10,#11),$,'01');
        #13=DRAUGHTING_DRAWING_REVISION('01',$,$);
        #14=DRAWING_SHEET_REVISION_USAGE(#12,#13,'01');
        #15=DRAUGHTING_TITLE((#13),'JAPANESE','\X2\56F39762\X0\');
        #16=DRAUGHTING_TITLE((),'', 'unrelated');
    ";

    #[test]
    fn resolves_sheet_geodetic_definition_and_wrapped_mapping_by_entity_ids() {
        let output = parse(RECORDS);
        assert!(output.warnings.is_empty());
        let model = output.document.sfc_model.as_ref().unwrap();
        assert_eq!(model.sheet.as_ref().unwrap().component_ids, [10, 9]);
        assert_eq!(model.sfig_definitions[0].name, "測地");
        assert_eq!(model.sfig_definitions[0].kind_flag, 2);
        assert!(model.sfig_definitions[0].component_ids.is_empty());
        assert_eq!(
            model.sfig_references,
            [SfcSfigReference {
                placement_id: 10,
                definition_id: 6
            }]
        );
        let typed = &output.document.typed_features;
        assert_eq!(typed.len(), 3);
        let TypedFeature::SfigLocate(placement) = &typed[1].feature else {
            panic!()
        };
        assert_eq!(placement.position, Point2 { x: 4.0, y: 24.0 });
        assert_eq!(placement.angle_deg, 270.0);
        assert_eq!((placement.ratio_x, placement.ratio_y), (2.0, 2.0));
        let TypedFeature::DrawingSheet(sheet) = &typed[2].feature else {
            panic!()
        };
        assert_eq!(sheet.name, "図面");
        assert_eq!((sheet.sheet_type, sheet.orientation), (3, 1));
        // Structural names are decoded, but the source STEP graph remains unchanged.
        let EntityBody::Simple(raw) = &output.document.entities[5].body else {
            panic!()
        };
        assert_eq!(
            raw.parameters[0],
            Value::String(r"$$SXF_FG_\X2\6E2C5730\X0\".into())
        );
    }

    #[test]
    fn reads_fractional_free_size_via_presentation_size() {
        let output = parse(
            &RECORDS
                .replace("420.,297.", "450.25,300.75")
                .replace("'A3_horizontal',(#10,#11)", "'FREE',(#10)")
                .replace(
                    "#16=DRAUGHTING_TITLE",
                    "#17=PRESENTATION_SIZE(#12,#11);#16=DRAUGHTING_TITLE",
                ),
        );
        let TypedFeature::DrawingSheet(sheet) = &output.document.typed_features[2].feature else {
            panic!()
        };
        assert_eq!(sheet.sheet_type, 9);
        assert_eq!((sheet.free_x_mm, sheet.free_y_mm), (450.25, 300.75));
        assert!(output.warnings.is_empty());
    }

    #[test]
    fn explicit_free_paper_is_not_reclassified_as_standard() {
        let output = parse(&RECORDS.replace("A3_horizontal", "FREE"));
        let TypedFeature::DrawingSheet(sheet) = &output.document.typed_features[2].feature else {
            panic!()
        };
        assert_eq!(sheet.sheet_type, 9);
        assert_eq!((sheet.free_x_mm, sheet.free_y_mm), (420.0, 297.0));
    }

    #[test]
    fn generic_graph_keeps_empty_typed_view_and_no_model() {
        let output = parse("#1=CARTESIAN_POINT('',(0.,1.));");
        assert!(output.document.typed_features.is_empty());
        assert!(output.document.sfc_model.is_none());
        assert!(output.warnings.is_empty());
    }

    #[test]
    fn multiple_sheets_select_first_and_report_limit() {
        let output = parse(&format!(
            "{RECORDS}#17=DRAWING_SHEET_REVISION('A4_vertical',(),$,'02');"
        ));
        assert_eq!(
            output.document.sfc_model.unwrap().sheet.unwrap().entity_id,
            12
        );
        assert_eq!(output.warnings.len(), 1);
        assert_eq!(output.warnings[0].code, "p21-sheet-model");
    }

    #[test]
    fn attributes_are_separate_from_ordinary_groups() {
        let output = parse(&RECORDS.replace(
            r"$$SXF_FG_\X2\6E2C5730\X0\",
            r"$$SXF_G_$$ATRU$$42$$\X2\7A2E985E\X0\$$\X2\67506599\X0\$$\X2\92FC\X0\$$STR",
        ));
        let model = output.document.sfc_model.unwrap();
        assert!(model.sfig_definitions.is_empty());
        assert!(model.sfig_references.is_empty());
        let attachment = &model.attribute_attachments[0];
        assert_eq!(attachment.placement_ids, [10]);
        let SfcAttributeMechanism::SingleAttribute {
            attribute_name,
            attribute_value,
            ..
        } = &attachment.mechanism
        else {
            panic!()
        };
        assert_eq!(attribute_name.as_deref(), Some("材料"));
        assert_eq!(attribute_value.as_deref(), Some("鋼"));
    }

    #[test]
    fn flags_sheared_mapping_without_removing_raw_records() {
        let records = RECORDS
            .replace("(0.,5.)", "(1.,1.)")
            .replace("#5,2.,2.", "#5,2.,1.");
        let output = parse(&records);
        assert!(output
            .document
            .sfc_model
            .unwrap()
            .sfig_references
            .is_empty());
        assert_eq!(output.document.entities.len(), 16);
        assert!(output
            .warnings
            .iter()
            .any(|w| w.code == "p21-placement-model"));
    }

    #[test]
    fn unicode_decoder_preserves_literals_and_invalid_escapes() {
        assert_eq!(
            decode_step_string(r"a\X2\56F39762D83DDE00\X0\b\X4\0001F600\X0\"),
            "a図面😀b😀"
        );
        assert_eq!(decode_step_string(r"\\X2\\56F3\\X0\\"), r"\X2\56F3\X0\");
        assert_eq!(decode_step_string(r"\X2\D800\X0\"), r"\X2\D800\X0\");
        assert_eq!(decode_step_string(r"\X4\00110000\X0\"), r"\X4\00110000\X0\");
        assert_eq!(decode_step_string(r"\X2\XYZ\X0\"), r"\X2\XYZ\X0\");
        assert_eq!(decode_step_string(r"\X2\56F3"), r"\X2\56F3");
    }
}
