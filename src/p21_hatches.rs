//! AP202 subset §§3-2-30–34: composite boundaries and four fill styles.
use super::*;

impl Graph {
    pub(super) fn composite(
        &mut self,
        id: i64,
        curve: &CompositeCurveFeature,
        model: &SfcModel,
    ) -> Result<i64, WriteError> {
        let definition = model
            .composite_curve_definitions
            .iter()
            .find(|d| d.entity_id == id)
            .ok_or_else(|| error("Missing composite curve definition"))?;
        let mut segments = Vec::new();
        for child in &definition.component_ids {
            let mut geometry = lookup(&self.curves, *child, "boundary curve")?;
            if matches!(&self.entities[(geometry-1) as usize].body, EntityBody::Simple(r) if matches!(r.keyword.as_str(), "CIRCLE" | "ELLIPSE"))
            {
                geometry = self.add(
                    "TRIMMED_CURVE",
                    vec![
                        string(""),
                        reference(geometry),
                        Value::List(vec![measure("PARAMETER_VALUE", 0.0)]),
                        Value::List(vec![measure("PARAMETER_VALUE", std::f64::consts::TAU)]),
                        enumeration("T"),
                        enumeration("PARAMETER"),
                    ],
                );
            }
            segments.push(self.add(
                "COMPOSITE_CURVE_SEGMENT",
                vec![
                    enumeration("CONTINUOUS"),
                    enumeration("T"),
                    reference(geometry),
                ],
            ));
        }
        let geometry = self.add(
            "COMPOSITE_CURVE",
            vec![string(""), references(&segments), enumeration("F")],
        );
        self.curves.insert(id, geometry);
        let assignment = self.curve_style(&curve.style)?;
        let occurrence = self.occurrence(
            "ANNOTATION_CURVE_OCCURRENCE",
            "",
            geometry,
            assignment,
            curve.style.layer_code.unwrap_or(0),
        );
        if curve.visibility_flag == 0 {
            self.add("INVISIBILITY", vec![references(&[occurrence])]);
        }
        Ok(occurrence)
    }
    fn vector(&mut self, length: f64, angle: f64) -> i64 {
        let angle = angle.to_radians();
        let direction = self.add(
            "DIRECTION",
            vec![
                string(""),
                Value::List(vec![real(angle.cos()), real(angle.sin())]),
            ],
        );
        self.add(
            "VECTOR",
            vec![string(""), reference(direction), real(length)],
        )
    }
    pub(super) fn fill(
        &mut self,
        feature: &TypedFeature,
        model: &SfcModel,
        by_id: &BTreeMap<i64, &TypedFeature>,
    ) -> Result<i64, WriteError> {
        let (outer, holes) = match feature {
            TypedFeature::ExternallyDefinedHatch(v) => (v.out_id, &v.in_ids),
            TypedFeature::FillAreaStyleColour(v) => (v.out_id, &v.in_ids),
            TypedFeature::FillAreaStyleHatching(v) => (v.out_id, &v.in_ids),
            TypedFeature::FillAreaStyleTiles(v) => (v.out_id, &v.in_ids),
            _ => unreachable!(),
        };
        let definition = |code| {
            model
                .composite_curve_definitions
                .iter()
                .find(|d| d.code == code)
                .ok_or_else(|| error(format!("Missing boundary code {code}")))
        };
        let mut ids = vec![definition(outer)?.entity_id];
        for hole in holes {
            ids.push(definition(*hole)?.entity_id);
        }
        let boundaries = ids
            .iter()
            .map(|id| lookup(&self.curves, *id, "fill boundary"))
            .collect::<Result<Vec<_>, _>>()?;
        // Only the representative fill point is sampled; emitted boundary
        // geometry remains exact. Scanline selection avoids holes/concavities.
        let polygons = ids
            .iter()
            .map(|id| boundary_points(*id, model, by_id))
            .collect::<Result<Vec<_>, _>>()?;
        let target = self.point(&interior_point(&polygons)?);
        let area = self.add(
            "ANNOTATION_FILL_AREA",
            vec![string(""), references(&boundaries)],
        );
        let style = feature.style().unwrap();
        let styles = match feature {
            TypedFeature::FillAreaStyleColour(_) => vec![self.add(
                "FILL_AREA_STYLE_COLOUR",
                vec![
                    string(""),
                    reference(lookup(
                        &self.colors,
                        style.color_code.filter(|c| *c != 0).unwrap_or(1),
                        "colour",
                    )?),
                ],
            )],
            TypedFeature::ExternallyDefinedHatch(v) => vec![self.add(
                "EXTERNALLY_DEFINED_HATCH_STYLE",
                vec![
                    Value::Typed {
                        keyword: "IDENTIFIER".into(),
                        parameters: vec![string(&v.name)],
                    },
                    reference(self.source),
                    string(""),
                ],
            )],
            TypedFeature::FillAreaStyleHatching(v) => {
                let mut styles = Vec::new();
                for pattern in &v.patterns {
                    let assignment = self.curve_style(&CommonStyle {
                        color_code: Some(pattern.color_code),
                        line_type_code: Some(pattern.line_type_code),
                        line_width_code: Some(pattern.line_width_code),
                        ..CommonStyle::default()
                    })?;
                    let EntityBody::Simple(r) = &self.entities[(assignment - 1) as usize].body
                    else {
                        unreachable!()
                    };
                    let Value::List(values) = &r.parameters[0] else {
                        unreachable!()
                    };
                    let Value::Reference(curve_style) = values[0] else {
                        unreachable!()
                    };
                    let start = self.point(&pattern.start);
                    let vector = self.vector(pattern.spacing, pattern.angle_deg + 90.0);
                    let repeat = self.add(
                        "ONE_DIRECTION_REPEAT_FACTOR",
                        vec![string(""), reference(vector)],
                    );
                    styles.push(self.add(
                        "FILL_AREA_STYLE_HATCHING",
                        vec![
                            string(""),
                            reference(curve_style),
                            reference(repeat),
                            reference(start),
                            reference(start),
                            real(pattern.angle_deg.to_radians()),
                        ],
                    ));
                }
                styles
            }
            TypedFeature::FillAreaStyleTiles(v) => {
                let definition = self.add(
                    "EXTERNALLY_DEFINED_SYMBOL",
                    vec![
                        Value::Typed {
                            keyword: "IDENTIFIER".into(),
                            parameters: vec![string(&v.name)],
                        },
                        reference(self.source),
                    ],
                );
                let symbol = self.symbol(
                    definition,
                    &v.hatch_pattern_position,
                    v.hatch_pattern_angle_deg,
                    (v.hatch_pattern_scale_x, v.hatch_pattern_scale_y),
                    &CommonStyle {
                        layer_code: style.layer_code,
                        color_code: Some(v.hatch_color),
                        ..CommonStyle::default()
                    },
                )?;
                let tile = self.add(
                    "FILL_AREA_STYLE_TILE_SYMBOL_WITH_STYLE",
                    vec![string(""), reference(symbol)],
                );
                let first = self.vector(v.hatch_pattern_vector1, v.hatch_pattern_vector1_angle_deg);
                let second =
                    self.vector(v.hatch_pattern_vector2, v.hatch_pattern_vector2_angle_deg);
                let repeat = self.add(
                    "TWO_DIRECTION_REPEAT_FACTOR",
                    vec![string(""), reference(first), reference(second)],
                );
                vec![self.add(
                    "FILL_AREA_STYLE_TILES",
                    vec![
                        string(""),
                        reference(repeat),
                        references(&[tile]),
                        real(1.0),
                    ],
                )]
            }
            _ => unreachable!(),
        };
        let fill_style = self.add("FILL_AREA_STYLE", vec![string(""), references(&styles)]);
        let assignment = self.add(
            "PRESENTATION_STYLE_ASSIGNMENT",
            vec![references(&[fill_style])],
        );
        let id = self.complex(vec![
            record("ANNOTATION_FILL_AREA_OCCURRENCE", vec![reference(target)]),
            record("ANNOTATION_OCCURRENCE", vec![]),
            record("DRAUGHTING_ANNOTATION_OCCURRENCE", vec![]),
            record("GEOMETRIC_REPRESENTATION_ITEM", vec![]),
            record("REPRESENTATION_ITEM", vec![string("")]),
            record(
                "STYLED_ITEM",
                vec![references(&[assignment]), reference(area)],
            ),
        ]);
        self.layer_items
            .entry(style.layer_code.unwrap_or(0))
            .or_default()
            .push(id);
        Ok(id)
    }
}

fn boundary_points(
    id: i64,
    model: &SfcModel,
    by_id: &BTreeMap<i64, &TypedFeature>,
) -> Result<Vec<Point2>, WriteError> {
    let definition = model
        .composite_curve_definitions
        .iter()
        .find(|d| d.entity_id == id)
        .unwrap();
    let mut result: Vec<Point2> = Vec::new();
    for id in &definition.component_ids {
        let points = match by_id[id] {
            TypedFeature::Line(v) => vec![v.start.clone(), v.end.clone()],
            TypedFeature::Polyline(v) => v.points.clone(),
            TypedFeature::Circle(v) => sampled(&v.center, v.radius, v.radius, 0.0, 0.0, 360.0, 0),
            TypedFeature::Arc(v) => sampled(
                &v.center,
                v.radius,
                v.radius,
                0.0,
                v.start_angle_deg,
                v.end_angle_deg,
                v.direction_flag,
            ),
            TypedFeature::Ellipse(v) => sampled(
                &v.center,
                v.radius_x,
                v.radius_y,
                v.rotation_angle_deg,
                0.0,
                360.0,
                0,
            ),
            TypedFeature::EllipseArc(v) => sampled(
                &v.center,
                v.radius_x,
                v.radius_y,
                v.rotation_angle_deg,
                v.start_angle_deg,
                v.end_angle_deg,
                v.direction_flag,
            ),
            _ => return Err(error("Unsupported fill boundary geometry")),
        };
        if let Some(previous) = result.last() {
            if !near(previous, &points[0]) {
                return Err(error(
                    "P21 fill boundary components must form a connected, directed loop",
                ));
            }
            result.extend(points.into_iter().skip(1));
        } else {
            result.extend(points);
        }
    }
    if result.len() < 4 || !near(&result[0], result.last().unwrap()) {
        return Err(error("P21 fill boundary must be closed"));
    }
    Ok(result)
}
fn near(a: &Point2, b: &Point2) -> bool {
    (a.x - b.x).hypot(a.y - b.y) < 1.0e-5
}
fn sampled(
    center: &Point2,
    rx: f64,
    ry: f64,
    rotation: f64,
    start: f64,
    end: f64,
    direction: i64,
) -> Vec<Point2> {
    let sweep = if direction == 0 {
        (end - start).rem_euclid(360.0)
    } else {
        -(start - end).rem_euclid(360.0)
    };
    let sweep = if sweep == 0.0 {
        if direction == 0 {
            360.0
        } else {
            -360.0
        }
    } else {
        sweep
    };
    let (s, c) = rotation.to_radians().sin_cos();
    (0..=360)
        .map(|i| {
            let angle = (start + sweep * i as f64 / 360.0).to_radians();
            let x = rx * angle.cos();
            let y = ry * angle.sin();
            Point2 {
                x: center.x + x * c - y * s,
                y: center.y + x * s + y * c,
            }
        })
        .collect()
}
fn interior_point(polygons: &[Vec<Point2>]) -> Result<Point2, WriteError> {
    let mut ys: Vec<_> = polygons.iter().flatten().map(|p| p.y).collect();
    ys.sort_by(f64::total_cmp);
    ys.dedup();
    for pair in ys.windows(2) {
        let y = pair[0] + (pair[1] - pair[0]) / 2.0;
        if y == pair[0] || y == pair[1] {
            continue;
        }
        let mut crossings = Vec::new();
        for polygon in polygons {
            for edge in polygon.windows(2) {
                let (a, b) = (&edge[0], &edge[1]);
                if (a.y > y) != (b.y > y) {
                    crossings.push(a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y));
                }
            }
        }
        crossings.sort_by(f64::total_cmp);
        for pair in crossings.chunks_exact(2) {
            if pair[1] - pair[0] > 1.0e-6 {
                return Ok(Point2 {
                    x: pair[0] + (pair[1] - pair[0]) / 2.0,
                    y,
                });
            }
        }
    }
    Err(error("P21 fill has no interior point"))
}
