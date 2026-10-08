//! SXF AP202 output from the validated, editable SFC model (AP202 subset §3).
//! Preserves AP202 dimensions, fills, subfigures and attribute group names.
//! External dependencies are allowed only by the validated bundle writer.
use crate::editor::{record, string};
use crate::model::*;
use crate::writer::{serialize_sfc, SfcWriteOptions, WriteError};
use std::collections::BTreeMap;
use std::fmt::Write;

#[path = "p21_annotations.rs"]
mod annotations;
#[path = "p21_hatches.rs"]
mod hatches;

fn error(message: impl Into<String>) -> WriteError {
    WriteError(message.into())
}
fn real(value: f64) -> Value {
    Value::Real(value)
}
fn reference(id: i64) -> Value {
    Value::Reference(id)
}
fn references(ids: &[i64]) -> Value {
    Value::List(ids.iter().copied().map(reference).collect())
}
fn enumeration(value: &str) -> Value {
    Value::Enum(value.into())
}
fn measure(keyword: &str, value: f64) -> Value {
    Value::Typed {
        keyword: keyword.into(),
        parameters: vec![real(value)],
    }
}

#[derive(Default)]
struct Graph {
    entities: Vec<EntityInstance>,
    items: BTreeMap<i64, i64>,
    maps: BTreeMap<i64, i64>,
    curves: BTreeMap<i64, i64>,
    colors: BTreeMap<i64, i64>,
    line_types: BTreeMap<i64, i64>,
    widths: BTreeMap<i64, i64>,
    fonts: BTreeMap<i64, i64>,
    styles: BTreeMap<(i64, i64, i64), i64>,
    layer_items: BTreeMap<i64, Vec<i64>>,
    context: i64,
    length_unit: i64,
    source: i64,
}

impl Graph {
    fn predefined_color(&mut self, name: &str) -> i64 {
        let rgb = match name {
            "deeppink" => Some([0.75, 0.0, 0.5]),
            "brown" => Some([0.75, 0.5, 0.25]),
            "orange" => Some([1.0, 0.5, 0.0]),
            "lightgreen" => Some([0.5, 0.75, 0.5]),
            "lightblue" => Some([0.0, 0.5, 1.0]),
            "lavender" => Some([0.5, 0.25, 1.0]),
            "lightgray" => Some([0.75, 0.75, 0.75]),
            "darkgray" => Some([0.5, 0.5, 0.5]),
            _ => None,
        };
        if let Some(rgb) = rgb {
            self.add(
                "COLOUR_RGB",
                vec![
                    string(format!("$$SXF_{name}")),
                    real(rgb[0]),
                    real(rgb[1]),
                    real(rgb[2]),
                ],
            )
        } else {
            self.add("DRAUGHTING_PRE_DEFINED_COLOUR", vec![string(name)])
        }
    }
    fn push(&mut self, body: EntityBody) -> i64 {
        let id = self.entities.len() as i64 + 1;
        self.entities.push(EntityInstance {
            id,
            sfc_version: None,
            body,
        });
        id
    }
    fn add(&mut self, keyword: &str, parameters: Vec<Value>) -> i64 {
        self.push(EntityBody::Simple(record(keyword, parameters)))
    }
    fn complex(&mut self, mut records: Vec<Record>) -> i64 {
        records.sort_by(|a, b| a.keyword.cmp(&b.keyword));
        self.push(EntityBody::Complex(records))
    }
    fn point(&mut self, p: &Point2) -> i64 {
        self.add(
            "CARTESIAN_POINT",
            vec![string(""), Value::List(vec![real(p.x), real(p.y)])],
        )
    }
    fn axis(&mut self, p: &Point2, angle_deg: f64) -> i64 {
        let point = self.point(p);
        let angle = angle_deg.to_radians();
        let direction = self.add(
            "DIRECTION",
            vec![
                string(""),
                Value::List(vec![real(angle.cos()), real(angle.sin())]),
            ],
        );
        self.add(
            "AXIS2_PLACEMENT_2D",
            vec![string(""), reference(point), reference(direction)],
        )
    }
    fn occurrence(&mut self, kind: &str, name: &str, target: i64, style: i64, layer: i64) -> i64 {
        let mut records = vec![
            record("ANNOTATION_OCCURRENCE", vec![]),
            record(kind, vec![]),
            record("DRAUGHTING_ANNOTATION_OCCURRENCE", vec![]),
            record("GEOMETRIC_REPRESENTATION_ITEM", vec![]),
            record("REPRESENTATION_ITEM", vec![string(name)]),
            record("STYLED_ITEM", vec![references(&[style]), reference(target)]),
        ];
        if kind == "ANNOTATION_SUBFIGURE_OCCURRENCE" {
            records.push(record("ANNOTATION_SYMBOL_OCCURRENCE", vec![]));
        }
        let id = self.complex(records);
        self.layer_items.entry(layer).or_default().push(id);
        id
    }
    fn curve_style(&mut self, style: &CommonStyle) -> Result<i64, WriteError> {
        let key = (
            style.color_code.filter(|code| *code != 0).unwrap_or(1),
            style.line_type_code.filter(|code| *code != 0).unwrap_or(1),
            style.line_width_code.filter(|code| *code != 0).unwrap_or(1),
        );
        if let Some(id) = self.styles.get(&key) {
            return Ok(*id);
        }
        let color = lookup(&self.colors, key.0, "colour")?;
        let font = lookup(&self.line_types, key.1, "line type")?;
        let width = lookup(&self.widths, key.2, "line width")?;
        let curve_style = self.add(
            "CURVE_STYLE",
            vec![
                string(""),
                reference(font),
                reference(width),
                reference(color),
            ],
        );
        let assignment = self.add(
            "PRESENTATION_STYLE_ASSIGNMENT",
            vec![references(&[curve_style])],
        );
        self.styles.insert(key, assignment);
        Ok(assignment)
    }
    fn curve(&mut self, feature: &TypedFeature) -> Result<i64, WriteError> {
        Ok(match feature {
            TypedFeature::Line(line) => {
                let start = self.point(&line.start);
                let end = self.point(&line.end);
                let dx = line.end.x - line.start.x;
                let dy = line.end.y - line.start.y;
                let length = dx.hypot(dy);
                if length == 0.0 {
                    return Err(error("P21 cannot emit a zero-length line"));
                }
                let direction = self.add(
                    "DIRECTION",
                    vec![
                        string(""),
                        Value::List(vec![real(dx / length), real(dy / length)]),
                    ],
                );
                let vector = self.add("VECTOR", vec![string(""), reference(direction), real(1.0)]);
                let origin = self.point(&Point2 { x: 0.0, y: 0.0 });
                let basis = self.add(
                    "LINE",
                    vec![string(""), reference(origin), reference(vector)],
                );
                self.add(
                    "TRIMMED_CURVE",
                    vec![
                        string(""),
                        reference(basis),
                        references(&[start]),
                        references(&[end]),
                        enumeration("T"),
                        enumeration("CARTESIAN"),
                    ],
                )
            }
            TypedFeature::Polyline(line) => {
                let points: Vec<_> = line.points.iter().map(|point| self.point(point)).collect();
                self.add("POLYLINE", vec![string(""), references(&points)])
            }
            TypedFeature::Circle(circle) => {
                let axis = self.axis(&circle.center, 0.0);
                self.add(
                    "CIRCLE",
                    vec![string(""), reference(axis), real(circle.radius)],
                )
            }
            TypedFeature::Arc(arc) => {
                let axis = self.axis(&arc.center, 0.0);
                let circle = self.add(
                    "CIRCLE",
                    vec![string(""), reference(axis), real(arc.radius)],
                );
                self.add(
                    "TRIMMED_CURVE",
                    vec![
                        string(""),
                        reference(circle),
                        Value::List(vec![measure(
                            "PARAMETER_VALUE",
                            arc.start_angle_deg.to_radians(),
                        )]),
                        Value::List(vec![measure(
                            "PARAMETER_VALUE",
                            arc.end_angle_deg.to_radians(),
                        )]),
                        enumeration(if arc.direction_flag == 0 { "T" } else { "F" }),
                        enumeration("PARAMETER"),
                    ],
                )
            }
            TypedFeature::Ellipse(ellipse) => {
                let axis = self.axis(&ellipse.center, ellipse.rotation_angle_deg);
                self.add(
                    "ELLIPSE",
                    vec![
                        string(""),
                        reference(axis),
                        real(ellipse.radius_x),
                        real(ellipse.radius_y),
                    ],
                )
            }
            TypedFeature::EllipseArc(arc) => {
                let axis = self.axis(&arc.center, arc.rotation_angle_deg);
                let ellipse = self.add(
                    "ELLIPSE",
                    vec![
                        string(""),
                        reference(axis),
                        real(arc.radius_x),
                        real(arc.radius_y),
                    ],
                );
                self.add(
                    "TRIMMED_CURVE",
                    vec![
                        string(""),
                        reference(ellipse),
                        Value::List(vec![measure(
                            "PARAMETER_VALUE",
                            arc.start_angle_deg.to_radians(),
                        )]),
                        Value::List(vec![measure(
                            "PARAMETER_VALUE",
                            arc.end_angle_deg.to_radians(),
                        )]),
                        enumeration(if arc.direction_flag == 0 { "T" } else { "F" }),
                        enumeration("PARAMETER"),
                    ],
                )
            }
            _ => return Err(error("Unsupported P21 curve")),
        })
    }
    fn text(&mut self, text: &TextFeature) -> Result<i64, WriteError> {
        let color = lookup(
            &self.colors,
            text.style.color_code.filter(|code| *code != 0).unwrap_or(1),
            "colour",
        )?;
        let font = lookup(&self.fonts, text.style.font_code.unwrap_or(1), "text font")?;
        let extent = self.add(
            "PLANAR_EXTENT",
            vec![string(""), real(text.width), real(text.height)],
        );
        // AP202 text placement is the baseline origin, not the feature's box
        // anchor. Paired SFC/P21 corpus and the native common library use these
        // offsets for lower/middle/upper anchors, in the unscaled local frame.
        let offset = [0.5, -0.5, -1.0][((text.base_point - 1) / 3) as usize] * text.height;
        let angle = text.angle_deg.to_radians();
        let baseline = Point2 {
            x: text.anchor.x - angle.sin() * offset,
            y: text.anchor.y + angle.cos() * offset,
        };
        let axis = self.axis(&baseline, text.angle_deg);
        let horizontal = ["left", "centre", "right"][((text.base_point - 1) % 3) as usize];
        let vertical = ["baseline", "middleline", "topline"][((text.base_point - 1) / 3) as usize];
        let literal = self.add(
            "TEXT_LITERAL_WITH_EXTENT",
            vec![
                string(format!("$$SXF_{vertical} {horizontal}")),
                string(&text.text),
                reference(axis),
                string(format!("baseline {horizontal}")),
                enumeration(if text.direction == 1 { "RIGHT" } else { "DOWN" }),
                reference(font),
                reference(extent),
            ],
        );
        let appearance = self.add("TEXT_STYLE_FOR_DEFINED_FONT", vec![reference(color)]);
        let style = self.complex(vec![
            record("TEXT_STYLE", vec![string(""), reference(appearance)]),
            record(
                "TEXT_STYLE_WITH_BOX_CHARACTERISTICS",
                vec![Value::List(vec![
                    measure("BOX_HEIGHT", text.height),
                    measure("BOX_WIDTH", text.width),
                    measure("BOX_SLANT_ANGLE", text.slant_deg.to_radians()),
                    measure("BOX_ROTATE_ANGLE", 0.0),
                ])],
            ),
            record(
                "TEXT_STYLE_WITH_SPACING",
                vec![measure("LENGTH_MEASURE", text.spacing)],
            ),
        ]);
        let assignment = self.add("PRESENTATION_STYLE_ASSIGNMENT", vec![references(&[style])]);
        Ok(self.occurrence(
            "ANNOTATION_TEXT_OCCURRENCE",
            "",
            literal,
            assignment,
            text.style.layer_code.unwrap_or(0),
        ))
    }
}

fn lookup(table: &BTreeMap<i64, i64>, code: i64, name: &str) -> Result<i64, WriteError> {
    table
        .get(&code)
        .copied()
        .ok_or_else(|| error(format!("P21 {name} code {code} has no definition")))
}

/// Generate a new AP202 graph; source SXF IDs/codes resolve content but are not
/// reused as output graph IDs. No dimension/group flattening or rasterization.
pub fn serialize_p21(output: &ParseOutput) -> Result<Vec<u8>, WriteError> {
    serialize_p21_with_dependencies(output, false, None)
}

pub(crate) fn serialize_p21_with_dependencies(
    output: &ParseOutput,
    allow_external_references: bool,
    file_name: Option<&str>,
) -> Result<Vec<u8>, WriteError> {
    serialize_sfc(
        output,
        SfcWriteOptions {
            allow_external_references,
            ..SfcWriteOptions::default()
        },
    )?;
    let document = &output.document;
    let model = document
        .sfc_model
        .as_ref()
        .ok_or_else(|| error("P21 generation needs a resolved SFC model"))?;
    if !allow_external_references && model.attribute_attachments.iter().any(|a| matches!(&a.mechanism, SfcAttributeMechanism::SingleAttribute { attribute_name: Some(name), .. } if matches!(name.as_str(), "画像" | "ファイル名"))) {
        return Err(error("P21 has external dependencies; use save_p21_bundle or save_p2z"));
    }
    for feature in &document.typed_features {
        if let TypedFeature::DrawingAttribute(title) = &feature.feature {
            let year = title.drawing_year;
            let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
            let days = match title.drawing_month {
                2 => {
                    if leap {
                        29
                    } else {
                        28
                    }
                }
                4 | 6 | 9 | 11 => 30,
                _ => 31,
            };
            if year <= 0 || title.drawing_day > days {
                return Err(error(
                    "P21 drawing title requires a valid Gregorian calendar date",
                ));
            }
        }
        if !matches!(
            feature.feature,
            TypedFeature::DrawingSheet(_)
                | TypedFeature::Layer(_)
                | TypedFeature::PreDefinedFont(_)
                | TypedFeature::UserDefinedFont(_)
                | TypedFeature::PreDefinedColour(_)
                | TypedFeature::UserDefinedColour(_)
                | TypedFeature::Width(_)
                | TypedFeature::TextFont(_)
                | TypedFeature::Line(_)
                | TypedFeature::Circle(_)
                | TypedFeature::Arc(_)
                | TypedFeature::Polyline(_)
                | TypedFeature::Ellipse(_)
                | TypedFeature::EllipseArc(_)
                | TypedFeature::Text(_)
                | TypedFeature::LinearDim(_)
                | TypedFeature::CurveDim(_)
                | TypedFeature::AngularDim(_)
                | TypedFeature::RadiusDim(_)
                | TypedFeature::DiameterDim(_)
                | TypedFeature::Label(_)
                | TypedFeature::Balloon(_)
                | TypedFeature::CompositeCurve(_)
                | TypedFeature::ExternallyDefinedHatch(_)
                | TypedFeature::FillAreaStyleColour(_)
                | TypedFeature::FillAreaStyleHatching(_)
                | TypedFeature::FillAreaStyleTiles(_)
                | TypedFeature::DrawingAttribute(_)
                | TypedFeature::ExternallyDefinedSymbol(_)
                | TypedFeature::SfigOrg(_)
                | TypedFeature::SfigLocate(_)
        ) {
            return Err(error(format!(
                "P21 output does not support #{} {}",
                feature.id, feature.keyword
            )));
        }
    }
    let mut graph = Graph::default();
    let angle_unit = graph.complex(vec![
        record("NAMED_UNIT", vec![Value::Omitted]),
        record("PLANE_ANGLE_UNIT", vec![]),
        record("SI_UNIT", vec![Value::Unset, enumeration("RADIAN")]),
    ]);
    graph.length_unit = graph.complex(vec![
        record("LENGTH_UNIT", vec![]),
        record("NAMED_UNIT", vec![Value::Omitted]),
        record("SI_UNIT", vec![enumeration("MILLI"), enumeration("METRE")]),
    ]);
    graph.context = graph.complex(vec![
        record("GEOMETRIC_REPRESENTATION_CONTEXT", vec![Value::Integer(2)]),
        record(
            "GLOBAL_UNIT_ASSIGNED_CONTEXT",
            vec![references(&[angle_unit, graph.length_unit])],
        ),
        record("REPRESENTATION_CONTEXT", vec![string("ID1"), string("2D")]),
    ]);
    let by_id: BTreeMap<_, _> = document
        .typed_features
        .iter()
        .map(|f| (f.id, &f.feature))
        .collect();
    for binding in &model.code_tables.colors {
        let id = match by_id[&binding.entity_id] {
            TypedFeature::PreDefinedColour(color) => {
                graph.predefined_color(&crate::features::normalized_predefined_name(&color.name))
            }
            TypedFeature::UserDefinedColour(color) => graph.add(
                "COLOUR_RGB",
                vec![
                    string(""),
                    real(color.red as f64 / 255.0),
                    real(color.green as f64 / 255.0),
                    real(color.blue as f64 / 255.0),
                ],
            ),
            _ => unreachable!(),
        };
        graph.colors.insert(binding.code, id);
    }
    for binding in &model.code_tables.line_types {
        let id = match by_id[&binding.entity_id] {
            TypedFeature::PreDefinedFont(font) => graph.add(
                "DRAUGHTING_PRE_DEFINED_CURVE_FONT",
                vec![string(crate::features::normalized_predefined_name(
                    &font.name,
                ))],
            ),
            TypedFeature::UserDefinedFont(font) => {
                let mut patterns = Vec::new();
                if font.pitch.len() % 2 != 0 {
                    return Err(error("P21 custom line type needs draw/gap pairs"));
                }
                for pair in font.pitch.as_chunks::<2>().0 {
                    patterns.push(graph.add(
                        "CURVE_STYLE_FONT_PATTERN",
                        vec![real(pair[0]), real(pair[1])],
                    ));
                }
                graph.add(
                    "CURVE_STYLE_FONT",
                    vec![
                        string(format!("$$SXF_{}", font.name)),
                        references(&patterns),
                    ],
                )
            }
            _ => unreachable!(),
        };
        graph.line_types.insert(binding.code, id);
    }
    for binding in &model.code_tables.line_widths {
        let TypedFeature::Width(width) = by_id[&binding.entity_id] else {
            unreachable!()
        };
        let id = graph.add(
            "LENGTH_MEASURE_WITH_UNIT",
            vec![
                measure("POSITIVE_LENGTH_MEASURE", width.width),
                reference(graph.length_unit),
            ],
        );
        graph.widths.insert(binding.code, id);
    }
    // Predefined SXF codes are valid even without an explicit SFC declaration.
    for (index, name) in [
        "black",
        "red",
        "green",
        "blue",
        "yellow",
        "magenta",
        "cyan",
        "white",
        "deeppink",
        "brown",
        "orange",
        "lightgreen",
        "lightblue",
        "lavender",
        "lightgray",
        "darkgray",
    ]
    .iter()
    .enumerate()
    {
        let code = index as i64 + 1;
        if !graph.colors.contains_key(&code) {
            let id = graph.predefined_color(name);
            graph.colors.insert(code, id);
        }
    }
    for (index, name) in [
        "continuous",
        "dashed",
        "dashed spaced",
        "long dashed dotted",
        "long dashed double-dotted",
        "long dashed triplicate-dotted",
        "dotted",
        "chain",
        "chain double dash",
        "dashed dotted",
        "double-dashed dotted",
        "dashed double-dotted",
        "double-dashed double-dotted",
        "dashed triplicate-dotted",
        "double-dashed triplicate-dotted",
    ]
    .iter()
    .enumerate()
    {
        let code = index as i64 + 1;
        if !graph.line_types.contains_key(&code) {
            let id = graph.add("DRAUGHTING_PRE_DEFINED_CURVE_FONT", vec![string(name)]);
            graph.line_types.insert(code, id);
        }
    }
    for (index, width) in [0.13, 0.18, 0.25, 0.35, 0.5, 0.7, 1.0, 1.4, 2.0]
        .iter()
        .enumerate()
    {
        let code = index as i64 + 1;
        if !graph.widths.contains_key(&code) {
            let id = graph.add(
                "LENGTH_MEASURE_WITH_UNIT",
                vec![
                    measure("POSITIVE_LENGTH_MEASURE", *width),
                    reference(graph.length_unit),
                ],
            );
            graph.widths.insert(code, id);
        }
    }
    let source = graph.add(
        "EXTERNAL_SOURCE",
        vec![Value::Typed {
            keyword: "IDENTIFIER".into(),
            parameters: vec![string("SCADEC")],
        }],
    );
    graph.source = source;
    for binding in &model.code_tables.text_fonts {
        let TypedFeature::TextFont(font) = by_id[&binding.entity_id] else {
            unreachable!()
        };
        let id = graph.add(
            "EXTERNALLY_DEFINED_TEXT_FONT",
            vec![
                Value::Typed {
                    keyword: "IDENTIFIER".into(),
                    parameters: vec![string(&font.name)],
                },
                reference(source),
            ],
        );
        graph.fonts.insert(binding.code, id);
    }
    let definitions: BTreeMap<_, _> = model
        .sfig_definitions
        .iter()
        .map(|d| (d.entity_id, d.clone()))
        .chain(model.attribute_attachments.iter().map(|a| {
            (
                a.definition_id,
                SfcSfigDefinition {
                    entity_id: a.definition_id,
                    name: a.name.clone(),
                    kind_flag: a.kind_flag,
                    component_ids: a.component_ids.clone(),
                },
            )
        }))
        .collect();
    let targets: BTreeMap<_, _> = model
        .sfig_references
        .iter()
        .map(|r| (r.placement_id, r.definition_id))
        .chain(
            model
                .attribute_attachments
                .iter()
                .flat_map(|a| a.placement_ids.iter().map(move |id| (*id, a.definition_id))),
        )
        .collect();
    for feature in &document.typed_features {
        let source_id = feature.id;
        let item = match &feature.feature {
            feature @ (TypedFeature::Line(_)
            | TypedFeature::Circle(_)
            | TypedFeature::Arc(_)
            | TypedFeature::Polyline(_)
            | TypedFeature::Ellipse(_)
            | TypedFeature::EllipseArc(_)) => {
                let style = feature.style().unwrap();
                let assignment = graph.curve_style(style)?;
                let geometry = graph.curve(feature)?;
                graph.curves.insert(source_id, geometry);
                Some(graph.occurrence(
                    "ANNOTATION_CURVE_OCCURRENCE",
                    "",
                    geometry,
                    assignment,
                    style.layer_code.unwrap_or(0),
                ))
            }
            TypedFeature::Text(text) => Some(graph.text(text)?),
            feature @ (TypedFeature::LinearDim(_)
            | TypedFeature::CurveDim(_)
            | TypedFeature::AngularDim(_)
            | TypedFeature::RadiusDim(_)
            | TypedFeature::DiameterDim(_)
            | TypedFeature::Label(_)
            | TypedFeature::Balloon(_)) => Some(graph.dimension(feature)?),
            TypedFeature::CompositeCurve(curve) => Some(graph.composite(feature.id, curve, model)?),
            fill @ (TypedFeature::ExternallyDefinedHatch(_)
            | TypedFeature::FillAreaStyleColour(_)
            | TypedFeature::FillAreaStyleHatching(_)
            | TypedFeature::FillAreaStyleTiles(_)) => Some(graph.fill(fill, model, &by_id)?),
            TypedFeature::ExternallyDefinedSymbol(symbol) => Some(graph.external_symbol(symbol)?),
            TypedFeature::SfigOrg(_) => {
                let definition = &definitions[&feature.id];
                let mut components = definition
                    .component_ids
                    .iter()
                    .map(|id| lookup(&graph.items, *id, "component"))
                    .collect::<Result<Vec<_>, _>>()?;
                let axis = graph.axis(&Point2 { x: 0.0, y: 0.0 }, 0.0);
                components.push(axis);
                let prefix = match definition.kind_flag {
                    1 => "FM",
                    2 => "FG",
                    3 => "G",
                    4 => "P",
                    _ => unreachable!(),
                };
                let name = format!("$$SXF_{prefix}_{}", definition.name);
                // The original feature name already passed the SFC byte limit.
                // AP202 adds an identifier prefix outside that feature name.
                let representation = graph.add(
                    "DRAUGHTING_SUBFIGURE_REPRESENTATION",
                    vec![
                        string(name),
                        references(&components),
                        reference(graph.context),
                    ],
                );
                let map = graph.add(
                    "SYMBOL_REPRESENTATION_MAP",
                    vec![reference(axis), reference(representation)],
                );
                graph.maps.insert(feature.id, map);
                None
            }
            TypedFeature::SfigLocate(placement) => {
                let definition_id = targets[&feature.id];
                let map = lookup(&graph.maps, definition_id, "subfigure")?;
                let definition = &definitions[&definition_id];
                // Geodetic partial drawings exchange X and Y local axes (Feature
                // Specification §2-3); rejected until the reader shares that rule.
                if definition.kind_flag == 2 {
                    return Err(error(format!(
                        "P21 geodetic partial drawing #{} is not supported",
                        definition_id
                    )));
                }
                let axis = graph.axis(&placement.position, placement.angle_deg);
                let target = graph.add(
                    "SYMBOL_TARGET",
                    vec![
                        string(""),
                        reference(axis),
                        real(placement.ratio_x),
                        real(placement.ratio_y),
                    ],
                );
                let mapped = graph.complex(vec![
                    record("ANNOTATION_SYMBOL", vec![]),
                    record("GEOMETRIC_REPRESENTATION_ITEM", vec![]),
                    record("MAPPED_ITEM", vec![reference(map), reference(target)]),
                    record("REPRESENTATION_ITEM", vec![string("")]),
                ]);
                let assignment = graph.add(
                    "PRESENTATION_STYLE_ASSIGNMENT",
                    vec![Value::List(vec![Value::Typed {
                        keyword: "NULL_STYLE".into(),
                        parameters: vec![enumeration("NULL")],
                    }])],
                );
                let prefix = match definition.kind_flag {
                    1 => "FM",
                    3 => "G",
                    4 => "P",
                    _ => unreachable!(),
                };
                Some(graph.occurrence(
                    "ANNOTATION_SUBFIGURE_OCCURRENCE",
                    &format!("$$SXF_{prefix}_{}", definition.name),
                    mapped,
                    assignment,
                    placement.style.layer_code.unwrap_or(0),
                ))
            }
            _ => None,
        };
        if let Some(item) = item {
            graph.items.insert(feature.id, item);
        }
    }
    let sheet_model = model.sheet.as_ref().ok_or_else(|| error("Missing sheet"))?;
    let TypedFeature::DrawingSheet(sheet) = by_id[&sheet_model.entity_id] else {
        unreachable!()
    };
    let title = document
        .typed_features
        .iter()
        .find_map(|f| match &f.feature {
            TypedFeature::DrawingAttribute(a) => Some(a),
            _ => None,
        });
    let revision = graph.drawing_title(title, &sheet.name);
    let axis = graph.axis(&Point2 { x: 0.0, y: 0.0 }, 0.0);
    let (width, height) = crate::authoring::sheet_dimensions(
        sheet.sheet_type,
        sheet.orientation,
        if sheet.sheet_type == 9 {
            Some(sheet.free_x_mm as f64)
        } else {
            None
        },
        if sheet.sheet_type == 9 {
            Some(sheet.free_y_mm as f64)
        } else {
            None
        },
    )?;
    let box_id = graph.add(
        "PLANAR_BOX",
        vec![
            string(""),
            real(width as f64),
            real(height as f64),
            reference(axis),
        ],
    );
    let mut sheet_items = sheet_model
        .component_ids
        .iter()
        .map(|id| lookup(&graph.items, *id, "sheet item"))
        .collect::<Result<Vec<_>, _>>()?;
    sheet_items.push(box_id);
    let paper_name = if sheet.sheet_type == 9 {
        "FREE".into()
    } else {
        format!(
            "A{}_{}",
            sheet.sheet_type,
            if sheet.orientation == 0 {
                "vertical"
            } else {
                "horizontal"
            }
        )
    };
    let sheet_id = graph.add(
        "DRAWING_SHEET_REVISION",
        vec![
            string(paper_name),
            references(&sheet_items),
            reference(graph.context),
            string("01"),
        ],
    );
    graph.add(
        "DRAWING_SHEET_REVISION_USAGE",
        vec![reference(sheet_id), reference(revision), string("01")],
    );
    graph.add(
        "PRESENTATION_SIZE",
        vec![reference(sheet_id), reference(box_id)],
    );
    for binding in &model.code_tables.layers {
        let TypedFeature::Layer(layer) = by_id[&binding.entity_id] else {
            unreachable!()
        };
        if let Some(items) = graph.layer_items.get(&binding.code).cloned() {
            let assignment = graph.add(
                "PRESENTATION_LAYER_ASSIGNMENT",
                vec![string(&layer.name), string(""), references(&items)],
            );
            graph.add(
                "PRESENTATION_LAYER_USAGE",
                vec![reference(assignment), reference(sheet_id)],
            );
            if layer.visibility_flag == 0 {
                graph.add("INVISIBILITY", vec![references(&[assignment])]);
            }
        }
    }
    if let Some(items) = graph.layer_items.get(&0).cloned() {
        let assignment = graph.add(
            "PRESENTATION_LAYER_ASSIGNMENT",
            vec![
                string("$$SXF_dummy_layer_for_subfigure"),
                string(""),
                references(&items),
            ],
        );
        graph.add(
            "PRESENTATION_LAYER_USAGE",
            vec![reference(assignment), reference(sheet_id)],
        );
    }
    let mut header = document.header.clone();
    for record in &mut header.entities {
        if record.keyword.eq_ignore_ascii_case("FILE_DESCRIPTION") {
            record.parameters = vec![
                Value::List(vec![string("SCADEC level2 AP202_mode")]),
                string("1"),
            ];
        }
        if record.keyword.eq_ignore_ascii_case("FILE_NAME") {
            let filename = record.parameters[0]
                .as_string()
                .ok_or_else(|| error("Invalid FILE_NAME"))?;
            let filename = file_name.map(str::to_owned).unwrap_or_else(|| {
                std::path::Path::new(&filename)
                    .with_extension("p21")
                    .to_string_lossy()
                    .into_owned()
            });
            record.parameters[0] = string(filename);
        }
    }
    encode_graph(header, graph.entities)
}

fn step_string(value: &str) -> Result<String, WriteError> {
    if value.chars().any(char::is_control) {
        return Err(error("P21 strings must not contain control characters"));
    }
    if value.is_ascii() && !value.contains(['\\', '\'']) {
        return Ok(value.into());
    }
    let mut result = String::from("\\X2\\");
    for unit in value.encode_utf16() {
        write!(result, "{unit:04X}").unwrap();
    }
    result.push_str("\\X0\\");
    Ok(result)
}

fn prepare(value: &mut Value) -> Result<(), WriteError> {
    match value {
        Value::String(text) => *text = step_string(text)?,
        Value::List(values)
        | Value::Typed {
            parameters: values, ..
        } => {
            for value in values {
                prepare(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn encode_graph(
    mut header: HeaderSection,
    mut entities: Vec<EntityInstance>,
) -> Result<Vec<u8>, WriteError> {
    let mut text = String::from("ISO-10303-21;\r\nHEADER;\r\n");
    let emit_record = |text: &mut String, record: &mut Record| -> Result<(), WriteError> {
        for value in &mut record.parameters {
            prepare(value)?;
        }
        text.push_str(&record.keyword);
        text.push('(');
        // Strings already have standard STEP escapes; emit_values doubles '\\',
        // so restore their single spelling after emitting the whole record.
        let mut parameters = String::new();
        crate::writer::emit_values(&mut parameters, &record.parameters, false, false, 0)?;
        text.push_str(&parameters.replace("\\\\", "\\"));
        text.push(')');
        Ok(())
    };
    for record in &mut header.entities {
        emit_record(&mut text, record)?;
        text.push_str(";\r\n");
    }
    text.push_str("ENDSEC;\r\nDATA;\r\n");
    for entity in &mut entities {
        write!(text, "#{}=", entity.id).unwrap();
        match &mut entity.body {
            EntityBody::Simple(record) => emit_record(&mut text, record)?,
            EntityBody::Complex(records) => {
                text.push('(');
                for record in records {
                    emit_record(&mut text, record)?;
                }
                text.push(')');
            }
        }
        text.push_str(";\r\n");
    }
    text.push_str("ENDSEC;\r\nEND-ISO-10303-21;\r\n");
    let parsed = crate::parser::parse_from_bytes(FileFormat::P21, text.as_bytes(), true)
        .map_err(|e| error(format!("P21 output validation failed: {e}")))?;
    if !parsed.warnings.is_empty()
        || parsed.document.entities != entities
        || parsed.document.header != header
    {
        return Err(error(
            "P21 output changed entity/header values or has warnings",
        ));
    }
    Ok(text.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_sheet_is_a_real_ap202_graph() {
        let doc = crate::SfcDocument::new("drawing.sfc", "図面", 297, 210, "2026-10-07").unwrap();
        let bytes = serialize_p21(doc.snapshot()).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("AP202_mode"));
        assert!(text.contains("DRAWING_SHEET_REVISION("));
        assert!(text.contains("PLANAR_BOX("));
        assert!(!text.contains("/*SXF"));
    }
    #[test]
    fn unsupported_features_fail_explicitly() {
        let mut doc =
            crate::SfcDocument::new("drawing.sfc", "drawing", 297, 210, "2026-10-07").unwrap();
        let id = doc.add_feature("spline", &BTreeMap::new()).unwrap();
        let message = serialize_p21(doc.snapshot()).unwrap_err().to_string();
        assert!(message.contains(&format!("#{id} spline_feature")));
    }
    #[test]
    fn attributes_are_resolved_as_groups_without_sfig_references() {
        let mut doc =
            crate::SfcDocument::new("drawing.sfc", "drawing", 297, 210, "2026-10-07").unwrap();
        let id = doc
            .add_element(
                "text_string_feature",
                vec![
                    Value::Integer(1),
                    Value::Integer(1),
                    Value::Integer(1),
                    string("title"),
                    real(10.0),
                    real(20.0),
                    real(3.0),
                    real(15.0),
                    real(0.0),
                    real(0.0),
                    real(0.0),
                    Value::Integer(1),
                    Value::Integer(1),
                ],
            )
            .unwrap();
        doc.set_text_attribute(id, "title", Some("STR"), None)
            .unwrap();
        assert!(doc
            .snapshot()
            .document
            .sfc_model
            .as_ref()
            .unwrap()
            .sfig_references
            .is_empty());
        let bytes = serialize_p21(doc.snapshot()).unwrap();
        assert!(String::from_utf8(bytes)
            .unwrap()
            .contains("$$SXF_G_$$ATRS$$"));
    }
    #[test]
    fn dimensions_keep_callouts_and_omission_flags() {
        let mut doc =
            crate::SfcDocument::new("drawing.sfc", "drawing", 297, 210, "2026-10-07").unwrap();
        doc.add_feature(
            "linear_dimension",
            &BTreeMap::from([
                ("end_x".into(), real(20.0)),
                ("text_present".into(), Value::Integer(0)),
            ]),
        )
        .unwrap();
        let bytes = serialize_p21(doc.snapshot()).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("LINEAR_DIMENSION()"));
        assert!(text.contains("DIMENSION_CURVE()"));
        assert!(!text.contains("PROJECTION_CURVE()"));
        assert!(!text.contains("DIMENSION_CURVE_TERMINATOR("));
        assert!(!text.contains("TEXT_LITERAL_WITH_EXTENT("));
    }
}
