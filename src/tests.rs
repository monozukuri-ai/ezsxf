//! Rust regression tests for parser syntax, semantics, and the real corpus.

use crate::features::*;
use crate::model::*;
use crate::parser::*;
use encoding_rs::SHIFT_JIS;
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

const HEADER: &str = "ISO-10303-21;
HEADER;
FILE_DESCRIPTION(('SCADEC level2 feature_mode'),'2;1');
FILE_NAME('sample.sfc','2007-06-29T07:56:58',('author'),('organization'),'translator$$3.1','system','');
FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING'));
ENDSEC;";

fn collect_files_by_extension(root: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files_by_extension(&path, ext, out);
            continue;
        }
        if path
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|value| value.eq_ignore_ascii_case(ext))
        {
            out.push(path);
        }
    }
}

fn collect_p21_sfc_pairs() -> Vec<(PathBuf, PathBuf)> {
    let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
    if !data_root.exists() {
        return Vec::new();
    }

    let mut p21_files = Vec::new();
    let mut sfc_files = Vec::new();
    collect_files_by_extension(&data_root, "p21", &mut p21_files);
    collect_files_by_extension(&data_root, "sfc", &mut sfc_files);
    p21_files.sort();
    sfc_files.sort();

    let mut sfc_by_stem = HashMap::new();
    for path in sfc_files {
        if let Some(stem) = path.file_stem().and_then(OsStr::to_str) {
            sfc_by_stem.insert(stem.to_ascii_uppercase(), path);
        }
    }

    let mut pairs = Vec::new();
    for p21_path in p21_files {
        let Some(stem) = p21_path.file_stem().and_then(OsStr::to_str) else {
            continue;
        };
        if let Some(sfc_path) = sfc_by_stem.get(&stem.to_ascii_uppercase()) {
            pairs.push((p21_path, sfc_path.clone()));
        }
    }
    pairs
}

fn parse_file(format: FileFormat, path: &Path) -> ParseOutput {
    let bytes = fs::read(path).unwrap_or_else(|error| {
        panic!("failed to read {}: {error}", path.display());
    });
    parse_from_bytes(format, &bytes, true).unwrap_or_else(|error| {
        panic!("failed to parse {}: {error}", path.display());
    })
}

fn document_file_name_stem(document: &ParsedDocument) -> Option<String> {
    let file_name = document.header.find_keyword("FILE_NAME")?;
    let Value::String(raw_name) = file_name.parameters.first()? else {
        return None;
    };
    let stem = Path::new(raw_name)
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or(raw_name);
    Some(stem.to_ascii_uppercase())
}

#[test]
fn parse_minimal_p21() {
    let source = format!(
        "{header}
DATA;
#1=CARTESIAN_POINT(' ',(0.0,1.0));
#2=VECTOR(' ',#1,1.0);
ENDSEC;
END-ISO-10303-21;",
        header = HEADER
    );
    let output = parse_p21_text(&source, true).expect("p21 must parse");
    assert_eq!(output.document.format, FileFormat::P21);
    assert_eq!(output.document.entities.len(), 2);
    assert!(output.document.typed_features.is_empty());
    assert!(output.warnings.is_empty());
}

#[test]
fn parse_data_pairs_share_common_identity_between_p21_and_sfc() {
    let pairs = collect_p21_sfc_pairs();
    if pairs.is_empty() {
        return;
    }

    for (p21_path, sfc_path) in pairs {
        let p21 = parse_file(FileFormat::P21, &p21_path);
        let sfc = parse_file(FileFormat::Sfc, &sfc_path);

        assert!(
            !p21.document.entities.is_empty(),
            "P21 entity list must not be empty: {}",
            p21_path.display()
        );
        assert!(
            !sfc.document.entities.is_empty(),
            "SFC entity list must not be empty: {}",
            sfc_path.display()
        );
        assert_eq!(
            sfc.document.typed_features.len(),
            sfc.document.entities.len(),
            "every supported real SFC entity should have a typed representation: {}; warnings: {:?}",
            sfc_path.display(),
            sfc.warnings
        );
        assert!(
            sfc.warnings.is_empty(),
            "SFC strict parse should not produce warnings: {}; warnings: {:?}",
            sfc_path.display(),
            sfc.warnings
        );
        assert!(
            sfc.document
                .typed_features
                .iter()
                .any(|item| matches!(item.feature, TypedFeature::DrawingSheet(_))),
            "SFC must contain drawing_sheet_feature: {}",
            sfc_path.display()
        );
        let model = sfc
            .document
            .sfc_model
            .as_ref()
            .expect("SFC must expose a resolved model");
        let expected_components: HashSet<i64> = sfc
            .document
            .typed_features
            .iter()
            .filter(|item| {
                item.feature.requires_pre_sheet_order()
                    && !matches!(
                        item.feature,
                        TypedFeature::SfigOrg(_) | TypedFeature::CompositeCurve(_)
                    )
            })
            .map(|item| item.id)
            .collect();
        let mut modeled_component_list = model
            .sheet
            .iter()
            .flat_map(|sheet| sheet.component_ids.iter().copied())
            .collect::<Vec<_>>();
        modeled_component_list.extend(
            model
                .sfig_definitions
                .iter()
                .flat_map(|definition| definition.component_ids.iter().copied()),
        );
        modeled_component_list.extend(
            model
                .attribute_attachments
                .iter()
                .flat_map(|attachment| attachment.component_ids.iter().copied()),
        );
        modeled_component_list.extend(
            model
                .composite_curve_definitions
                .iter()
                .flat_map(|definition| definition.component_ids.iter().copied()),
        );
        let modeled_components: HashSet<i64> = modeled_component_list.iter().copied().collect();
        assert_eq!(
            modeled_component_list.len(),
            modeled_components.len(),
            "an SFC component must belong to exactly one container: {}",
            sfc_path.display()
        );
        assert_eq!(
            modeled_components,
            expected_components,
            "resolved SFC hierarchy lost or duplicated components: {}",
            sfc_path.display()
        );
        let attribute_placement_count = model
            .attribute_attachments
            .iter()
            .map(|attachment| attachment.placement_ids.len())
            .sum::<usize>();
        assert_eq!(
            model.sfig_references.len() + attribute_placement_count,
            sfc.document
                .typed_features
                .iter()
                .filter(|item| matches!(item.feature, TypedFeature::SfigLocate(_)))
                .count(),
            "all sfig placements must resolve: {}",
            sfc_path.display()
        );
        let expected_attribute_count = sfc
            .document
            .typed_features
            .iter()
            .filter(|item| {
                let TypedFeature::SfigOrg(feature) = &item.feature else {
                    return false;
                };
                matches!(parse_sfc_attribute_mechanism(&feature.name), Ok(Some(_)))
            })
            .count();
        assert_eq!(
            model.attribute_attachments.len(),
            expected_attribute_count,
            "all attribute naming mechanisms must be structured: {}",
            sfc_path.display()
        );
        assert_eq!(
            model.hatch_references.len(),
            sfc.document
                .typed_features
                .iter()
                .filter(|item| hatch_composite_curve_codes(&item.feature).is_some())
                .count(),
            "all hatch boundaries must resolve: {}",
            sfc_path.display()
        );

        let p21_model =
            document_file_name_stem(&p21.document).expect("P21 FILE_NAME[0] must be string");
        let sfc_model =
            document_file_name_stem(&sfc.document).expect("SFC FILE_NAME[0] must be string");
        assert_eq!(
            p21_model,
            sfc_model,
            "model identifier mismatch between {} and {}",
            p21_path.display(),
            sfc_path.display()
        );

        let expected = p21_path
            .file_stem()
            .and_then(OsStr::to_str)
            .unwrap_or_default()
            .to_ascii_uppercase();
        assert_eq!(
            p21_model,
            expected,
            "FILE_NAME model stem should match file stem for {}",
            p21_path.display()
        );
        assert!(
            p21.warnings.is_empty(),
            "P21 strict parse should not produce warnings: {}",
            p21_path.display()
        );
    }
}

#[test]
fn parse_complex_entity_instance() {
    let source = format!(
        "{header}
DATA;
#10=(
LENGTH_UNIT()
NAMED_UNIT(*)
SI_UNIT(.MILLI.,.METRE.)
);
ENDSEC;
END-ISO-10303-21;",
        header = HEADER
    );
    let output = parse_p21_text(&source, true).expect("complex record must parse");
    assert_eq!(output.document.entities.len(), 1);
    match &output.document.entities[0].body {
        EntityBody::Complex(records) => assert_eq!(records.len(), 3),
        EntityBody::Simple(_) => panic!("expected complex body"),
    }
}

#[test]
fn parse_minimal_sfc() {
    let source = format!(
        "{header}
DATA;
/*SXF
#10 = layer_feature('layer1','1')
SXF*/
/*SXF
#1 = line_feature('1','2','1','1','0.000000','0.000000','10.000000','10.000000')
SXF*/
/*SXF3.1
#2 = clothoid_feature('0','0','1','0','10')
SXF3.1*/
/*SXF
#99 = drawing_sheet_feature('sheet','9','1','100','300')
SXF*/
ENDSEC;
END-ISO-10303-21;",
        header = HEADER
    );
    let output = parse_sfc_text(&source, true).expect("sfc must parse");
    assert_eq!(output.document.format, FileFormat::Sfc);
    assert_eq!(output.document.entities.len(), 4);
    assert_eq!(output.document.typed_features.len(), 3);
    match &output.document.typed_features[1].feature {
        TypedFeature::Line(line) => {
            assert_eq!(line.start, Point2 { x: 0.0, y: 0.0 });
            assert_eq!(line.end, Point2 { x: 10.0, y: 10.0 });
        }
        other => panic!("expected line feature, got {other:?}"),
    }
}

#[test]
fn parse_sfc_maps_supported_typed_features() {
    let source = format!(
        "{header}
DATA;
/*SXF
#1 = layer_feature('layer1','1')
SXF*/
/*SXF
#2 = layer_feature('layer2','1')
SXF*/
/*SXF
#3 = layer_feature('layer3','1')
SXF*/
/*SXF
#10 = polyline_feature('1','4','3','1','4','(-5.000000,5.000000,5.000000,-5.000000)','(0.000000,0.000000,5.000000,0.000000)')
SXF*/
/*SXF
#11 = composite_curve_org_feature('8','8','2','1')
SXF*/
/*SXF
#12 = circle_feature('3','1','5','2','0.000000','0.000000','10.000000')
SXF*/
/*SXF
#13 = arc_feature('2','1','7','1','0.000000','0.000000','10.000000','0','0.00000000000000','90.0000000000000')
SXF*/
/*SXF
#14 = text_string_feature('2','4','1','text10','0.000000','0.000000','5.000000','20.000000','1.000000','0.00000000000000','0.00000000000000','1','2')
SXF*/
/*SXF
#15 = line_feature('2','2','1','1','0.000000','0.000000','10.000000','10.000000')
SXF*/
/*SXF
#99 = drawing_sheet_feature('sheet','9','1','100','300')
SXF*/
ENDSEC;
END-ISO-10303-21;",
        header = HEADER
    );

    let output = parse_sfc_text(&source, true).expect("typed feature mapping must parse");
    assert_eq!(output.document.typed_features.len(), 10);

    assert!(matches!(
        output.document.typed_features[3].feature,
        TypedFeature::Polyline(_)
    ));
    assert!(matches!(
        output.document.typed_features[4].feature,
        TypedFeature::CompositeCurve(_)
    ));
    assert!(matches!(
        output.document.typed_features[5].feature,
        TypedFeature::Circle(_)
    ));
    assert!(matches!(
        output.document.typed_features[6].feature,
        TypedFeature::Arc(_)
    ));
    assert!(matches!(
        output.document.typed_features[7].feature,
        TypedFeature::Text(_)
    ));
    assert!(matches!(
        output.document.typed_features[8].feature,
        TypedFeature::Line(_)
    ));
    assert!(matches!(
        output.document.typed_features[9].feature,
        TypedFeature::DrawingSheet(_)
    ));
}

#[test]
fn parse_sfc_maps_additional_geometry_features() {
    let source = format!(
        "{header}
DATA;
/*SXF
#1 = layer_feature('layer1','1')
SXF*/
/*SXF
#2 = layer_feature('layer2','1')
SXF*/
/*SXF
#3 = layer_feature('layer3','1')
SXF*/
/*SXF
#10 = point_marker_feature('1','1','0.000000','0.000000','7','0.00000000000000','1.000000')
SXF*/
/*SXF
#11 = ellipse_feature('2','1','5','2','0.000000','0.000000','10.000000','20.000000','0.00000000000000')
SXF*/
/*SXF
#12 = ellipse_arc_feature('3','1','5','2','0.000000','0.000000','10.000000','20.000000','1','0.00000000000000','0.00000000000000','90.0000000000000')
SXF*/
/*SXF
#13 = spline_feature('1','3','3','3','1','4','(1.000000,2.000000,4.000000,3.000000)','(0.000000,-1.000000,3.000000,5.000000)')
SXF*/
/*SXF3.1
#14 = clothoid_feature('2','1','7','1','0.000000','0.000000','100.000000','0','0.00000000000000','0.000000','100.000000')
SXF3.1*/
/*SXF
#99 = drawing_sheet_feature('sheet','9','1','100','300')
SXF*/
ENDSEC;
END-ISO-10303-21;",
        header = HEADER
    );

    let output = parse_sfc_text(&source, true).expect("additional geometry features must parse");

    assert!(output
        .document
        .typed_features
        .iter()
        .any(|item| matches!(item.feature, TypedFeature::PointMarker(_))));
    assert!(output
        .document
        .typed_features
        .iter()
        .any(|item| matches!(item.feature, TypedFeature::Ellipse(_))));
    assert!(output
        .document
        .typed_features
        .iter()
        .any(|item| matches!(item.feature, TypedFeature::EllipseArc(_))));
    assert!(output
        .document
        .typed_features
        .iter()
        .any(|item| matches!(item.feature, TypedFeature::Spline(_))));
    assert!(output
        .document
        .typed_features
        .iter()
        .any(|item| matches!(item.feature, TypedFeature::Clothoid(_))));
}

#[test]
fn parse_sfc_maps_structured_and_hatch_features() {
    let no_extension = "'0','0.000000','0.000000','0.000000','0.000000','0.000000','0.000000'";
    let no_dimension_arrow = "'0','0','0.000000','0.000000','0.000000'";
    let no_text = "'0','0','','0.000000','0.000000','0.000000','0.000000','0.000000','0.00000000000000','0.00000000000000','0','0'";
    let source = format!(
        "{header}
DATA;
/*SXF
#1 = layer_feature('layer1','1')
SXF*/
/*SXF
#2 = layer_feature('layer2','1')
SXF*/
/*SXF
#3 = layer_feature('layer3','1')
SXF*/
/*SXF
#4 = polyline_feature('1','1','1','1','4','(0.000000,10.000000,10.000000,0.000000)','(0.000000,0.000000,10.000000,0.000000)')
SXF*/
/*SXF
#5 = composite_curve_feature('1','1','1','1')
SXF*/
/*SXF
#6 = polyline_feature('1','1','1','1','4','(2.000000,8.000000,8.000000,2.000000)','(2.000000,2.000000,8.000000,2.000000)')
SXF*/
/*SXF
#7 = composite_curve_feature('1','1','1','1')
SXF*/
/*SXF
#10 = sfig_org_feature('sfig','4')
SXF*/
/*SXF
#11 = sfig_locate_feature('1','sfig','0.000000','0.000000','0.00000000000000','2.00000000000000','2.00000000000000')
SXF*/
/*SXF
#12 = symbol_externally_defined_feature('1','1','11','40201050100000&&a0010010','1.300000','1.400000','0.00000000000000','1.00000000000000')
SXF*/
/*SXF
#13 = linear_dim_feature('3','1','3','1','0.000000','0.000000','10.000000','0.000000',{no_extension},{no_extension},{no_dimension_arrow},{no_dimension_arrow},{no_text})
SXF*/
/*SXF3.1
#14 = curve_dim_feature('3','1','3','1','0.000000','0.000000','10.000000','0.00000000000000','90.0000000000000',{no_extension},{no_extension},{no_dimension_arrow},{no_dimension_arrow},{no_text})
SXF3.1*/
/*SXF
#15 = angular_dim_feature('3','1','3','1','0.000000','0.000000','10.000000','0.00000000000000','90.0000000000000',{no_extension},{no_extension},{no_dimension_arrow},{no_dimension_arrow},{no_text})
SXF*/
/*SXF
#16 = radius_dim_feature('3','1','3','1','0.000000','0.000000','10.000000','0.000000',{no_dimension_arrow},{no_text})
SXF*/
/*SXF
#17 = diameter_dim_feature('3','1','3','1','0.000000','0.000000','10.000000','0.000000',{no_dimension_arrow},{no_dimension_arrow},{no_text})
SXF*/
/*SXF
#18 = label_feature('1','1','1','1','2','(1.000000,3.000000)','(2.000000,4.000000)','1','3.00000000000000',{no_text})
SXF*/
/*SXF
#19 = balloon_feature('1','1','1','1','2','(1.000000,3.000000)','(2.000000,4.000000)','0.000000','0.000000','10.000000','4','3.00000000000000',{no_text})
SXF*/
/*SXF
#20 = externally_defined_hatch_feature('2','sxf_hatch_style_1','1','1','(2)')
SXF*/
/*SXF
#21 = fill_area_style_colour_feature('2','8','1','1','(2)')
SXF*/
/*SXF
#22 = fill_area_style_hatching_feature('3','2','(1,3,2,11.000000,12.000000,5.000000,60.0000000000000)','(10,1,1,1.000000,2.000000,5.000000,30.0000000000000)','1','1','(2)')
SXF*/
/*SXF
#23 = fill_area_style_tiles_hatching_feature('3','40201050100000&&a0010010','2','11.000000','12.000000','13.000000','30.0000000000000','15.000000','60.0000000000000','1.00000000000000','1.00000000000000','45.0000000000000','1','1','(2)')
SXF*/
/*SXF
#99 = drawing_sheet_feature('sheet','9','1','100','300')
SXF*/
ENDSEC;
END-ISO-10303-21;",
        header = HEADER,
        no_extension = no_extension,
        no_dimension_arrow = no_dimension_arrow,
        no_text = no_text
    );

    let output = parse_sfc_text(&source, true).expect("structured and hatch features must parse");

    let symbol = output
        .document
        .typed_features
        .iter()
        .find(|item| item.id == 12)
        .expect("symbol must exist");
    match &symbol.feature {
        TypedFeature::ExternallyDefinedSymbol(feature) => {
            assert_eq!(feature.color_flag, 1);
            assert_eq!(feature.style.layer_code, Some(1));
            assert_eq!(feature.style.color_code, Some(11));
        }
        other => panic!("expected externally defined symbol, got {other:?}"),
    }
    let linear = output
        .document
        .typed_features
        .iter()
        .find(|item| item.id == 13)
        .expect("linear dimension must exist");
    match &linear.feature {
        TypedFeature::LinearDim(feature) => {
            assert_eq!(feature.start, Point2 { x: 0.0, y: 0.0 });
            assert_eq!(feature.end, Point2 { x: 10.0, y: 0.0 });
            assert_eq!(feature.raw_parameters.len(), 40);
        }
        other => panic!("expected linear dimension, got {other:?}"),
    }

    assert!(output
        .document
        .typed_features
        .iter()
        .any(|item| matches!(item.feature, TypedFeature::SfigOrg(_))));
    assert!(output
        .document
        .typed_features
        .iter()
        .any(|item| matches!(item.feature, TypedFeature::SfigLocate(_))));
    assert!(output
        .document
        .typed_features
        .iter()
        .any(|item| matches!(item.feature, TypedFeature::ExternallyDefinedSymbol(_))));
    assert!(output
        .document
        .typed_features
        .iter()
        .any(|item| matches!(item.feature, TypedFeature::LinearDim(_))));
    assert!(output
        .document
        .typed_features
        .iter()
        .any(|item| matches!(item.feature, TypedFeature::CurveDim(_))));
    assert!(output
        .document
        .typed_features
        .iter()
        .any(|item| matches!(item.feature, TypedFeature::AngularDim(_))));
    assert!(output
        .document
        .typed_features
        .iter()
        .any(|item| matches!(item.feature, TypedFeature::RadiusDim(_))));
    assert!(output
        .document
        .typed_features
        .iter()
        .any(|item| matches!(item.feature, TypedFeature::DiameterDim(_))));
    assert!(output
        .document
        .typed_features
        .iter()
        .any(|item| matches!(item.feature, TypedFeature::Label(_))));
    assert!(output
        .document
        .typed_features
        .iter()
        .any(|item| matches!(item.feature, TypedFeature::Balloon(_))));
    assert!(output
        .document
        .typed_features
        .iter()
        .any(|item| matches!(item.feature, TypedFeature::ExternallyDefinedHatch(_))));
    assert!(output
        .document
        .typed_features
        .iter()
        .any(|item| matches!(item.feature, TypedFeature::FillAreaStyleColour(_))));
    assert!(output
        .document
        .typed_features
        .iter()
        .any(|item| matches!(item.feature, TypedFeature::FillAreaStyleHatching(_))));
    assert!(output
        .document
        .typed_features
        .iter()
        .any(|item| matches!(item.feature, TypedFeature::FillAreaStyleTiles(_))));

    let model = output
        .document
        .sfc_model
        .as_ref()
        .expect("SFC must expose a resolved model");
    assert_eq!(model.composite_curve_definitions.len(), 2);
    assert_eq!(model.composite_curve_definitions[0].component_ids, vec![4]);
    assert_eq!(model.sfig_definitions.len(), 1);
    assert_eq!(model.sfig_references.len(), 1);
    assert_eq!(model.sfig_references[0].definition_id, 10);
    assert_eq!(model.hatch_references.len(), 4);
    assert_eq!(
        model.sheet.as_ref().expect("sheet must resolve").entity_id,
        99
    );
}

#[test]
fn parse_sfc_attribute_naming_mechanisms() {
    assert_eq!(
        parse_sfc_attribute_mechanism("$$ATRF$$42$$sample.saf").unwrap(),
        Some(SfcAttributeMechanism::AttributeFile {
            figure_id: "42".to_string(),
            attribute_file_name: Some("sample.saf".to_string()),
        })
    );
    assert_eq!(
        parse_sfc_attribute_mechanism("$$ATRU$$7$$等高線$$等高線$$12.5$$LEN$$m").unwrap(),
        Some(SfcAttributeMechanism::SingleAttribute {
            figure_id: "7".to_string(),
            figure_name: Some("等高線".to_string()),
            attribute_name: Some("等高線".to_string()),
            attribute_value: Some("12.5".to_string()),
            attribute_type: Some("LEN".to_string()),
            unit: Some("m".to_string()),
        })
    );
    assert_eq!(
        parse_sfc_attribute_mechanism("$$ATRS$$9$$表題_事業名$$STR").unwrap(),
        Some(SfcAttributeMechanism::TextAttribute {
            figure_id: "9".to_string(),
            attribute_name: "表題_事業名".to_string(),
            attribute_type: Some("STR".to_string()),
            unit: None,
        })
    );
    assert!(parse_sfc_attribute_mechanism("ordinary-group")
        .unwrap()
        .is_none());
    assert!(parse_sfc_attribute_mechanism("$$ATRF-custom-group")
        .unwrap()
        .is_none());
    assert!(parse_sfc_attribute_mechanism("$$ATRF$$").is_err());
    assert!(parse_sfc_attribute_mechanism("$$ATRU$$1$$$$name").is_err());
    assert!(parse_sfc_attribute_mechanism("$$ATRS$$1").is_err());
}

#[test]
fn parse_sfc_resolves_attribute_attachment_separately_from_groups() {
    let source = format!(
        "{header}
DATA;
/*SXF
#1 = line_feature('1','1','1','1','0.000000','0.000000','10.000000','10.000000')
SXF*/
/*SXF
#2 = sfig_org_feature('$$ATRU$$42$$等高線$$等高線$$12.5$$LEN$$m','3')
SXF*/
/*SXF
#3 = sfig_locate_feature('0','$$ATRU$$42$$等高線$$等高線$$12.5$$LEN$$m','0.000000','0.000000','0.00000000000000','1.00000000000000','1.00000000000000')
SXF*/
/*SXF
#99 = drawing_sheet_feature('sheet','9','1','100','300')
SXF*/
ENDSEC;
END-ISO-10303-21;",
        header = HEADER
    );

    let output = parse_sfc_text(&source, true).expect("attribute attachment must parse");
    let model = output
        .document
        .sfc_model
        .as_ref()
        .expect("SFC model must resolve");
    assert!(model.sfig_definitions.is_empty());
    assert!(model.sfig_references.is_empty());
    assert_eq!(model.attribute_attachments.len(), 1);
    let attachment = &model.attribute_attachments[0];
    assert_eq!(attachment.definition_id, 2);
    assert_eq!(attachment.component_ids, vec![1]);
    assert_eq!(attachment.placement_ids, vec![3]);
    assert!(matches!(
        attachment.mechanism,
        SfcAttributeMechanism::SingleAttribute { ref figure_id, .. } if figure_id == "42"
    ));
    assert_eq!(
        model
            .sheet
            .as_ref()
            .expect("sheet must resolve")
            .component_ids,
        vec![3]
    );
}

#[test]
fn parse_sfc_resolves_default_saf_file_name() {
    let source = format!(
        "{header}
DATA;
/*SXF
#1 = line_feature('1','1','1','1','0.000000','0.000000','10.000000','10.000000')
SXF*/
/*SXF
#2 = sfig_org_feature('$$ATRF$$42','3')
SXF*/
/*SXF
#3 = sfig_locate_feature('0','$$ATRF$$42','0.000000','0.000000','0.00000000000000','1.00000000000000','1.00000000000000')
SXF*/
/*SXF
#99 = drawing_sheet_feature('sheet','9','1','100','300')
SXF*/
ENDSEC;
END-ISO-10303-21;",
        header = HEADER
    );

    let output = parse_sfc_text(&source, true).expect("ATRF attachment must parse");
    let attachment = &output
        .document
        .sfc_model
        .as_ref()
        .expect("SFC model must resolve")
        .attribute_attachments[0];
    assert_eq!(
        attachment.resolved_attribute_file_name.as_deref(),
        Some("sample.SAF")
    );
    assert!(matches!(
        attachment.mechanism,
        SfcAttributeMechanism::AttributeFile {
            attribute_file_name: None,
            ..
        }
    ));
}

#[test]
fn strict_sfc_rejects_non_identity_drawing_group_placement() {
    let source = format!(
        "{header}
DATA;
/*SXF
#1 = line_feature('1','1','1','1','0.000000','0.000000','10.000000','10.000000')
SXF*/
/*SXF
#2 = sfig_org_feature('group','3')
SXF*/
/*SXF
#3 = sfig_locate_feature('0','group','1.000000','0.000000','0.00000000000000','1.00000000000000','1.00000000000000')
SXF*/
/*SXF
#99 = drawing_sheet_feature('sheet','9','1','100','300')
SXF*/
ENDSEC;
END-ISO-10303-21;",
        header = HEADER
    );

    let error =
        parse_sfc_text(&source, true).expect_err("drawing-group placement transform must be fixed");
    assert!(error.message.contains("Drawing-group placement"));
}

#[test]
fn strict_sfc_rejects_unused_composite_figure_definition() {
    let source = format!(
        "{header}
DATA;
/*SXF
#1 = line_feature('1','1','1','1','0.000000','0.000000','10.000000','10.000000')
SXF*/
/*SXF
#2 = sfig_org_feature('unused-part','4')
SXF*/
/*SXF
#99 = drawing_sheet_feature('sheet','9','1','100','300')
SXF*/
ENDSEC;
END-ISO-10303-21;",
        header = HEADER
    );

    let error =
        parse_sfc_text(&source, true).expect_err("unused composite figure definition must fail");
    assert!(error.message.contains("is not used by a placement"));
}

#[test]
fn strict_sfc_rejects_partial_drawing_nested_in_group() {
    let source = format!(
        "{header}
DATA;
/*SXF
#1 = line_feature('1','1','1','1','0.000000','0.000000','10.000000','10.000000')
SXF*/
/*SXF
#2 = sfig_org_feature('partial','1')
SXF*/
/*SXF
#3 = sfig_locate_feature('0','partial','0.000000','0.000000','0.00000000000000','1.00000000000000','1.00000000000000')
SXF*/
/*SXF
#4 = sfig_org_feature('group','3')
SXF*/
/*SXF
#5 = sfig_locate_feature('0','group','0.000000','0.000000','0.00000000000000','1.00000000000000','1.00000000000000')
SXF*/
/*SXF
#99 = drawing_sheet_feature('sheet','9','1','100','300')
SXF*/
ENDSEC;
END-ISO-10303-21;",
        header = HEADER
    );

    let error = parse_sfc_text(&source, true).expect_err("partial drawing must be on the sheet");
    assert!(error.message.contains("invalid parent container"));
}

#[test]
fn parse_sfc_maps_drawing_info_and_style_definitions() {
    let source = format!(
        "{header}
DATA;
/*SXF3
#1 = drawing_attribute_feature('project','construction','contract','title','1','plan','1:500','2024','12','25','contractor','owner')
SXF3*/
/*SXF
#2 = pre_defined_font_feature('continuous')
SXF*/
/*SXF
#3 = user_defined_font_feature('$$SXF_dashed','4','(5.500000,3.500000,2.500000,1.500000)')
SXF*/
/*SXF
#4 = pre_defined_colour_feature('red')
SXF*/
/*SXF
#5 = user_defined_colour_feature('255','255','0')
SXF*/
/*SXF
#6 = width_feature('0.130000')
SXF*/
/*SXF
#7 = text_font_feature('Century')
SXF*/
/*SXF
#8 = layer_feature('layer1','1')
SXF*/
/*SXF
#9 = line_feature('1','17','17','1','0.000000','0.000000','10.000000','10.000000')
SXF*/
/*SXF
#99 = drawing_sheet_feature('sheet','9','1','100','300')
SXF*/
ENDSEC;
END-ISO-10303-21;",
        header = HEADER
    );

    let output = parse_sfc_text(&source, true).expect("drawing/style definitions must parse");
    assert_eq!(output.document.typed_features.len(), 10);

    assert!(output
        .document
        .typed_features
        .iter()
        .any(|item| matches!(item.feature, TypedFeature::DrawingAttribute(_))));
    assert!(output
        .document
        .typed_features
        .iter()
        .any(|item| matches!(item.feature, TypedFeature::PreDefinedFont(_))));
    assert!(output
        .document
        .typed_features
        .iter()
        .any(|item| matches!(item.feature, TypedFeature::UserDefinedFont(_))));
    assert!(output
        .document
        .typed_features
        .iter()
        .any(|item| matches!(item.feature, TypedFeature::PreDefinedColour(_))));
    assert!(output
        .document
        .typed_features
        .iter()
        .any(|item| matches!(item.feature, TypedFeature::UserDefinedColour(_))));
    assert!(output
        .document
        .typed_features
        .iter()
        .any(|item| matches!(item.feature, TypedFeature::Width(_))));
    assert!(output
        .document
        .typed_features
        .iter()
        .any(|item| matches!(item.feature, TypedFeature::TextFont(_))));
    let tables = &output
        .document
        .sfc_model
        .as_ref()
        .expect("SFC model must exist")
        .code_tables;
    assert_eq!(
        tables
            .line_types
            .iter()
            .map(|binding| binding.code)
            .collect::<Vec<_>>(),
        vec![1, 17]
    );
    assert_eq!(
        tables
            .colors
            .iter()
            .map(|binding| binding.code)
            .collect::<Vec<_>>(),
        vec![2, 17]
    );
    assert_eq!(tables.line_widths[0].code, 1);
    assert_eq!(tables.text_fonts[0].code, 1);
}

#[test]
fn strict_mode_skips_incomplete_sfc_feature_with_warning() {
    let source = format!(
        "{header}
DATA;
/*SXF
#1 = layer_feature('layer1','1')
SXF*/
/*SXF
#2 = line_feature('1','2','1')
SXF*/
/*SXF
#3 = line_feature('1','2','1','1','0.000000','0.000000','10.000000','10.000000')
SXF*/
/*SXF
#99 = drawing_sheet_feature('sheet','9','1','100','300')
SXF*/
ENDSEC;
END-ISO-10303-21;",
        header = HEADER
    );

    let output = parse_sfc_text(&source, true).expect("strict mode should skip incomplete feature");
    assert!(output
        .warnings
        .iter()
        .any(|warning| warning.code == "sfc-incomplete-feature-skipped"));

    assert_eq!(
        output
            .document
            .typed_features
            .iter()
            .filter(|item| matches!(item.feature, TypedFeature::Line(_)))
            .count(),
        1
    );
}

#[test]
fn lenient_mode_recovers_from_malformed_sfc_block() {
    let source = format!(
        "{header}
DATA;
/*SXF
#1 = layer_feature('layer1','1')
SXF*/
/*SXF
#2 = line_feature('1','2','1','1','0.000000','0.000000','10.000000'
SXF*/
/*SXF
#3 = line_feature('1','2','1','1','0.000000','0.000000','20.000000','20.000000')
SXF*/
/*SXF
#99 = drawing_sheet_feature('sheet','9','1','100','300')
SXF*/
ENDSEC;
END-ISO-10303-21;",
        header = HEADER
    );

    let output = parse_sfc_text(&source, false).expect("lenient mode should recover");
    assert!(output
        .warnings
        .iter()
        .any(|warning| warning.code == "sfc-feature-block-skipped"));
    assert_eq!(
        output
            .document
            .typed_features
            .iter()
            .filter(|item| matches!(item.feature, TypedFeature::Line(_)))
            .count(),
        1
    );
}

#[test]
fn strict_mode_rejects_sfc_missing_sheet() {
    let source = format!(
        "{header}
DATA;
/*SXF
#1 = layer_feature('layer1','1')
SXF*/
/*SXF
#2 = line_feature('1','2','1','1','0.000000','0.000000','10.000000','10.000000')
SXF*/
ENDSEC;
END-ISO-10303-21;",
        header = HEADER
    );
    let error = parse_sfc_text(&source, true).expect_err("strict mode should fail");
    assert!(error.message.contains("drawing_sheet_feature"));
}

#[test]
fn lenient_mode_warns_for_sfc_missing_sheet() {
    let source = format!(
        "{header}
DATA;
/*SXF
#1 = layer_feature('layer1','1')
SXF*/
/*SXF
#2 = line_feature('1','2','1','1','0.000000','0.000000','10.000000','10.000000')
SXF*/
ENDSEC;
END-ISO-10303-21;",
        header = HEADER
    );
    let output = parse_sfc_text(&source, false).expect("lenient mode should continue");
    assert!(output
        .warnings
        .iter()
        .any(|warning| warning.code == "sfc-missing-drawing-sheet"));
}

#[test]
fn strict_mode_rejects_undefined_sfc_layer_code() {
    let source = format!(
        "{header}
DATA;
/*SXF
#1 = layer_feature('layer1','1')
SXF*/
/*SXF
#2 = line_feature('2','2','1','1','0.000000','0.000000','10.000000','10.000000')
SXF*/
/*SXF
#99 = drawing_sheet_feature('sheet','9','1','100','300')
SXF*/
ENDSEC;
END-ISO-10303-21;",
        header = HEADER
    );
    let error = parse_sfc_text(&source, true).expect_err("strict mode should fail");
    assert!(error.message.contains("undefined layer code"));
}

#[test]
fn strict_mode_rejects_undefined_sfc_line_type_code() {
    let source = format!(
        "{header}
DATA;
/*SXF
#1 = layer_feature('layer1','1')
SXF*/
/*SXF
#2 = pre_defined_colour_feature('red')
SXF*/
/*SXF
#3 = pre_defined_font_feature('continuous')
SXF*/
/*SXF
#4 = width_feature('0.130000')
SXF*/
/*SXF
#5 = line_feature('1','1','16','1','0.000000','0.000000','10.000000','10.000000')
SXF*/
/*SXF
#99 = drawing_sheet_feature('sheet','9','1','100','300')
SXF*/
ENDSEC;
END-ISO-10303-21;",
        header = HEADER
    );
    let error = parse_sfc_text(&source, true).expect_err("strict mode should fail");
    assert!(error.message.contains("undefined line type code"));
}

#[test]
fn strict_mode_allows_zero_style_codes() {
    let source = format!(
        "{header}
DATA;
/*SXF
#1 = line_feature('0','0','0','0','0.000000','0.000000','10.000000','10.000000')
SXF*/
/*SXF
#99 = drawing_sheet_feature('sheet','9','1','100','300')
SXF*/
ENDSEC;
END-ISO-10303-21;",
        header = HEADER
    );
    let output = parse_sfc_text(&source, true).expect("strict mode should allow zero codes");
    assert_eq!(output.document.typed_features.len(), 2);
}

#[test]
fn parse_sfc_accepts_backslash_quoted_strings() {
    let source = format!(
        "{header}
DATA;
/*SXF
#1 = pre_defined_colour_feature(\\'red\\')
SXF*/
/*SXF
#2 = drawing_sheet_feature(\\'sheet\\',\\'9\\',\\'1\\',\\'100\\',\\'300\\')
SXF*/
ENDSEC;
END-ISO-10303-21;",
        header = HEADER
    );
    let output =
        parse_sfc_text(&source, true).expect("backslash-quoted strings should parse in SFC");
    assert!(output.document.typed_features.iter().any(|item| {
        matches!(
            item.feature,
            TypedFeature::PreDefinedColour(PreDefinedColourFeature { ref name })
                if name == "red"
        )
    }));
}

#[test]
fn parse_sfc_keeps_quote_inside_backslash_quoted_string() {
    let source = format!(
        "{header}
DATA;
/*SXF
#1 = text_string_feature('1','1','1',\\'A'B\\','0.000000','0.000000','5.000000','20.000000','1.000000','0.00000000000000','0.00000000000000','1','1')
SXF*/
/*SXF
#99 = drawing_sheet_feature('sheet','9','1','100','300')
SXF*/
ENDSEC;
END-ISO-10303-21;",
        header = HEADER
    );
    let output =
        parse_sfc_text(&source, true).expect("quote inside backslash-quoted string should parse");
    assert!(output.document.typed_features.iter().any(|item| {
        matches!(
            item.feature,
            TypedFeature::Text(TextFeature { ref text, .. }) if text == "A'B"
        )
    }));
}

#[test]
fn strict_mode_rejects_undefined_reference() {
    let source = format!(
        "{header}
DATA;
#1=VECTOR(' ',#99,1.0);
ENDSEC;
END-ISO-10303-21;",
        header = HEADER
    );
    let error = parse_p21_text(&source, true).expect_err("strict mode should fail");
    assert!(error.message.contains("undefined entity #99"));
}

#[test]
fn lenient_mode_keeps_warning_for_undefined_reference() {
    let source = format!(
        "{header}
DATA;
#1=VECTOR(' ',#99,1.0);
ENDSEC;
END-ISO-10303-21;",
        header = HEADER
    );
    let output = parse_p21_text(&source, false).expect("lenient mode should pass");
    assert_eq!(output.document.entities.len(), 1);
    assert!(output
        .warnings
        .iter()
        .any(|warning| warning.code == "undefined-reference"));
}

#[test]
fn parse_sfc_decodes_shift_jis_without_replacement() {
    let source = format!(
        "{header}
DATA;
/*SXF
#1 = layer_feature(\'日本語レイヤ\','1')
SXF*/
/*SXF
#99 = drawing_sheet_feature(\'図面\','9','1','100','300')
SXF*/
ENDSEC;
END-ISO-10303-21;",
        header = HEADER
    );
    let (bytes, _, had_errors) = SHIFT_JIS.encode(&source);
    assert!(!had_errors);
    let output = parse_from_bytes(FileFormat::Sfc, &bytes, true)
        .expect("Shift-JIS SFC must parse losslessly");
    assert!(output.warnings.is_empty());
    assert!(output.document.typed_features.iter().any(|item| {
        matches!(
            &item.feature,
            TypedFeature::Layer(LayerFeature { name, .. }) if name == "日本語レイヤ"
        )
    }));
}

#[test]
fn strict_sfc_rejects_invalid_shift_jis() {
    let error = decode_bytes(FileFormat::Sfc, &[0x81], true)
        .expect_err("an incomplete Shift-JIS lead byte must fail");
    assert!(error.message.contains("Shift-JIS"));
}

#[test]
fn strict_sfc_rejects_wrong_feature_version_marker() {
    let source = format!(
        "{header}
DATA;
/*SXF
#1 = drawing_attribute_feature('','','','','','','','2024','1','1','','')
SXF*/
/*SXF
#99 = drawing_sheet_feature('sheet','9','1','100','300')
SXF*/
ENDSEC;
END-ISO-10303-21;",
        header = HEADER
    );
    let error = parse_sfc_text(&source, true).expect_err("wrong feature marker must fail");
    assert!(error.message.contains("must use /*SXF3"));
}

#[test]
fn strict_sfc_rejects_forward_sfig_reference() {
    let source = format!(
        "{header}
DATA;
/*SXF
#1 = layer_feature('layer1','1')
SXF*/
/*SXF
#2 = sfig_locate_feature('1','later','0.000000','0.000000','0.00000000000000','1.00000000000000','1.00000000000000')
SXF*/
/*SXF
#3 = sfig_org_feature('later','4')
SXF*/
/*SXF
#99 = drawing_sheet_feature('sheet','9','1','100','300')
SXF*/
ENDSEC;
END-ISO-10303-21;",
        header = HEADER
    );
    let error = parse_sfc_text(&source, true).expect_err("forward sfig reference must fail");
    assert!(error.message.contains("undefined or later definition"));
}

#[test]
fn strict_sfc_rejects_undefined_hatch_boundary() {
    let source = format!(
        "{header}
DATA;
/*SXF
#1 = layer_feature('layer1','1')
SXF*/
/*SXF
#2 = fill_area_style_colour_feature('1','1','1','0','()')
SXF*/
/*SXF
#99 = drawing_sheet_feature('sheet','9','1','100','300')
SXF*/
ENDSEC;
END-ISO-10303-21;",
        header = HEADER
    );
    let error = parse_sfc_text(&source, true).expect_err("undefined hatch boundary must fail");
    assert!(error.message.contains("outer composite curve code 1"));
}

#[test]
fn sfc_skips_quoted_exponent_and_oversized_text() {
    let oversized = "A".repeat(257);
    let source = format!(
        "{header}
DATA;
/*SXF
#1 = layer_feature('layer1','1')
SXF*/
/*SXF
#2 = line_feature('1','1','1','1','0.000000','0.000000','1e3','10.000000')
SXF*/
/*SXF
#3 = text_string_feature('1','1','0','{oversized}','0.000000','0.000000','5.000000','20.000000','1.000000','0.00000000000000','0.00000000000000','1','1')
SXF*/
/*SXF
#99 = drawing_sheet_feature('sheet','9','1','100','300')
SXF*/
ENDSEC;
END-ISO-10303-21;",
        header = HEADER,
        oversized = oversized
    );
    let output = parse_sfc_text(&source, true)
        .expect("invalid individual features must be skipped according to the SFC spec");
    assert_eq!(
        output
            .document
            .typed_features
            .iter()
            .filter(|item| matches!(item.feature, TypedFeature::Line(_) | TypedFeature::Text(_)))
            .count(),
        0
    );
    assert_eq!(
        output
            .warnings
            .iter()
            .filter(|warning| {
                matches!(
                    warning.code.as_str(),
                    "sfc-incomplete-feature-skipped" | "sfc-invalid-feature-skipped"
                )
            })
            .count(),
        2
    );
}
