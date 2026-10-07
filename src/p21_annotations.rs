//! AP202 subset §§3-2-1, 22–29: semantic callouts, terminators and titles.
use super::*;

impl Graph {
    fn add_record(&mut self, id: i64, value: Record) {
        let EntityBody::Complex(records) = &mut self.entities[(id - 1) as usize].body else {
            unreachable!()
        };
        records.push(value);
        records.sort_by(|a, b| a.keyword.cmp(&b.keyword));
    }
    fn set_name(&mut self, id: i64, name: &str) {
        let EntityBody::Complex(records) = &mut self.entities[(id - 1) as usize].body else {
            unreachable!()
        };
        records
            .iter_mut()
            .find(|r| r.keyword == "REPRESENTATION_ITEM")
            .unwrap()
            .parameters = vec![string(name)];
    }
    fn annotated_curve(
        &mut self,
        feature: &TypedFeature,
        style: &CommonStyle,
        role: &str,
        name: &str,
    ) -> Result<i64, WriteError> {
        let geometry = self.curve(feature)?;
        let assignment = self.curve_style(style)?;
        let id = self.occurrence(
            "ANNOTATION_CURVE_OCCURRENCE",
            name,
            geometry,
            assignment,
            style.layer_code.unwrap_or(0),
        );
        self.add_record(id, record(role, vec![]));
        Ok(id)
    }
    fn annotation_text(
        &mut self,
        value: &FeatureText,
        style: &CommonStyle,
    ) -> Result<Option<i64>, WriteError> {
        if value.present_flag == 0 {
            return Ok(None);
        }
        let mut style = style.clone();
        style.font_code = Some(value.font_code);
        let id = self.text(&TextFeature {
            style,
            text: value.text.clone(),
            anchor: value.anchor.clone(),
            height: value.height,
            width: value.width,
            spacing: value.spacing,
            angle_deg: value.angle_deg,
            slant_deg: value.slant_deg,
            base_point: value.base_point,
            direction: value.direction,
        })?;
        self.set_name(id, "dimension value");
        Ok(Some(id))
    }
    fn symbol_style(&mut self, style: &CommonStyle) -> Result<i64, WriteError> {
        let color = lookup(
            &self.colors,
            style.color_code.filter(|c| *c != 0).unwrap_or(1),
            "colour",
        )?;
        let color = self.add("SYMBOL_COLOUR", vec![reference(color)]);
        let style = self.add("SYMBOL_STYLE", vec![string(""), reference(color)]);
        Ok(self.add("PRESENTATION_STYLE_ASSIGNMENT", vec![references(&[style])]))
    }
    pub(super) fn symbol(
        &mut self,
        definition: i64,
        position: &Point2,
        angle: f64,
        scales: (f64, f64),
        style: &CommonStyle,
    ) -> Result<i64, WriteError> {
        let axis = self.axis(position, angle);
        let target = self.add(
            "SYMBOL_TARGET",
            vec![string(""), reference(axis), real(scales.0), real(scales.1)],
        );
        let symbol = self.add(
            "DEFINED_SYMBOL",
            vec![string(""), reference(definition), reference(target)],
        );
        let style_id = self.symbol_style(style)?;
        Ok(self.occurrence(
            "ANNOTATION_SYMBOL_OCCURRENCE",
            "",
            symbol,
            style_id,
            style.layer_code.unwrap_or(0),
        ))
    }
    pub(super) fn external_symbol(
        &mut self,
        symbol: &ExternallyDefinedSymbolFeature,
    ) -> Result<i64, WriteError> {
        let definition = self.add(
            "EXTERNALLY_DEFINED_SYMBOL",
            vec![
                Value::Typed {
                    keyword: "IDENTIFIER".into(),
                    parameters: vec![string(&symbol.name)],
                },
                reference(self.source),
            ],
        );
        let id = self.symbol(
            definition,
            &symbol.position,
            symbol.rotation_angle_deg,
            (symbol.scale, symbol.scale),
            &symbol.style,
        )?;
        if symbol.color_flag == 0 {
            let assignment = self.add(
                "PRESENTATION_STYLE_ASSIGNMENT",
                vec![Value::List(vec![Value::Typed {
                    keyword: "NULL_STYLE".into(),
                    parameters: vec![enumeration("NULL")],
                }])],
            );
            let EntityBody::Complex(records) = &mut self.entities[(id - 1) as usize].body else {
                unreachable!()
            };
            records
                .iter_mut()
                .find(|r| r.keyword == "STYLED_ITEM")
                .unwrap()
                .parameters[0] = references(&[assignment]);
        }
        Ok(id)
    }
    fn arrow(
        &mut self,
        arrow: &DimensionArrow,
        vector: (f64, f64),
        curve: i64,
        style: &CommonStyle,
        index: usize,
        leader: bool,
    ) -> Result<Option<i64>, WriteError> {
        if arrow.code == 0 || arrow.direction_flag == 0 {
            return Ok(None);
        }
        if vector.0.hypot(vector.1) == 0.0 {
            return Err(error("P21 terminator direction is undefined"));
        }
        let names = [
            "blanked arrow",
            "blanked box",
            "blanked dot",
            "dimension origin",
            "filled box",
            "filled arrow",
            "filled dot",
            "integral symbol",
            "open arrow",
            "slash",
            "unfilled arrow",
        ];
        let definition = self.add(
            "PRE_DEFINED_TERMINATOR_SYMBOL",
            vec![string(names[(arrow.code - 1) as usize])],
        );
        // Symbol local +X points towards the tip. For inside arrows the first
        // terminator points opposite the directed dimension/leader curve.
        let flip = if arrow.direction_flag == 1 || leader {
            -1.0
        } else {
            1.0
        };
        let angle = (vector.1 * flip).atan2(vector.0 * flip).to_degrees();
        let id = self.symbol(
            definition,
            &arrow.position,
            angle,
            (arrow.scale, arrow.scale),
            style,
        )?;
        self.add_record(id, record("TERMINATOR_SYMBOL", vec![reference(curve)]));
        if leader {
            self.add_record(id, record("LEADER_TERMINATOR", vec![]));
        } else {
            self.set_name(id, &format!("$$SXF_arw_{index}"));
            self.add_record(
                id,
                record(
                    "DIMENSION_CURVE_TERMINATOR",
                    vec![enumeration(if index == 1 { "ORIGIN" } else { "TARGET" })],
                ),
            );
        }
        Ok(Some(id))
    }
    fn callout(&mut self, kind: &str, contents: &[i64], leader: bool, layer: i64) -> i64 {
        let mut records = vec![
            record("DRAUGHTING_CALLOUT", vec![references(contents)]),
            record("DRAUGHTING_ELEMENTS", vec![]),
            record("GEOMETRIC_REPRESENTATION_ITEM", vec![]),
            record("REPRESENTATION_ITEM", vec![string("")]),
        ];
        records.push(record(kind, vec![]));
        if !leader {
            records.push(record("DIMENSION_CURVE_DIRECTED_CALLOUT", vec![]));
        }
        let id = self.complex(records);
        self.layer_items.entry(layer).or_default().push(id);
        id
    }
    pub(super) fn dimension(&mut self, feature: &TypedFeature) -> Result<i64, WriteError> {
        let style = feature.style().unwrap();
        let line = |start: &Point2, end: &Point2| {
            TypedFeature::Line(LineFeature {
                style: style.clone(),
                start: start.clone(),
                end: end.clone(),
            })
        };
        let vector = |a: &Point2, b: &Point2| (b.x - a.x, b.y - a.y);
        let (kind, geometry, extensions, arrows, text, leader, balloon) = match feature {
            TypedFeature::LinearDim(v) => (
                "LINEAR_DIMENSION",
                line(&v.start, &v.end),
                vec![&v.extension_line1, &v.extension_line2],
                vec![
                    (&v.arrow1, vector(&v.start, &v.end)),
                    (&v.arrow2, vector(&v.end, &v.start)),
                ],
                &v.text,
                false,
                None,
            ),
            TypedFeature::RadiusDim(v) => (
                "RADIUS_DIMENSION",
                line(&v.start, &v.end),
                vec![],
                vec![(&v.arrow, vector(&v.end, &v.start))],
                &v.text,
                false,
                None,
            ),
            TypedFeature::DiameterDim(v) => (
                "DIAMETER_DIMENSION",
                line(&v.start, &v.end),
                vec![],
                vec![
                    (&v.arrow1, vector(&v.start, &v.end)),
                    (&v.arrow2, vector(&v.end, &v.start)),
                ],
                &v.text,
                false,
                None,
            ),
            TypedFeature::CurveDim(v) => (
                "CURVE_DIMENSION",
                TypedFeature::Arc(ArcFeature {
                    style: style.clone(),
                    center: v.center.clone(),
                    radius: v.radius,
                    start_angle_deg: v.start_angle_deg,
                    end_angle_deg: v.end_angle_deg,
                    direction_flag: 0,
                }),
                vec![&v.extension_line1, &v.extension_line2],
                vec![
                    (
                        &v.arrow1,
                        (
                            v.center.y - v.arrow1.position.y,
                            v.arrow1.position.x - v.center.x,
                        ),
                    ),
                    (
                        &v.arrow2,
                        (
                            v.arrow2.position.y - v.center.y,
                            v.center.x - v.arrow2.position.x,
                        ),
                    ),
                ],
                &v.text,
                false,
                None,
            ),
            TypedFeature::AngularDim(v) => (
                "ANGULAR_DIMENSION",
                TypedFeature::Arc(ArcFeature {
                    style: style.clone(),
                    center: v.center.clone(),
                    radius: v.radius,
                    start_angle_deg: v.start_angle_deg,
                    end_angle_deg: v.end_angle_deg,
                    direction_flag: 0,
                }),
                vec![&v.extension_line1, &v.extension_line2],
                vec![
                    (
                        &v.arrow1,
                        (
                            v.center.y - v.arrow1.position.y,
                            v.arrow1.position.x - v.center.x,
                        ),
                    ),
                    (
                        &v.arrow2,
                        (
                            v.arrow2.position.y - v.center.y,
                            v.center.x - v.arrow2.position.x,
                        ),
                    ),
                ],
                &v.text,
                false,
                None,
            ),
            TypedFeature::Label(v) => (
                "LEADER_DIRECTED_CALLOUT",
                TypedFeature::Polyline(PolylineFeature {
                    style: style.clone(),
                    declared_point_count: Some(v.vertices.len()),
                    points: v.vertices.clone(),
                }),
                vec![],
                vec![],
                &v.text,
                true,
                None,
            ),
            TypedFeature::Balloon(v) => (
                "LEADER_DIRECTED_CALLOUT",
                TypedFeature::Polyline(PolylineFeature {
                    style: style.clone(),
                    declared_point_count: Some(v.vertices.len()),
                    points: v.vertices.clone(),
                }),
                vec![],
                vec![],
                &v.text,
                true,
                Some((&v.center, v.radius)),
            ),
            _ => unreachable!(),
        };
        let curve = self.annotated_curve(
            &geometry,
            style,
            if leader {
                "LEADER_CURVE"
            } else {
                "DIMENSION_CURVE"
            },
            "",
        )?;
        let mut contents = vec![curve];
        for (i, extension) in extensions.iter().enumerate() {
            if extension.present_flag == 1 {
                let projection = self.annotated_curve(
                    &line(&extension.start, &extension.end),
                    style,
                    "PROJECTION_CURVE",
                    &format!("$$SXF_prj_{}", i + 1),
                )?;
                // SXF stores the extension's measurement base in LINE.pnt,
                // independently of the visible Cartesian trim endpoints.
                // Using (0,0) here collapses native linear-dimension bases.
                let EntityBody::Complex(occurrence) =
                    &self.entities[(projection - 1) as usize].body
                else {
                    unreachable!()
                };
                let Value::Reference(trimmed) = occurrence
                    .iter()
                    .find(|r| r.keyword == "STYLED_ITEM")
                    .unwrap()
                    .parameters[1]
                else {
                    unreachable!()
                };
                let EntityBody::Simple(trimmed) = &self.entities[(trimmed - 1) as usize].body
                else {
                    unreachable!()
                };
                let Value::Reference(basis) = trimmed.parameters[1] else {
                    unreachable!()
                };
                let EntityBody::Simple(basis) = &self.entities[(basis - 1) as usize].body else {
                    unreachable!()
                };
                let Value::Reference(base) = basis.parameters[1] else {
                    unreachable!()
                };
                let EntityBody::Simple(base) = &mut self.entities[(base - 1) as usize].body else {
                    unreachable!()
                };
                base.parameters[1] =
                    Value::List(vec![real(extension.base.x), real(extension.base.y)]);
                contents.push(projection);
            }
        }
        if let Some((center, radius)) = balloon {
            let circle = self.curve(&TypedFeature::Circle(CircleFeature {
                style: style.clone(),
                center: center.clone(),
                radius,
            }))?;
            let assignment = self.curve_style(style)?;
            contents.push(self.occurrence(
                "ANNOTATION_CURVE_OCCURRENCE",
                "",
                circle,
                assignment,
                style.layer_code.unwrap_or(0),
            ));
        }
        let text_id = self.annotation_text(text, style)?;
        contents.extend(text_id);
        for (i, (arrow, vector)) in arrows.iter().enumerate() {
            contents.extend(self.arrow(arrow, *vector, curve, style, i + 1, false)?);
        }
        let leader_data = match feature {
            TypedFeature::Label(v) => Some((&v.vertices, &v.arrow)),
            TypedFeature::Balloon(v) => Some((&v.vertices, &v.arrow)),
            _ => None,
        };
        if let Some((vertices, arrow)) = leader_data {
            let arrow = DimensionArrow {
                code: arrow.code,
                direction_flag: 2,
                position: vertices[0].clone(),
                scale: arrow.scale,
            };
            contents.extend(self.arrow(
                &arrow,
                vector(&vertices[0], &vertices[1]),
                curve,
                style,
                1,
                true,
            )?);
        }
        let callout = self.callout(kind, &contents, leader, style.layer_code.unwrap_or(0));
        if !leader {
            if let Some(text) = text_id {
                let structured = self.complex(vec![
                    record("DRAUGHTING_CALLOUT", vec![references(&[text])]),
                    record("DRAUGHTING_ELEMENTS", vec![]),
                    record("GEOMETRIC_REPRESENTATION_ITEM", vec![]),
                    record("REPRESENTATION_ITEM", vec![string("")]),
                    record("STRUCTURED_DIMENSION_CALLOUT", vec![]),
                ]);
                self.add(
                    "DIMENSION_CALLOUT_RELATIONSHIP",
                    vec![
                        string("primary"),
                        string(""),
                        reference(callout),
                        reference(structured),
                    ],
                );
            }
        }
        Ok(callout)
    }
    pub(super) fn drawing_title(
        &mut self,
        value: Option<&DrawingAttributeFeature>,
        fallback: &str,
    ) -> i64 {
        let definition = self.add(
            "DRAWING_DEFINITION",
            vec![
                string(value.map_or("01", |v| &v.drawing_number)),
                value.map_or(Value::Unset, |v| string(&v.drawing_type)),
            ],
        );
        let revision = self.add(
            "DRAUGHTING_DRAWING_REVISION",
            vec![
                string("01"),
                reference(definition),
                value.map_or(Value::Unset, |v| string(&v.drawing_scale)),
            ],
        );
        self.add(
            "DRAUGHTING_TITLE",
            vec![
                references(&[revision]),
                string("JAPANESE"),
                string(value.map_or(fallback, |v| &v.drawing_name)),
            ],
        );
        if let Some(v) = value {
            let contract_type = self.add("CONTRACT_TYPE", vec![string(&v.contract_type)]);
            let contract = self.add(
                "CONTRACT",
                vec![
                    string(&v.construction_name),
                    string(&v.project_name),
                    reference(contract_type),
                ],
            );
            self.add(
                "DRAUGHTING_CONTRACT_ASSIGNMENT",
                vec![reference(contract), references(&[revision])],
            );
            if v.drawing_year != 0 {
                let status = self.add("APPROVAL_STATUS", vec![string("")]);
                let approval = self.add("APPROVAL", vec![reference(status), string("")]);
                let date = self.add(
                    "CALENDAR_DATE",
                    vec![
                        Value::Integer(v.drawing_year),
                        Value::Integer(v.drawing_day),
                        Value::Integer(v.drawing_month),
                    ],
                );
                self.add(
                    "APPROVAL_DATE_TIME",
                    vec![reference(date), reference(approval)],
                );
                self.add(
                    "DRAUGHTING_APPROVAL_ASSIGNMENT",
                    vec![reference(approval), references(&[revision])],
                );
            }
            for (role, name) in [("creator", &v.contractor_name), ("owner", &v.owner_name)] {
                let role = self.add("ORGANIZATION_ROLE", vec![string(role)]);
                let organization =
                    self.add("ORGANIZATION", vec![Value::Unset, string(name), string("")]);
                self.add(
                    "DRAUGHTING_ORGANIZATION_ASSIGNMENT",
                    vec![
                        reference(organization),
                        reference(role),
                        references(&[revision]),
                    ],
                );
            }
        }
        revision
    }
}
