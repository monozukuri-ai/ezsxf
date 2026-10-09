//! P21 preflight shares the writer's feature and boundary rules.
use super::*;

pub(crate) fn dropped(output: &ParseOutput) -> BTreeMap<i64, String> {
    let model = output.document.sfc_model.as_ref().unwrap();
    let mut dropped = BTreeMap::new();
    for item in &output.document.typed_features {
        if matches!(&item.feature, TypedFeature::Clothoid(_))
            || matches!(&item.feature,TypedFeature::Spline(v) if v.points.len()<4 || (v.points.len()-1)%3!=0)
        {
            dropped.insert(item.id, feature_reason(&item.feature).unwrap());
        }
    }
    let references: BTreeMap<_, Vec<_>> = model
        .sfig_definitions
        .iter()
        .map(|d| {
            (
                d.entity_id,
                model
                    .sfig_references
                    .iter()
                    .filter(|r| r.definition_id == d.entity_id)
                    .map(|r| r.placement_id)
                    .collect(),
            )
        })
        .chain(
            model
                .attribute_attachments
                .iter()
                .map(|a| (a.definition_id, a.placement_ids.clone())),
        )
        .collect();
    loop {
        let before = dropped.len();
        let figures = model
            .sfig_definitions
            .iter()
            .map(|d| (d.entity_id, &d.component_ids, false))
            .chain(
                model
                    .attribute_attachments
                    .iter()
                    .map(|a| (a.definition_id, &a.component_ids, true)),
            );
        for (id, members, attribute) in figures {
            let placements = &references[&id];
            let cause = if dropped.contains_key(&id) {
                Some(id)
            } else if !placements.is_empty() && placements.iter().all(|p| dropped.contains_key(p)) {
                Some(placements[0])
            } else if !members.is_empty()
                && ((attribute && members.iter().any(|p| dropped.contains_key(p)))
                    || members.iter().all(|p| dropped.contains_key(p)))
            {
                members.iter().copied().find(|p| dropped.contains_key(p))
            } else {
                None
            };
            if let Some(cause) = cause {
                for child in std::iter::once(id)
                    .chain(members.iter().copied())
                    .chain(placements.iter().copied())
                {
                    dropped
                        .entry(child)
                        .or_insert_with(|| format!("Inside/dependent on dropped entity #{cause}"));
                }
            }
        }
        for d in &model.composite_curve_definitions {
            if let Some(cause) = std::iter::once(d.entity_id)
                .chain(d.component_ids.iter().copied())
                .find(|id| dropped.contains_key(id))
            {
                for child in std::iter::once(d.entity_id).chain(d.component_ids.iter().copied()) {
                    dropped
                        .entry(child)
                        .or_insert_with(|| format!("Depends on dropped boundary entity #{cause}"));
                }
            }
        }
        for h in &model.hatch_references {
            if let Some(cause) = std::iter::once(h.outer_definition_id)
                .chain(h.inner_definition_ids.iter().copied())
                .find(|id| dropped.contains_key(id))
            {
                dropped
                    .entry(h.hatch_id)
                    .or_insert_with(|| format!("Depends on dropped boundary entity #{cause}"));
            }
        }
        if before == dropped.len() {
            break;
        }
    }
    dropped
}

pub(crate) fn feature_reason(feature: &TypedFeature) -> Option<String> {
    let same = |a: &Point2, b: &Point2| a.x == b.x && a.y == b.y;
    let zero = match feature {
        TypedFeature::Line(v) => same(&v.start, &v.end),
        TypedFeature::LinearDim(v) => same(&v.start, &v.end),
        TypedFeature::RadiusDim(v) => same(&v.start, &v.end),
        TypedFeature::DiameterDim(v) => same(&v.start, &v.end),
        _ => false,
    };
    if zero {
        return Some("P21 cannot emit a zero-length line".into());
    }
    let extensions = match feature {
        TypedFeature::LinearDim(v) => Some([&v.extension_line1, &v.extension_line2]),
        TypedFeature::CurveDim(v) => Some([&v.extension_line1, &v.extension_line2]),
        TypedFeature::AngularDim(v) => Some([&v.extension_line1, &v.extension_line2]),
        _ => None,
    };
    if extensions.is_some_and(|lines| {
        lines
            .iter()
            .any(|line| line.present_flag == 1 && same(&line.start, &line.end))
    }) {
        return Some("P21 cannot emit a zero-length projection line".into());
    }
    let leader = match feature {
        TypedFeature::Label(v) => Some((&v.vertices, v.arrow.code)),
        TypedFeature::Balloon(v) => Some((&v.vertices, v.arrow.code)),
        _ => None,
    };
    if leader.is_some_and(|(vertices, code)| code != 0 && same(&vertices[0], &vertices[1])) {
        return Some(
            "P21 terminator direction is undefined on a zero-length leader segment".into(),
        );
    }
    match feature {
        TypedFeature::Clothoid(_) => Some("P21 output does not support clothoid_feature".into()),
        TypedFeature::Spline(v) if v.points.len() < 4 || (v.points.len() - 1) % 3 != 0 => {
            Some("P21 cubic spline needs 3n+1 control points (at least four)".into())
        }
        TypedFeature::DrawingAttribute(v) => {
            let leap =
                v.drawing_year % 4 == 0 && (v.drawing_year % 100 != 0 || v.drawing_year % 400 == 0);
            let days = match v.drawing_month {
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
            (v.drawing_year <= 0 || v.drawing_day > days)
                .then(|| "P21 drawing title requires a valid Gregorian calendar date".into())
        }
        _ => None,
    }
}

pub(crate) fn issues(output: &ParseOutput) -> Vec<(i64, String)> {
    let model = output.document.sfc_model.as_ref().unwrap();
    let by_id: BTreeMap<_, _> = output
        .document
        .typed_features
        .iter()
        .map(|f| (f.id, &f.feature))
        .collect();
    let mut result = Vec::new();
    for item in &output.document.typed_features {
        if let Some(reason) = feature_reason(&item.feature) {
            result.push((item.id, reason));
        } else if let Some((outer, holes)) =
            crate::features::hatch_composite_curve_codes(&item.feature)
        {
            let check = || {
                let mut polygons = Vec::new();
                for code in std::iter::once(outer).chain(holes.iter().copied()) {
                    let boundary = model
                        .composite_curve_definitions
                        .iter()
                        .find(|d| d.code == code)
                        .unwrap();
                    polygons.push(hatches::boundary_points(boundary.entity_id, model, &by_id)?);
                }
                hatches::interior_point(&polygons)
            };
            if let Err(reason) = check() {
                result.push((item.id, reason.to_string()));
            }
        }
    }
    result
}
