#![allow(clippy::useless_conversion)]

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use pyo3::exceptions::{PyIOError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList};

// SXF Ver.3.1 common predefined element code ranges (別冊「共通既定義要素編」).
const COMMON_PREDEFINED_LINE_TYPE_MAX_CODE: i64 = 8;
const COMMON_PREDEFINED_LINE_WIDTH_MAX_CODE: i64 = 8;
const COMMON_PREDEFINED_COLOR_MAX_CODE: i64 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFormat {
    P21,
    Sfc,
}

impl FileFormat {
    fn as_str(self) -> &'static str {
        match self {
            Self::P21 => "p21",
            Self::Sfc => "sfc",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Unset,
    Omitted,
    Integer(i64),
    Real(f64),
    String(String),
    Enum(String),
    Reference(i64),
    Binary(String),
    List(Vec<Value>),
    Typed {
        keyword: String,
        parameters: Vec<Value>,
    },
}

impl Value {
    fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Integer(v) => Some(*v),
            Value::Real(v) => {
                if v.is_finite() && v.fract() == 0.0 {
                    let rounded = *v as i64;
                    if (rounded as f64 - *v).abs() < f64::EPSILON {
                        Some(rounded)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            Value::String(v) => v.trim().parse::<i64>().ok(),
            _ => None,
        }
    }

    fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Integer(v) => Some(*v as f64),
            Value::Real(v) => Some(*v),
            Value::String(v) => v.trim().parse::<f64>().ok(),
            _ => None,
        }
    }

    fn as_string(&self) -> Option<String> {
        match self {
            Value::String(v) => Some(v.clone()),
            _ => None,
        }
    }

    fn as_f64_list(&self) -> Option<Vec<f64>> {
        match self {
            Value::List(values) => {
                let mut out = Vec::with_capacity(values.len());
                for value in values {
                    out.push(value.as_f64()?);
                }
                Some(out)
            }
            Value::String(text) => parse_f64_list_from_string(text),
            _ => None,
        }
    }

    fn as_i64_list(&self) -> Option<Vec<i64>> {
        match self {
            Value::List(values) => {
                let mut out = Vec::with_capacity(values.len());
                for value in values {
                    out.push(value.as_i64()?);
                }
                Some(out)
            }
            Value::String(text) => parse_i64_list_from_string(text),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub keyword: String,
    pub parameters: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EntityBody {
    Simple(Record),
    Complex(Vec<Record>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityInstance {
    pub id: i64,
    pub body: EntityBody,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HeaderSection {
    pub entities: Vec<Record>,
}

impl HeaderSection {
    pub fn find_keyword(&self, keyword: &str) -> Option<&Record> {
        self.entities
            .iter()
            .find(|record| record.keyword.eq_ignore_ascii_case(keyword))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedDocument {
    pub format: FileFormat,
    pub header: HeaderSection,
    pub entities: Vec<EntityInstance>,
    pub typed_features: Vec<TypedFeatureInstance>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypedFeatureInstance {
    pub id: i64,
    pub keyword: String,
    pub feature: TypedFeature,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CommonStyle {
    pub layer_code: Option<i64>,
    pub color_code: Option<i64>,
    pub line_type_code: Option<i64>,
    pub line_width_code: Option<i64>,
    pub font_code: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Point2 {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LineFeature {
    pub style: CommonStyle,
    pub start: Point2,
    pub end: Point2,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolylineFeature {
    pub style: CommonStyle,
    pub declared_point_count: Option<usize>,
    pub points: Vec<Point2>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CircleFeature {
    pub style: CommonStyle,
    pub center: Point2,
    pub radius: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArcFeature {
    pub style: CommonStyle,
    pub center: Point2,
    pub radius: f64,
    pub direction_flag: i64,
    pub start_angle_deg: f64,
    pub end_angle_deg: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextFeature {
    pub style: CommonStyle,
    pub text: String,
    pub anchor: Point2,
    pub height: f64,
    pub width: f64,
    pub spacing: f64,
    pub angle_deg: f64,
    pub slant_deg: f64,
    pub base_point: i64,
    pub direction: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PointMarkerFeature {
    pub style: CommonStyle,
    pub position: Point2,
    pub marker_code: i64,
    pub rotation_angle_deg: f64,
    pub scale: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EllipseFeature {
    pub style: CommonStyle,
    pub center: Point2,
    pub radius_x: f64,
    pub radius_y: f64,
    pub rotation_angle_deg: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EllipseArcFeature {
    pub style: CommonStyle,
    pub center: Point2,
    pub radius_x: f64,
    pub radius_y: f64,
    pub direction_flag: i64,
    pub rotation_angle_deg: f64,
    pub start_angle_deg: f64,
    pub end_angle_deg: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SplineFeature {
    pub style: CommonStyle,
    pub open_close: i64,
    pub declared_point_count: Option<usize>,
    pub points: Vec<Point2>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClothoidFeature {
    pub style: CommonStyle,
    pub base: Point2,
    pub parameter: f64,
    pub direction_flag: i64,
    pub angle_deg: f64,
    pub start_length: f64,
    pub end_length: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SfigOrgFeature {
    pub name: String,
    pub kind_flag: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SfigLocateFeature {
    pub style: CommonStyle,
    pub name: String,
    pub position: Point2,
    pub angle_deg: f64,
    pub ratio_x: f64,
    pub ratio_y: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternallyDefinedSymbolFeature {
    pub style: CommonStyle,
    pub color_flag: i64,
    pub name: String,
    pub position: Point2,
    pub rotation_angle_deg: f64,
    pub scale: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinearDimFeature {
    pub style: CommonStyle,
    pub raw_parameters: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CurveDimFeature {
    pub style: CommonStyle,
    pub raw_parameters: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AngularDimFeature {
    pub style: CommonStyle,
    pub raw_parameters: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RadiusDimFeature {
    pub style: CommonStyle,
    pub raw_parameters: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiameterDimFeature {
    pub style: CommonStyle,
    pub raw_parameters: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LabelFeature {
    pub style: CommonStyle,
    pub declared_vertex_count: Option<usize>,
    pub vertices: Vec<Point2>,
    pub arrow_code: Option<i64>,
    pub arrow_scale: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BalloonFeature {
    pub style: CommonStyle,
    pub declared_vertex_count: Option<usize>,
    pub vertices: Vec<Point2>,
    pub center: Point2,
    pub radius: f64,
    pub arrow_code: Option<i64>,
    pub arrow_scale: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternallyDefinedHatchFeature {
    pub style: CommonStyle,
    pub name: String,
    pub out_id: i64,
    pub hole_count: Option<usize>,
    pub in_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FillAreaStyleColourFeature {
    pub style: CommonStyle,
    pub out_id: i64,
    pub hole_count: Option<usize>,
    pub in_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FillAreaStyleHatchingFeature {
    pub style: CommonStyle,
    pub hatch_number: i64,
    pub hatch_patterns: Vec<Value>,
    pub out_id: i64,
    pub hole_count: Option<usize>,
    pub in_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FillAreaStyleTilesFeature {
    pub style: CommonStyle,
    pub name: String,
    pub hatch_color: i64,
    pub hatch_pattern_position: Point2,
    pub out_id: i64,
    pub hatch_pattern_vector1: f64,
    pub hatch_pattern_vector1_angle_deg: f64,
    pub hatch_pattern_vector2: f64,
    pub hatch_pattern_vector2_angle_deg: f64,
    pub hatch_pattern_scale_x: f64,
    pub hatch_pattern_scale_y: f64,
    pub hatch_pattern_angle_deg: f64,
    pub hole_count: Option<usize>,
    pub in_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompositeCurveFeature {
    pub style: CommonStyle,
    pub visibility_flag: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DrawingSheetFeature {
    pub name: String,
    pub sheet_type: i64,
    pub orientation: i64,
    pub free_x_mm: i64,
    pub free_y_mm: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayerFeature {
    pub name: String,
    pub visibility_flag: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DrawingAttributeFeature {
    pub project_name: String,
    pub construction_name: String,
    pub contract_type: String,
    pub drawing_name: String,
    pub drawing_number: String,
    pub drawing_type: String,
    pub drawing_scale: String,
    pub drawing_year: i64,
    pub drawing_month: i64,
    pub drawing_day: i64,
    pub contractor_name: String,
    pub owner_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreDefinedFontFeature {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UserDefinedFontFeature {
    pub name: String,
    pub segment_count: i64,
    pub pitch: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreDefinedColourFeature {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UserDefinedColourFeature {
    pub red: i64,
    pub green: i64,
    pub blue: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WidthFeature {
    pub width: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextFontFeature {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypedFeature {
    DrawingAttribute(DrawingAttributeFeature),
    DrawingSheet(DrawingSheetFeature),
    Layer(LayerFeature),
    PreDefinedFont(PreDefinedFontFeature),
    UserDefinedFont(UserDefinedFontFeature),
    PreDefinedColour(PreDefinedColourFeature),
    UserDefinedColour(UserDefinedColourFeature),
    Width(WidthFeature),
    TextFont(TextFontFeature),
    SfigOrg(SfigOrgFeature),
    SfigLocate(SfigLocateFeature),
    ExternallyDefinedSymbol(ExternallyDefinedSymbolFeature),
    LinearDim(LinearDimFeature),
    CurveDim(CurveDimFeature),
    AngularDim(AngularDimFeature),
    RadiusDim(RadiusDimFeature),
    DiameterDim(DiameterDimFeature),
    Label(LabelFeature),
    Balloon(BalloonFeature),
    ExternallyDefinedHatch(ExternallyDefinedHatchFeature),
    FillAreaStyleColour(FillAreaStyleColourFeature),
    FillAreaStyleHatching(FillAreaStyleHatchingFeature),
    FillAreaStyleTiles(FillAreaStyleTilesFeature),
    PointMarker(PointMarkerFeature),
    Line(LineFeature),
    Polyline(PolylineFeature),
    Circle(CircleFeature),
    Arc(ArcFeature),
    Ellipse(EllipseFeature),
    EllipseArc(EllipseArcFeature),
    Text(TextFeature),
    Spline(SplineFeature),
    Clothoid(ClothoidFeature),
    CompositeCurve(CompositeCurveFeature),
}

impl TypedFeature {
    fn style(&self) -> Option<&CommonStyle> {
        match self {
            TypedFeature::SfigLocate(feature) => Some(&feature.style),
            TypedFeature::ExternallyDefinedSymbol(feature) => Some(&feature.style),
            TypedFeature::LinearDim(feature) => Some(&feature.style),
            TypedFeature::CurveDim(feature) => Some(&feature.style),
            TypedFeature::AngularDim(feature) => Some(&feature.style),
            TypedFeature::RadiusDim(feature) => Some(&feature.style),
            TypedFeature::DiameterDim(feature) => Some(&feature.style),
            TypedFeature::Label(feature) => Some(&feature.style),
            TypedFeature::Balloon(feature) => Some(&feature.style),
            TypedFeature::ExternallyDefinedHatch(feature) => Some(&feature.style),
            TypedFeature::FillAreaStyleColour(feature) => Some(&feature.style),
            TypedFeature::FillAreaStyleHatching(feature) => Some(&feature.style),
            TypedFeature::FillAreaStyleTiles(feature) => Some(&feature.style),
            TypedFeature::PointMarker(feature) => Some(&feature.style),
            TypedFeature::Line(feature) => Some(&feature.style),
            TypedFeature::Polyline(feature) => Some(&feature.style),
            TypedFeature::Circle(feature) => Some(&feature.style),
            TypedFeature::Arc(feature) => Some(&feature.style),
            TypedFeature::Ellipse(feature) => Some(&feature.style),
            TypedFeature::EllipseArc(feature) => Some(&feature.style),
            TypedFeature::Text(feature) => Some(&feature.style),
            TypedFeature::Spline(feature) => Some(&feature.style),
            TypedFeature::Clothoid(feature) => Some(&feature.style),
            TypedFeature::CompositeCurve(feature) => Some(&feature.style),
            TypedFeature::DrawingAttribute(_)
            | TypedFeature::DrawingSheet(_)
            | TypedFeature::Layer(_)
            | TypedFeature::PreDefinedFont(_)
            | TypedFeature::UserDefinedFont(_)
            | TypedFeature::PreDefinedColour(_)
            | TypedFeature::UserDefinedColour(_)
            | TypedFeature::Width(_)
            | TypedFeature::TextFont(_)
            | TypedFeature::SfigOrg(_) => None,
        }
    }

    fn requires_pre_sheet_order(&self) -> bool {
        matches!(
            self,
            TypedFeature::SfigOrg(_)
                | TypedFeature::SfigLocate(_)
                | TypedFeature::ExternallyDefinedSymbol(_)
                | TypedFeature::LinearDim(_)
                | TypedFeature::CurveDim(_)
                | TypedFeature::AngularDim(_)
                | TypedFeature::RadiusDim(_)
                | TypedFeature::DiameterDim(_)
                | TypedFeature::Label(_)
                | TypedFeature::Balloon(_)
                | TypedFeature::ExternallyDefinedHatch(_)
                | TypedFeature::FillAreaStyleColour(_)
                | TypedFeature::FillAreaStyleHatching(_)
                | TypedFeature::FillAreaStyleTiles(_)
                | TypedFeature::PointMarker(_)
                | TypedFeature::Line(_)
                | TypedFeature::Polyline(_)
                | TypedFeature::Circle(_)
                | TypedFeature::Arc(_)
                | TypedFeature::Ellipse(_)
                | TypedFeature::EllipseArc(_)
                | TypedFeature::Text(_)
                | TypedFeature::Spline(_)
                | TypedFeature::Clothoid(_)
                | TypedFeature::CompositeCurve(_)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseWarning {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseOutput {
    pub document: ParsedDocument,
    pub warnings: Vec<ParseWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub snippet: String,
}

impl ParseError {
    fn new(
        message: impl Into<String>,
        line: usize,
        column: usize,
        snippet: impl Into<String>,
    ) -> Self {
        Self {
            message: message.into(),
            line,
            column,
            snippet: snippet.into(),
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.snippet.is_empty() {
            write!(f, "{} at {}:{}", self.message, self.line, self.column)
        } else {
            write!(
                f,
                "{} at {}:{}\n{}",
                self.message, self.line, self.column, self.snippet
            )
        }
    }
}

impl std::error::Error for ParseError {}

#[derive(Clone, Copy)]
struct Snapshot {
    index: usize,
    line: usize,
    column: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SfcBlockTag {
    V2,
    V30,
    V31,
}

struct Parser<'a> {
    input: &'a str,
    chars: Vec<char>,
    index: usize,
    line: usize,
    column: usize,
    format: FileFormat,
    strict: bool,
    warnings: Vec<ParseWarning>,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str, format: FileFormat, strict: bool) -> Self {
        Self {
            input,
            chars: input.chars().collect(),
            index: 0,
            line: 1,
            column: 1,
            format,
            strict,
            warnings: Vec::new(),
        }
    }

    fn parse(mut self) -> Result<ParseOutput, ParseError> {
        self.skip_ws();
        self.consume_literal_ci("ISO-10303-21")?;
        self.skip_ws();
        self.consume_char(';')?;
        let header = self.parse_header_section()?;
        let entities = match self.format {
            FileFormat::P21 => self.parse_data_section_p21()?,
            FileFormat::Sfc => self.parse_data_section_sfc()?,
        };
        self.skip_ws();
        self.consume_literal_ci("END-ISO-10303-21")?;
        self.skip_ws();
        self.consume_char(';')?;
        self.skip_ws();
        if !self.is_eof() {
            self.issue_or_error(
                "trailing-data",
                "Unexpected trailing data after END-ISO-10303-21;".to_string(),
            )?;
        }

        let mut document = ParsedDocument {
            format: self.format,
            header,
            entities,
            typed_features: Vec::new(),
        };
        if self.format == FileFormat::Sfc {
            document.typed_features = self.extract_typed_features(&document.entities)?;
        }
        self.validate_document(&document)?;

        Ok(ParseOutput {
            document,
            warnings: self.warnings,
        })
    }

    fn parse_header_section(&mut self) -> Result<HeaderSection, ParseError> {
        self.skip_ws();
        self.consume_keyword_ci("HEADER")?;
        self.skip_ws();
        self.consume_char(';')?;

        let mut entities = Vec::new();
        loop {
            self.skip_ws();
            if self.try_consume_keyword_ci("ENDSEC") {
                self.skip_ws();
                self.consume_char(';')?;
                break;
            }
            let record = self.parse_simple_record()?;
            self.skip_ws();
            self.consume_char(';')?;
            entities.push(record);
        }

        Ok(HeaderSection { entities })
    }

    fn parse_data_section_p21(&mut self) -> Result<Vec<EntityInstance>, ParseError> {
        self.skip_ws();
        self.consume_keyword_ci("DATA")?;
        self.skip_ws();
        self.consume_char(';')?;

        let mut entities = Vec::new();
        loop {
            self.skip_ws();
            if self.try_consume_keyword_ci("ENDSEC") {
                self.skip_ws();
                self.consume_char(';')?;
                break;
            }
            entities.push(self.parse_entity_instance(true)?);
        }
        Ok(entities)
    }

    fn parse_data_section_sfc(&mut self) -> Result<Vec<EntityInstance>, ParseError> {
        self.skip_ws();
        self.consume_keyword_ci("DATA")?;
        self.skip_ws();
        self.consume_char(';')?;

        let mut entities = Vec::new();
        loop {
            self.skip_ws();
            if self.try_consume_keyword_ci("ENDSEC") {
                self.skip_ws();
                self.consume_char(';')?;
                break;
            }
            let tag = match self.parse_sfc_prefix() {
                Ok(tag) => tag,
                Err(error) if !self.strict => {
                    self.push_warning(
                        "sfc-block-prefix",
                        format!("Invalid SFC block prefix skipped: {}", error.message),
                    );
                    if self.seek_next_sfc_prefix_or_endsec() {
                        continue;
                    }
                    break;
                }
                Err(error) => return Err(error),
            };
            self.skip_ws();
            match self.parse_entity_instance(false) {
                Ok(entity) => entities.push(entity),
                Err(error) if !self.strict => {
                    self.push_warning(
                        "sfc-feature-block-skipped",
                        format!("Failed to parse feature block; skipped: {}", error.message),
                    );
                    if !self.recover_to_sfc_suffix(tag) {
                        break;
                    }
                    continue;
                }
                Err(error) => return Err(error),
            }
            self.skip_ws();
            match self.parse_sfc_suffix(tag) {
                Ok(()) => {}
                Err(error) if !self.strict => {
                    self.push_warning(
                        "sfc-block-suffix",
                        format!("Invalid SFC block suffix; skipped: {}", error.message),
                    );
                    if !self.recover_to_sfc_suffix(tag) {
                        break;
                    }
                }
                Err(error) => return Err(error),
            }
        }
        Ok(entities)
    }

    fn parse_sfc_prefix(&mut self) -> Result<SfcBlockTag, ParseError> {
        self.skip_ws();
        self.consume_char('/')?;
        self.consume_char('*')?;
        if self.try_consume_literal_ci("SXF3.1") {
            Ok(SfcBlockTag::V31)
        } else if self.try_consume_literal_ci("SXF3") {
            Ok(SfcBlockTag::V30)
        } else if self.try_consume_literal_ci("SXF") {
            Ok(SfcBlockTag::V2)
        } else {
            Err(self.error_here("Invalid SFC block prefix. Expected /*SXF, /*SXF3 or /*SXF3.1"))
        }
    }

    fn parse_sfc_suffix(&mut self, tag: SfcBlockTag) -> Result<(), ParseError> {
        match tag {
            SfcBlockTag::V2 => self.consume_literal_ci("SXF*/"),
            SfcBlockTag::V30 => self.consume_literal_ci("SXF3*/"),
            SfcBlockTag::V31 => self.consume_literal_ci("SXF3.1*/"),
        }
    }

    fn recover_to_sfc_suffix(&mut self, tag: SfcBlockTag) -> bool {
        let suffix = match tag {
            SfcBlockTag::V2 => "SXF*/",
            SfcBlockTag::V30 => "SXF3*/",
            SfcBlockTag::V31 => "SXF3.1*/",
        };
        self.seek_literal_ci(suffix)
    }

    fn seek_next_sfc_prefix_or_endsec(&mut self) -> bool {
        loop {
            if self.is_eof() {
                return false;
            }
            let snapshot = self.snapshot();
            if self.try_consume_keyword_ci("ENDSEC") {
                self.restore(snapshot);
                return true;
            }
            self.restore(snapshot);
            if self.try_consume_literal_ci("/*SXF")
                || self.try_consume_literal_ci("/*SXF3")
                || self.try_consume_literal_ci("/*SXF3.1")
            {
                self.restore(snapshot);
                return true;
            }
            self.advance_char();
        }
    }

    fn parse_entity_instance(
        &mut self,
        semicolon_required: bool,
    ) -> Result<EntityInstance, ParseError> {
        self.skip_ws();
        let id = self.parse_reference_id()?;
        self.skip_ws();
        self.consume_char('=')?;
        self.skip_ws();

        let body = if self.peek_char() == Some('(') {
            EntityBody::Complex(self.parse_complex_record()?)
        } else {
            EntityBody::Simple(self.parse_simple_record()?)
        };

        self.skip_ws();
        if semicolon_required {
            self.consume_char(';')?;
        } else {
            let _ = self.try_consume_char(';');
        }

        Ok(EntityInstance { id, body })
    }

    fn parse_complex_record(&mut self) -> Result<Vec<Record>, ParseError> {
        self.consume_char('(')?;
        self.skip_ws();
        let mut records = Vec::new();
        while self.peek_char() != Some(')') {
            records.push(self.parse_simple_record()?);
            self.skip_ws();
            let _ = self.try_consume_char(',');
            self.skip_ws();
        }
        self.consume_char(')')?;

        if records.is_empty() {
            Err(self.error_here("Complex record must include at least one simple record"))
        } else {
            Ok(records)
        }
    }

    fn parse_simple_record(&mut self) -> Result<Record, ParseError> {
        self.skip_ws();
        let keyword = self.parse_keyword()?;
        self.skip_ws();
        self.consume_char('(')?;
        let parameters = self.parse_parameter_list_until(')')?;
        self.consume_char(')')?;

        Ok(Record {
            keyword,
            parameters,
        })
    }

    fn parse_parameter_list_until(&mut self, terminator: char) -> Result<Vec<Value>, ParseError> {
        self.skip_ws();
        let mut parameters = Vec::new();
        if self.peek_char() == Some(terminator) {
            return Ok(parameters);
        }

        loop {
            parameters.push(self.parse_value()?);
            self.skip_ws();
            if self.try_consume_char(',') {
                self.skip_ws();
                continue;
            }
            break;
        }
        Ok(parameters)
    }

    fn parse_value(&mut self) -> Result<Value, ParseError> {
        self.skip_ws();
        let ch = self
            .peek_char()
            .ok_or_else(|| self.error_here("Unexpected end of input while parsing value"))?;

        match ch {
            '$' => {
                self.advance_char();
                Ok(Value::Unset)
            }
            '*' => {
                self.advance_char();
                Ok(Value::Omitted)
            }
            '#' => Ok(Value::Reference(self.parse_reference_id()?)),
            '\\' if self.peek_next_char() == Some('\'') => {
                Ok(Value::String(self.parse_backslash_quoted_string()?))
            }
            '\'' => Ok(Value::String(self.parse_quoted_string()?)),
            '"' => Ok(Value::Binary(self.parse_binary_literal()?)),
            '(' => {
                self.consume_char('(')?;
                let values = self.parse_parameter_list_until(')')?;
                self.consume_char(')')?;
                Ok(Value::List(values))
            }
            '.' => Ok(Value::Enum(self.parse_enum_value()?)),
            '+' | '-' | '0'..='9' => self.parse_number_value(),
            _ if is_keyword_start(ch) => {
                let keyword = self.parse_keyword()?;
                self.skip_ws();
                if self.try_consume_char('(') {
                    let parameters = self.parse_parameter_list_until(')')?;
                    self.consume_char(')')?;
                    Ok(Value::Typed {
                        keyword,
                        parameters,
                    })
                } else {
                    Err(self.error_here("Bare keyword is not allowed in parameter values"))
                }
            }
            _ => Err(self.error_here("Unsupported value token")),
        }
    }

    fn parse_number_value(&mut self) -> Result<Value, ParseError> {
        let start = self.snapshot();
        let mut text = String::new();
        if let Some(ch @ ('+' | '-')) = self.peek_char() {
            text.push(ch);
            self.advance_char();
        }

        let mut integer_digits = 0usize;
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_digit() {
                text.push(ch);
                self.advance_char();
                integer_digits += 1;
            } else {
                break;
            }
        }

        if integer_digits == 0 {
            return Err(self.error_at(start, "Invalid numeric literal"));
        }

        let mut is_real = false;
        if self.peek_char() == Some('.') {
            is_real = true;
            text.push('.');
            self.advance_char();
            while let Some(ch) = self.peek_char() {
                if ch.is_ascii_digit() {
                    text.push(ch);
                    self.advance_char();
                } else {
                    break;
                }
            }
        }

        let mut uses_exponent = false;
        if let Some(ch @ ('E' | 'e')) = self.peek_char() {
            uses_exponent = true;
            is_real = true;
            text.push(ch);
            self.advance_char();

            if let Some(sign @ ('+' | '-')) = self.peek_char() {
                text.push(sign);
                self.advance_char();
            }

            let mut exponent_digits = 0usize;
            while let Some(ch) = self.peek_char() {
                if ch.is_ascii_digit() {
                    text.push(ch);
                    self.advance_char();
                    exponent_digits += 1;
                } else {
                    break;
                }
            }

            if exponent_digits == 0 {
                return Err(self.error_at(start, "Invalid exponent in numeric literal"));
            }
        }

        if uses_exponent && self.format == FileFormat::Sfc {
            self.issue_or_error(
                "sfc-real-format",
                "Exponent notation is discouraged for SFC real values".to_string(),
            )?;
        }

        if is_real {
            text.parse::<f64>()
                .map(Value::Real)
                .map_err(|_| self.error_at(start, "Failed to parse floating-point value"))
        } else {
            text.parse::<i64>()
                .map(Value::Integer)
                .map_err(|_| self.error_at(start, "Failed to parse integer value"))
        }
    }

    fn parse_reference_id(&mut self) -> Result<i64, ParseError> {
        let start = self.snapshot();
        self.consume_char('#')?;
        let mut digits = String::new();
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_digit() {
                digits.push(ch);
                self.advance_char();
            } else {
                break;
            }
        }
        if digits.is_empty() {
            return Err(self.error_at(start, "Reference must contain at least one digit"));
        }
        digits
            .parse::<i64>()
            .map_err(|_| self.error_at(start, "Reference id is out of range"))
    }

    fn parse_quoted_string(&mut self) -> Result<String, ParseError> {
        self.consume_char('\'')?;
        let mut out = String::new();
        loop {
            match self.peek_char() {
                Some('\'') => {
                    self.advance_char();
                    if self.peek_char() == Some('\'') {
                        self.advance_char();
                        out.push('\'');
                    } else {
                        break;
                    }
                }
                Some('\\') => {
                    self.advance_char();
                    if self.peek_char() == Some('\\') {
                        self.advance_char();
                        out.push('\\');
                    } else {
                        out.push('\\');
                    }
                }
                Some(ch) => {
                    self.advance_char();
                    out.push(ch);
                }
                None => {
                    return Err(self.error_here("Unterminated quoted string literal"));
                }
            }
        }
        Ok(out)
    }

    fn parse_backslash_quoted_string(&mut self) -> Result<String, ParseError> {
        self.consume_char('\\')?;
        self.consume_char('\'')?;
        let mut out = String::new();
        loop {
            match self.peek_char() {
                Some('\\') if self.peek_next_char() == Some('\'') => {
                    self.advance_char();
                    self.advance_char();
                    break;
                }
                Some('\\') => {
                    self.advance_char();
                    if self.peek_char() == Some('\\') {
                        self.advance_char();
                        out.push('\\');
                    } else {
                        out.push('\\');
                    }
                }
                Some('\'') => {
                    if matches!(self.peek_following_non_ws_char(1), Some(',') | Some(')')) {
                        self.advance_char();
                        break;
                    }
                    self.advance_char();
                    out.push('\'');
                }
                Some(ch) => {
                    self.advance_char();
                    out.push(ch);
                }
                None => {
                    return Err(self.error_here("Unterminated quoted string literal"));
                }
            }
        }
        Ok(out)
    }

    fn parse_binary_literal(&mut self) -> Result<String, ParseError> {
        self.consume_char('"')?;
        let mut out = String::new();
        loop {
            match self.peek_char() {
                Some('"') => {
                    self.advance_char();
                    break;
                }
                Some(ch) => {
                    self.advance_char();
                    out.push(ch);
                }
                None => return Err(self.error_here("Unterminated binary literal")),
            }
        }
        Ok(out)
    }

    fn parse_enum_value(&mut self) -> Result<String, ParseError> {
        self.consume_char('.')?;
        let start = self.snapshot();
        let mut value = String::new();
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                value.push(ch);
                self.advance_char();
            } else {
                break;
            }
        }
        if value.is_empty() {
            return Err(self.error_at(start, "Enumeration value is empty"));
        }
        self.consume_char('.')?;
        Ok(value)
    }

    fn validate_document(&mut self, document: &ParsedDocument) -> Result<(), ParseError> {
        self.validate_required_header(document)?;
        self.validate_file_schema(document)?;
        self.validate_duplicate_ids(document)?;
        self.validate_references(document)?;
        if document.format == FileFormat::Sfc {
            self.validate_sfc_model_rules(document)?;
        }
        Ok(())
    }

    fn validate_required_header(&mut self, document: &ParsedDocument) -> Result<(), ParseError> {
        for required in ["FILE_DESCRIPTION", "FILE_NAME", "FILE_SCHEMA"] {
            if document.header.find_keyword(required).is_none() {
                self.issue_or_error(
                    "missing-header-entity",
                    format!("Required header entity {required} is missing"),
                )?;
            }
        }
        Ok(())
    }

    fn validate_file_schema(&mut self, document: &ParsedDocument) -> Result<(), ParseError> {
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

    fn validate_duplicate_ids(&mut self, document: &ParsedDocument) -> Result<(), ParseError> {
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

    fn validate_references(&mut self, document: &ParsedDocument) -> Result<(), ParseError> {
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

    fn validate_sfc_model_rules(&mut self, document: &ParsedDocument) -> Result<(), ParseError> {
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
        let mut next_line_type_code = 1_i64;
        let mut next_color_code = 1_i64;
        let mut next_line_width_code = 1_i64;
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
                TypedFeature::PreDefinedFont(_) | TypedFeature::UserDefinedFont(_) => {
                    let idx = *entity_index_map.get(&typed.id).unwrap_or(&usize::MAX);
                    line_type_code_to_index.insert(next_line_type_code, idx);
                    next_line_type_code += 1;
                }
                TypedFeature::PreDefinedColour(_) | TypedFeature::UserDefinedColour(_) => {
                    let idx = *entity_index_map.get(&typed.id).unwrap_or(&usize::MAX);
                    color_code_to_index.insert(next_color_code, idx);
                    next_color_code += 1;
                }
                TypedFeature::Width(_) => {
                    let idx = *entity_index_map.get(&typed.id).unwrap_or(&usize::MAX);
                    line_width_code_to_index.insert(next_line_width_code, idx);
                    next_line_width_code += 1;
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

            let mut check_code = |code: Option<i64>,
                                  map: &std::collections::HashMap<i64, usize>,
                                  common_max_code: Option<i64>,
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
                if common_max_code.is_some_and(|max| code <= max) || map.contains_key(&code) {
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
                "layer code",
                "sfc-layer-reference",
            )?;
            check_code(
                style.line_type_code,
                &line_type_code_to_index,
                Some(COMMON_PREDEFINED_LINE_TYPE_MAX_CODE),
                "line type code",
                "sfc-line-type-reference",
            )?;
            check_code(
                style.color_code,
                &color_code_to_index,
                Some(COMMON_PREDEFINED_COLOR_MAX_CODE),
                "color code",
                "sfc-color-reference",
            )?;
            check_code(
                style.line_width_code,
                &line_width_code_to_index,
                Some(COMMON_PREDEFINED_LINE_WIDTH_MAX_CODE),
                "line width code",
                "sfc-line-width-reference",
            )?;
            check_code(
                style.font_code,
                &text_font_code_to_index,
                None,
                "text font code",
                "sfc-text-font-reference",
            )?;
        }

        Ok(())
    }

    fn extract_typed_features(
        &mut self,
        entities: &[EntityInstance],
    ) -> Result<Vec<TypedFeatureInstance>, ParseError> {
        let mut typed = Vec::new();
        for entity in entities {
            let EntityBody::Simple(record) = &entity.body else {
                continue;
            };

            let Some(feature) = self.parse_typed_feature(record) else {
                if self.format == FileFormat::Sfc
                    && record.keyword.to_ascii_lowercase().ends_with("_feature")
                {
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
                Ok(feature) => typed.push(TypedFeatureInstance {
                    id: entity.id,
                    keyword: record.keyword.clone(),
                    feature,
                }),
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

    fn parse_typed_feature(&self, record: &Record) -> Option<Result<TypedFeature, String>> {
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
            "externally_defined_symbol_feature" => Some(
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
            "fill_area_style_tiles_feature" => Some(
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

    fn issue_or_error(&mut self, code: &str, message: String) -> Result<(), ParseError> {
        if self.strict {
            return Err(ParseError::new(
                message,
                self.line,
                self.column,
                self.current_line_snippet(),
            ));
        }
        self.warnings.push(ParseWarning {
            code: code.to_string(),
            message,
        });
        Ok(())
    }

    fn push_warning(&mut self, code: &str, message: String) {
        self.warnings.push(ParseWarning {
            code: code.to_string(),
            message,
        });
    }

    fn consume_keyword_ci(&mut self, expected: &str) -> Result<(), ParseError> {
        if self.try_consume_keyword_ci(expected) {
            Ok(())
        } else {
            Err(self.error_here(format!("Expected keyword {expected}")))
        }
    }

    fn try_consume_keyword_ci(&mut self, expected: &str) -> bool {
        let snapshot = self.snapshot();
        self.skip_ws();
        let mut found = String::new();
        while let Some(ch) = self.peek_char() {
            if is_keyword_char(ch) {
                found.push(ch);
                self.advance_char();
            } else {
                break;
            }
        }
        if !found.is_empty() && found.eq_ignore_ascii_case(expected) {
            true
        } else {
            self.restore(snapshot);
            false
        }
    }

    fn consume_literal_ci(&mut self, expected: &str) -> Result<(), ParseError> {
        if self.try_consume_literal_ci(expected) {
            Ok(())
        } else {
            Err(self.error_here(format!("Expected literal {expected}")))
        }
    }

    fn try_consume_literal_ci(&mut self, expected: &str) -> bool {
        let snapshot = self.snapshot();
        for expected_char in expected.chars() {
            let Some(actual) = self.peek_char() else {
                self.restore(snapshot);
                return false;
            };
            if !actual.eq_ignore_ascii_case(&expected_char) {
                self.restore(snapshot);
                return false;
            }
            self.advance_char();
        }
        true
    }

    fn seek_literal_ci(&mut self, expected: &str) -> bool {
        while !self.is_eof() {
            if self.try_consume_literal_ci(expected) {
                return true;
            }
            self.advance_char();
        }
        false
    }

    fn parse_keyword(&mut self) -> Result<String, ParseError> {
        self.skip_ws();
        let start = self.snapshot();
        let mut keyword = String::new();
        while let Some(ch) = self.peek_char() {
            if is_keyword_char(ch) {
                keyword.push(ch);
                self.advance_char();
            } else {
                break;
            }
        }
        if keyword.is_empty() {
            Err(self.error_at(start, "Expected keyword"))
        } else {
            Ok(keyword)
        }
    }

    fn consume_char(&mut self, expected: char) -> Result<(), ParseError> {
        match self.peek_char() {
            Some(ch) if ch == expected => {
                self.advance_char();
                Ok(())
            }
            _ => Err(self.error_here(format!("Expected '{expected}'"))),
        }
    }

    fn try_consume_char(&mut self, expected: char) -> bool {
        if self.peek_char() == Some(expected) {
            self.advance_char();
            true
        } else {
            false
        }
    }

    fn skip_ws(&mut self) {
        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace() {
                self.advance_char();
            } else {
                break;
            }
        }
    }

    fn is_eof(&self) -> bool {
        self.index >= self.chars.len()
    }

    fn peek_char(&self) -> Option<char> {
        self.chars.get(self.index).copied()
    }

    fn peek_next_char(&self) -> Option<char> {
        self.chars.get(self.index + 1).copied()
    }

    fn peek_following_non_ws_char(&self, offset: usize) -> Option<char> {
        let mut idx = self.index + offset;
        while let Some(ch) = self.chars.get(idx).copied() {
            if ch.is_whitespace() {
                idx += 1;
                continue;
            }
            return Some(ch);
        }
        None
    }

    fn advance_char(&mut self) {
        if let Some(ch) = self.peek_char() {
            self.index += 1;
            if ch == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            index: self.index,
            line: self.line,
            column: self.column,
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.index = snapshot.index;
        self.line = snapshot.line;
        self.column = snapshot.column;
    }

    fn error_here(&self, message: impl Into<String>) -> ParseError {
        ParseError::new(message, self.line, self.column, self.current_line_snippet())
    }

    fn error_at(&self, snapshot: Snapshot, message: impl Into<String>) -> ParseError {
        let snippet = self
            .input
            .lines()
            .nth(snapshot.line.saturating_sub(1))
            .unwrap_or("")
            .to_string();
        ParseError::new(message, snapshot.line, snapshot.column, snippet)
    }

    fn current_line_snippet(&self) -> String {
        self.input
            .lines()
            .nth(self.line.saturating_sub(1))
            .unwrap_or("")
            .to_string()
    }
}

impl EntityInstance {
    fn collect_references(&self) -> Vec<i64> {
        let mut refs = Vec::new();
        match &self.body {
            EntityBody::Simple(record) => collect_refs_from_record(record, &mut refs),
            EntityBody::Complex(records) => {
                for record in records {
                    collect_refs_from_record(record, &mut refs);
                }
            }
        }
        refs
    }
}

fn collect_refs_from_record(record: &Record, refs: &mut Vec<i64>) {
    for value in &record.parameters {
        collect_refs_from_value(value, refs);
    }
}

fn collect_refs_from_value(value: &Value, refs: &mut Vec<i64>) {
    match value {
        Value::Reference(id) => refs.push(*id),
        Value::List(values) => {
            for item in values {
                collect_refs_from_value(item, refs);
            }
        }
        Value::Typed { parameters, .. } => {
            for item in parameters {
                collect_refs_from_value(item, refs);
            }
        }
        Value::Unset
        | Value::Omitted
        | Value::Integer(_)
        | Value::Real(_)
        | Value::String(_)
        | Value::Enum(_)
        | Value::Binary(_) => {}
    }
}

fn parse_f64_list_from_string(text: &str) -> Option<Vec<f64>> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Some(Vec::new());
    }

    let inner = if let Some(stripped) = trimmed.strip_prefix('(') {
        stripped.strip_suffix(')')?
    } else {
        trimmed
    };

    if inner.trim().is_empty() {
        return Some(Vec::new());
    }

    let mut values = Vec::new();
    for part in inner.split(',') {
        let number = part.trim().parse::<f64>().ok()?;
        values.push(number);
    }
    Some(values)
}

fn parse_i64_list_from_string(text: &str) -> Option<Vec<i64>> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Some(Vec::new());
    }

    let inner = if let Some(stripped) = trimmed.strip_prefix('(') {
        stripped.strip_suffix(')')?
    } else {
        trimmed
    };

    if inner.trim().is_empty() {
        return Some(Vec::new());
    }

    let mut values = Vec::new();
    for part in inner.split(',') {
        let number = part.trim().parse::<i64>().ok()?;
        values.push(number);
    }
    Some(values)
}

fn parse_line_feature(params: &[Value]) -> Result<LineFeature, String> {
    if params.len() < 8 {
        return Err(format!(
            "line_feature requires 8 parameters, got {}",
            params.len()
        ));
    }
    let style = parse_line_style(params)?;
    let start_x = parse_required_f64(&params[4], "start_x")?;
    let start_y = parse_required_f64(&params[5], "start_y")?;
    let end_x = parse_required_f64(&params[6], "end_x")?;
    let end_y = parse_required_f64(&params[7], "end_y")?;
    Ok(LineFeature {
        style,
        start: Point2 {
            x: start_x,
            y: start_y,
        },
        end: Point2 { x: end_x, y: end_y },
    })
}

fn parse_drawing_attribute_feature(params: &[Value]) -> Result<DrawingAttributeFeature, String> {
    if params.len() < 12 {
        return Err(format!(
            "drawing_attribute_feature requires 12 parameters, got {}",
            params.len()
        ));
    }

    Ok(DrawingAttributeFeature {
        project_name: parse_required_string(&params[0], "project_name")?,
        construction_name: parse_required_string(&params[1], "construction_name")?,
        contract_type: parse_required_string(&params[2], "contract_type")?,
        drawing_name: parse_required_string(&params[3], "drawing_name")?,
        drawing_number: parse_required_string(&params[4], "drawing_number")?,
        drawing_type: parse_required_string(&params[5], "drawing_type")?,
        drawing_scale: parse_required_string(&params[6], "drawing_scale")?,
        drawing_year: parse_required_i64(&params[7], "drawing_year")?,
        drawing_month: parse_required_i64(&params[8], "drawing_month")?,
        drawing_day: parse_required_i64(&params[9], "drawing_day")?,
        contractor_name: parse_required_string(&params[10], "contractor_name")?,
        owner_name: parse_required_string(&params[11], "owner_name")?,
    })
}

fn parse_drawing_sheet_feature(params: &[Value]) -> Result<DrawingSheetFeature, String> {
    if params.len() < 5 {
        return Err(format!(
            "drawing_sheet_feature requires 5 parameters, got {}",
            params.len()
        ));
    }

    Ok(DrawingSheetFeature {
        name: parse_required_string(&params[0], "name")?,
        sheet_type: parse_required_i64(&params[1], "type")?,
        orientation: parse_required_i64(&params[2], "orientation")?,
        free_x_mm: parse_required_i64(&params[3], "x")?,
        free_y_mm: parse_required_i64(&params[4], "y")?,
    })
}

fn parse_layer_feature(params: &[Value]) -> Result<LayerFeature, String> {
    if params.len() < 2 {
        return Err(format!(
            "layer_feature requires 2 parameters, got {}",
            params.len()
        ));
    }

    Ok(LayerFeature {
        name: parse_required_string(&params[0], "name")?,
        visibility_flag: parse_required_i64(&params[1], "lflag")?,
    })
}

fn parse_pre_defined_font_feature(params: &[Value]) -> Result<PreDefinedFontFeature, String> {
    if params.is_empty() {
        return Err("pre_defined_font_feature requires 1 parameter".to_string());
    }
    Ok(PreDefinedFontFeature {
        name: parse_required_string(&params[0], "name")?,
    })
}

fn parse_user_defined_font_feature(params: &[Value]) -> Result<UserDefinedFontFeature, String> {
    if params.len() < 3 {
        return Err(format!(
            "user_defined_font_feature requires 3 parameters, got {}",
            params.len()
        ));
    }
    Ok(UserDefinedFontFeature {
        name: parse_required_string(&params[0], "name")?,
        segment_count: parse_required_i64(&params[1], "segment")?,
        pitch: parse_required_f64_list(&params[2], "pitch")?,
    })
}

fn parse_pre_defined_colour_feature(params: &[Value]) -> Result<PreDefinedColourFeature, String> {
    if params.is_empty() {
        return Err("pre_defined_colour_feature requires 1 parameter".to_string());
    }
    Ok(PreDefinedColourFeature {
        name: parse_required_string(&params[0], "name")?,
    })
}

fn parse_user_defined_colour_feature(params: &[Value]) -> Result<UserDefinedColourFeature, String> {
    if params.len() < 3 {
        return Err(format!(
            "user_defined_colour_feature requires 3 parameters, got {}",
            params.len()
        ));
    }
    Ok(UserDefinedColourFeature {
        red: parse_required_i64(&params[0], "red")?,
        green: parse_required_i64(&params[1], "green")?,
        blue: parse_required_i64(&params[2], "blue")?,
    })
}

fn parse_width_feature(params: &[Value]) -> Result<WidthFeature, String> {
    if params.is_empty() {
        return Err("width_feature requires 1 parameter".to_string());
    }
    Ok(WidthFeature {
        width: parse_required_f64(&params[0], "width")?,
    })
}

fn parse_text_font_feature(params: &[Value]) -> Result<TextFontFeature, String> {
    if params.is_empty() {
        return Err("text_font_feature requires 1 parameter".to_string());
    }
    Ok(TextFontFeature {
        name: parse_required_string(&params[0], "name")?,
    })
}

fn parse_point_marker_feature(params: &[Value]) -> Result<PointMarkerFeature, String> {
    if params.len() < 7 {
        return Err(format!(
            "point_marker_feature requires 7 parameters, got {}",
            params.len()
        ));
    }
    let style = parse_layer_color_style(params)?;
    Ok(PointMarkerFeature {
        style,
        position: Point2 {
            x: parse_required_f64(&params[2], "start_x")?,
            y: parse_required_f64(&params[3], "start_y")?,
        },
        marker_code: parse_required_i64(&params[4], "marker_code")?,
        rotation_angle_deg: parse_required_f64(&params[5], "rotation_angle")?,
        scale: parse_required_f64(&params[6], "scale")?,
    })
}

fn parse_polyline_feature(params: &[Value]) -> Result<PolylineFeature, String> {
    if params.len() < 7 {
        return Err(format!(
            "polyline_feature requires 7 parameters, got {}",
            params.len()
        ));
    }
    let style = parse_line_style(params)?;
    let declared = parse_required_i64(&params[4], "number")?;
    let xs = parse_required_f64_list(&params[5], "x coordinates")?;
    let ys = parse_required_f64_list(&params[6], "y coordinates")?;
    let point_count = xs.len().min(ys.len());
    if point_count == 0 {
        return Err("polyline_feature has no vertices".to_string());
    }

    let mut points = Vec::with_capacity(point_count);
    for i in 0..point_count {
        points.push(Point2 { x: xs[i], y: ys[i] });
    }

    Ok(PolylineFeature {
        style,
        declared_point_count: usize::try_from(declared).ok(),
        points,
    })
}

fn parse_circle_feature(params: &[Value]) -> Result<CircleFeature, String> {
    if params.len() < 7 {
        return Err(format!(
            "circle_feature requires 7 parameters, got {}",
            params.len()
        ));
    }
    let style = parse_line_style(params)?;
    Ok(CircleFeature {
        style,
        center: Point2 {
            x: parse_required_f64(&params[4], "center_x")?,
            y: parse_required_f64(&params[5], "center_y")?,
        },
        radius: parse_required_f64(&params[6], "radius")?,
    })
}

fn parse_arc_feature(params: &[Value]) -> Result<ArcFeature, String> {
    if params.len() < 10 {
        return Err(format!(
            "arc_feature requires 10 parameters, got {}",
            params.len()
        ));
    }
    let style = parse_line_style(params)?;
    Ok(ArcFeature {
        style,
        center: Point2 {
            x: parse_required_f64(&params[4], "center_x")?,
            y: parse_required_f64(&params[5], "center_y")?,
        },
        radius: parse_required_f64(&params[6], "radius")?,
        direction_flag: parse_required_i64(&params[7], "direction")?,
        start_angle_deg: parse_required_f64(&params[8], "start_angle")?,
        end_angle_deg: parse_required_f64(&params[9], "end_angle")?,
    })
}

fn parse_ellipse_feature(params: &[Value]) -> Result<EllipseFeature, String> {
    if params.len() < 9 {
        return Err(format!(
            "ellipse_feature requires 9 parameters, got {}",
            params.len()
        ));
    }
    let style = parse_line_style(params)?;
    Ok(EllipseFeature {
        style,
        center: Point2 {
            x: parse_required_f64(&params[4], "center_x")?,
            y: parse_required_f64(&params[5], "center_y")?,
        },
        radius_x: parse_required_f64(&params[6], "radius_x")?,
        radius_y: parse_required_f64(&params[7], "radius_y")?,
        rotation_angle_deg: parse_required_f64(&params[8], "rotation_angle")?,
    })
}

fn parse_ellipse_arc_feature(params: &[Value]) -> Result<EllipseArcFeature, String> {
    if params.len() < 12 {
        return Err(format!(
            "ellipse_arc_feature requires 12 parameters, got {}",
            params.len()
        ));
    }
    let style = parse_line_style(params)?;
    Ok(EllipseArcFeature {
        style,
        center: Point2 {
            x: parse_required_f64(&params[4], "center_x")?,
            y: parse_required_f64(&params[5], "center_y")?,
        },
        radius_x: parse_required_f64(&params[6], "radius_x")?,
        radius_y: parse_required_f64(&params[7], "radius_y")?,
        direction_flag: parse_required_i64(&params[8], "direction")?,
        rotation_angle_deg: parse_required_f64(&params[9], "rotation_angle")?,
        start_angle_deg: parse_required_f64(&params[10], "start_angle")?,
        end_angle_deg: parse_required_f64(&params[11], "end_angle")?,
    })
}

fn parse_text_feature(params: &[Value]) -> Result<TextFeature, String> {
    if params.len() < 13 {
        return Err(format!(
            "text_string_feature requires 13 parameters, got {}",
            params.len()
        ));
    }
    Ok(TextFeature {
        style: CommonStyle {
            layer_code: parse_optional_i64(&params[0]),
            color_code: parse_optional_i64(&params[1]),
            font_code: parse_optional_i64(&params[2]),
            ..Default::default()
        },
        text: parse_required_string(&params[3], "str")?,
        anchor: Point2 {
            x: parse_required_f64(&params[4], "text_x")?,
            y: parse_required_f64(&params[5], "text_y")?,
        },
        height: parse_required_f64(&params[6], "height")?,
        width: parse_required_f64(&params[7], "width")?,
        spacing: parse_required_f64(&params[8], "spacing")?,
        angle_deg: parse_required_f64(&params[9], "angle")?,
        slant_deg: parse_required_f64(&params[10], "slant")?,
        base_point: parse_required_i64(&params[11], "base_point")?,
        direction: parse_required_i64(&params[12], "direction")?,
    })
}

fn parse_spline_feature(params: &[Value]) -> Result<SplineFeature, String> {
    if params.len() < 8 {
        return Err(format!(
            "spline_feature requires 8 parameters, got {}",
            params.len()
        ));
    }
    let style = parse_line_style(params)?;
    let declared = parse_required_i64(&params[5], "number")?;
    let xs = parse_required_f64_list(&params[6], "x coordinates")?;
    let ys = parse_required_f64_list(&params[7], "y coordinates")?;
    let point_count = xs.len().min(ys.len());
    if point_count == 0 {
        return Err("spline_feature has no control points".to_string());
    }

    let mut points = Vec::with_capacity(point_count);
    for i in 0..point_count {
        points.push(Point2 { x: xs[i], y: ys[i] });
    }

    Ok(SplineFeature {
        style,
        open_close: parse_required_i64(&params[4], "open_close")?,
        declared_point_count: usize::try_from(declared).ok(),
        points,
    })
}

fn parse_clothoid_feature(params: &[Value]) -> Result<ClothoidFeature, String> {
    if params.len() < 11 {
        return Err(format!(
            "clothoid_feature requires 11 parameters, got {}",
            params.len()
        ));
    }
    let style = parse_line_style(params)?;
    Ok(ClothoidFeature {
        style,
        base: Point2 {
            x: parse_required_f64(&params[4], "base_x")?,
            y: parse_required_f64(&params[5], "base_y")?,
        },
        parameter: parse_required_f64(&params[6], "parameter")?,
        direction_flag: parse_required_i64(&params[7], "direction")?,
        angle_deg: parse_required_f64(&params[8], "angle")?,
        start_length: parse_required_f64(&params[9], "start_length")?,
        end_length: parse_required_f64(&params[10], "end_length")?,
    })
}

fn parse_composite_curve_feature(params: &[Value]) -> Result<CompositeCurveFeature, String> {
    if params.len() < 4 {
        return Err(format!(
            "composite_curve_feature requires 4 parameters, got {}",
            params.len()
        ));
    }
    Ok(CompositeCurveFeature {
        style: CommonStyle {
            color_code: parse_optional_i64(&params[0]),
            line_type_code: parse_optional_i64(&params[1]),
            line_width_code: parse_optional_i64(&params[2]),
            ..Default::default()
        },
        visibility_flag: parse_required_i64(&params[3], "invisibility")?,
    })
}

fn parse_sfig_org_feature(params: &[Value]) -> Result<SfigOrgFeature, String> {
    if params.len() < 2 {
        return Err(format!(
            "sfig_org_feature requires 2 parameters, got {}",
            params.len()
        ));
    }
    Ok(SfigOrgFeature {
        name: parse_required_string(&params[0], "name")?,
        kind_flag: parse_required_i64(&params[1], "kind_flag")?,
    })
}

fn parse_sfig_locate_feature(params: &[Value]) -> Result<SfigLocateFeature, String> {
    if params.len() < 7 {
        return Err(format!(
            "sfig_locate_feature requires 7 parameters, got {}",
            params.len()
        ));
    }
    Ok(SfigLocateFeature {
        style: CommonStyle {
            layer_code: Some(parse_required_i64(&params[0], "layer")?),
            ..Default::default()
        },
        name: parse_required_string(&params[1], "name")?,
        position: Point2 {
            x: parse_required_f64(&params[2], "x")?,
            y: parse_required_f64(&params[3], "y")?,
        },
        angle_deg: parse_required_f64(&params[4], "angle")?,
        ratio_x: parse_required_f64(&params[5], "ratio_x")?,
        ratio_y: parse_required_f64(&params[6], "ratio_y")?,
    })
}

fn parse_externally_defined_symbol_feature(
    params: &[Value],
) -> Result<ExternallyDefinedSymbolFeature, String> {
    if params.len() < 8 {
        return Err(format!(
            "externally_defined_symbol_feature requires 8 parameters, got {}",
            params.len()
        ));
    }
    Ok(ExternallyDefinedSymbolFeature {
        style: parse_layer_color_style(params)?,
        color_flag: parse_required_i64(&params[2], "color_flag")?,
        name: parse_required_string(&params[3], "name")?,
        position: Point2 {
            x: parse_required_f64(&params[4], "x")?,
            y: parse_required_f64(&params[5], "y")?,
        },
        rotation_angle_deg: parse_required_f64(&params[6], "rotation_angle")?,
        scale: parse_required_f64(&params[7], "scale")?,
    })
}

fn parse_linear_dim_feature(params: &[Value]) -> Result<LinearDimFeature, String> {
    if params.len() < 4 {
        return Err(format!(
            "linear_dim_feature requires at least 4 parameters, got {}",
            params.len()
        ));
    }
    Ok(LinearDimFeature {
        style: parse_line_style(params)?,
        raw_parameters: params[4..].to_vec(),
    })
}

fn parse_curve_dim_feature(params: &[Value]) -> Result<CurveDimFeature, String> {
    if params.len() < 4 {
        return Err(format!(
            "curve_dim_feature requires at least 4 parameters, got {}",
            params.len()
        ));
    }
    Ok(CurveDimFeature {
        style: parse_line_style(params)?,
        raw_parameters: params[4..].to_vec(),
    })
}

fn parse_angular_dim_feature(params: &[Value]) -> Result<AngularDimFeature, String> {
    if params.len() < 4 {
        return Err(format!(
            "angular_dim_feature requires at least 4 parameters, got {}",
            params.len()
        ));
    }
    Ok(AngularDimFeature {
        style: parse_line_style(params)?,
        raw_parameters: params[4..].to_vec(),
    })
}

fn parse_radius_dim_feature(params: &[Value]) -> Result<RadiusDimFeature, String> {
    if params.len() < 4 {
        return Err(format!(
            "radius_dim_feature requires at least 4 parameters, got {}",
            params.len()
        ));
    }
    Ok(RadiusDimFeature {
        style: parse_line_style(params)?,
        raw_parameters: params[4..].to_vec(),
    })
}

fn parse_diameter_dim_feature(params: &[Value]) -> Result<DiameterDimFeature, String> {
    if params.len() < 4 {
        return Err(format!(
            "diameter_dim_feature requires at least 4 parameters, got {}",
            params.len()
        ));
    }
    Ok(DiameterDimFeature {
        style: parse_line_style(params)?,
        raw_parameters: params[4..].to_vec(),
    })
}

fn parse_points_from_xy_lists(
    xs: Vec<f64>,
    ys: Vec<f64>,
    feature_name: &str,
) -> Result<Vec<Point2>, String> {
    let point_count = xs.len().min(ys.len());
    if point_count == 0 {
        return Err(format!("{feature_name} has no vertices"));
    }
    let mut points = Vec::with_capacity(point_count);
    for i in 0..point_count {
        points.push(Point2 { x: xs[i], y: ys[i] });
    }
    Ok(points)
}

fn parse_label_feature(params: &[Value]) -> Result<LabelFeature, String> {
    if params.len() < 7 {
        return Err(format!(
            "label_feature requires at least 7 parameters, got {}",
            params.len()
        ));
    }
    let style = parse_line_style(params)?;
    let declared = parse_required_i64(&params[4], "number")?;
    let points = parse_points_from_xy_lists(
        parse_required_f64_list(&params[5], "x coordinates")?,
        parse_required_f64_list(&params[6], "y coordinates")?,
        "label_feature",
    )?;
    Ok(LabelFeature {
        style,
        declared_vertex_count: usize::try_from(declared).ok(),
        vertices: points,
        arrow_code: params.get(7).and_then(parse_optional_i64),
        arrow_scale: params.get(8).and_then(Value::as_f64),
    })
}

fn parse_balloon_feature(params: &[Value]) -> Result<BalloonFeature, String> {
    if params.len() < 10 {
        return Err(format!(
            "balloon_feature requires at least 10 parameters, got {}",
            params.len()
        ));
    }
    let style = parse_line_style(params)?;
    let declared = parse_required_i64(&params[4], "number")?;
    let points = parse_points_from_xy_lists(
        parse_required_f64_list(&params[5], "x coordinates")?,
        parse_required_f64_list(&params[6], "y coordinates")?,
        "balloon_feature",
    )?;
    Ok(BalloonFeature {
        style,
        declared_vertex_count: usize::try_from(declared).ok(),
        vertices: points,
        center: Point2 {
            x: parse_required_f64(&params[7], "center_x")?,
            y: parse_required_f64(&params[8], "center_y")?,
        },
        radius: parse_required_f64(&params[9], "radius")?,
        arrow_code: params.get(10).and_then(parse_optional_i64),
        arrow_scale: params.get(11).and_then(Value::as_f64),
    })
}

fn parse_externally_defined_hatch_feature(
    params: &[Value],
) -> Result<ExternallyDefinedHatchFeature, String> {
    if params.len() < 5 {
        return Err(format!(
            "externally_defined_hatch_feature requires 5 parameters, got {}",
            params.len()
        ));
    }
    Ok(ExternallyDefinedHatchFeature {
        style: CommonStyle {
            layer_code: Some(parse_required_i64(&params[0], "layer")?),
            ..Default::default()
        },
        name: parse_required_string(&params[1], "name")?,
        out_id: parse_required_i64(&params[2], "out_id")?,
        hole_count: parse_optional_i64(&params[3]).and_then(|v| usize::try_from(v).ok()),
        in_ids: parse_required_i64_list(&params[4], "in_ids")?,
    })
}

fn parse_fill_area_style_colour_feature(
    params: &[Value],
) -> Result<FillAreaStyleColourFeature, String> {
    if params.len() < 5 {
        return Err(format!(
            "fill_area_style_colour_feature requires 5 parameters, got {}",
            params.len()
        ));
    }
    Ok(FillAreaStyleColourFeature {
        style: CommonStyle {
            layer_code: Some(parse_required_i64(&params[0], "layer")?),
            color_code: Some(parse_required_i64(&params[1], "color")?),
            ..Default::default()
        },
        out_id: parse_required_i64(&params[2], "out_id")?,
        hole_count: parse_optional_i64(&params[3]).and_then(|v| usize::try_from(v).ok()),
        in_ids: parse_required_i64_list(&params[4], "in_ids")?,
    })
}

fn parse_fill_area_style_hatching_feature(
    params: &[Value],
) -> Result<FillAreaStyleHatchingFeature, String> {
    if params.len() < 6 {
        return Err(format!(
            "fill_area_style_hatching_feature requires at least 6 parameters, got {}",
            params.len()
        ));
    }

    let hatch_number = parse_required_i64(&params[1], "hatch_number")?;
    let hatch_count = usize::try_from(hatch_number)
        .map_err(|_| "hatch_number must be a non-negative integer".to_string())?;
    let hatch_end = 2 + hatch_count;
    if params.len() < hatch_end + 3 {
        return Err(format!(
            "fill_area_style_hatching_feature expected {} hatch patterns and trailing out_id/hole info, got {} parameters",
            hatch_count,
            params.len()
        ));
    }

    Ok(FillAreaStyleHatchingFeature {
        style: CommonStyle {
            layer_code: Some(parse_required_i64(&params[0], "layer")?),
            ..Default::default()
        },
        hatch_number,
        hatch_patterns: params[2..hatch_end].to_vec(),
        out_id: parse_required_i64(&params[hatch_end], "out_id")?,
        hole_count: parse_optional_i64(&params[hatch_end + 1])
            .and_then(|v| usize::try_from(v).ok()),
        in_ids: parse_required_i64_list(&params[hatch_end + 2], "in_ids")?,
    })
}

fn parse_fill_area_style_tiles_feature(
    params: &[Value],
) -> Result<FillAreaStyleTilesFeature, String> {
    if params.len() < 15 {
        return Err(format!(
            "fill_area_style_tiles_feature requires 15 parameters, got {}",
            params.len()
        ));
    }
    Ok(FillAreaStyleTilesFeature {
        style: CommonStyle {
            layer_code: Some(parse_required_i64(&params[0], "layer")?),
            ..Default::default()
        },
        name: parse_required_string(&params[1], "name")?,
        hatch_color: parse_required_i64(&params[2], "hatch_color")?,
        hatch_pattern_position: Point2 {
            x: parse_required_f64(&params[3], "hatch_pattern_position_x")?,
            y: parse_required_f64(&params[4], "hatch_pattern_position_y")?,
        },
        out_id: parse_required_i64(&params[12], "out_id")?,
        hatch_pattern_vector1: parse_required_f64(&params[5], "hatch_pattern_vector1")?,
        hatch_pattern_vector1_angle_deg: parse_required_f64(
            &params[6],
            "hatch_pattern_vector1_angle",
        )?,
        hatch_pattern_vector2: parse_required_f64(&params[7], "hatch_pattern_vector2")?,
        hatch_pattern_vector2_angle_deg: parse_required_f64(
            &params[8],
            "hatch_pattern_vector2_angle",
        )?,
        hatch_pattern_scale_x: parse_required_f64(&params[9], "hatch_pattern_scale_x")?,
        hatch_pattern_scale_y: parse_required_f64(&params[10], "hatch_pattern_scale_y")?,
        hatch_pattern_angle_deg: parse_required_f64(&params[11], "hatch_pattern_angle")?,
        hole_count: parse_optional_i64(&params[13]).and_then(|v| usize::try_from(v).ok()),
        in_ids: parse_required_i64_list(&params[14], "in_ids")?,
    })
}

fn parse_layer_color_style(params: &[Value]) -> Result<CommonStyle, String> {
    Ok(CommonStyle {
        layer_code: Some(parse_required_i64(&params[0], "layer")?),
        color_code: Some(parse_required_i64(&params[1], "color")?),
        ..Default::default()
    })
}

fn parse_line_style(params: &[Value]) -> Result<CommonStyle, String> {
    Ok(CommonStyle {
        layer_code: Some(parse_required_i64(&params[0], "layer")?),
        color_code: Some(parse_required_i64(&params[1], "color")?),
        line_type_code: Some(parse_required_i64(&params[2], "line_type")?),
        line_width_code: Some(parse_required_i64(&params[3], "line_width")?),
        ..Default::default()
    })
}

fn parse_required_string(value: &Value, name: &str) -> Result<String, String> {
    value
        .as_string()
        .ok_or_else(|| format!("{name} must be a string"))
}

fn parse_required_i64(value: &Value, name: &str) -> Result<i64, String> {
    value
        .as_i64()
        .ok_or_else(|| format!("{name} must be an integer-compatible value"))
}

fn parse_optional_i64(value: &Value) -> Option<i64> {
    value.as_i64()
}

fn parse_required_f64(value: &Value, name: &str) -> Result<f64, String> {
    value
        .as_f64()
        .ok_or_else(|| format!("{name} must be a numeric value"))
}

fn parse_required_f64_list(value: &Value, name: &str) -> Result<Vec<f64>, String> {
    value
        .as_f64_list()
        .ok_or_else(|| format!("{name} must be a numeric list"))
}

fn parse_required_i64_list(value: &Value, name: &str) -> Result<Vec<i64>, String> {
    value
        .as_i64_list()
        .ok_or_else(|| format!("{name} must be an integer list"))
}

fn is_keyword_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_keyword_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

pub fn parse_p21_text(input: &str, strict: bool) -> Result<ParseOutput, ParseError> {
    Parser::new(input, FileFormat::P21, strict).parse()
}

pub fn parse_sfc_text(input: &str, strict: bool) -> Result<ParseOutput, ParseError> {
    Parser::new(input, FileFormat::Sfc, strict).parse()
}

fn decode_bytes(
    format: FileFormat,
    bytes: &[u8],
) -> Result<(String, Option<ParseWarning>), ParseError> {
    match String::from_utf8(bytes.to_vec()) {
        Ok(text) => Ok((text, None)),
        Err(_) if format == FileFormat::Sfc => {
            let decoded = String::from_utf8_lossy(bytes).into_owned();
            Ok((
                decoded,
                Some(ParseWarning {
                    code: "encoding".to_string(),
                    message: "Input was not valid UTF-8. Parsed with lossy decoding.".to_string(),
                }),
            ))
        }
        Err(_) => Err(ParseError::new("Input is not valid UTF-8", 1, 1, "")),
    }
}

fn parse_from_bytes(
    format: FileFormat,
    bytes: &[u8],
    strict: bool,
) -> Result<ParseOutput, ParseError> {
    let (text, encoding_warning) = decode_bytes(format, bytes)?;
    let mut output = match format {
        FileFormat::P21 => parse_p21_text(&text, strict)?,
        FileFormat::Sfc => parse_sfc_text(&text, strict)?,
    };
    if let Some(warning) = encoding_warning {
        output.warnings.push(warning);
    }
    Ok(output)
}

fn resolve_python_input(input: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    if let Ok(raw) = input.extract::<&[u8]>() {
        return Ok(raw.to_vec());
    }

    if let Ok(text) = input.extract::<String>() {
        let path = Path::new(&text);
        if path.exists() {
            return fs::read(path).map_err(|error| {
                PyIOError::new_err(format!("Failed to read file {}: {error}", path.display()))
            });
        }
        return Ok(text.into_bytes());
    }

    Err(PyTypeError::new_err(
        "input must be bytes or str (path or text content)",
    ))
}

fn parse_error_to_py(error: ParseError) -> PyErr {
    PyValueError::new_err(error.to_string())
}

fn value_to_python<'py>(py: Python<'py>, value: &Value) -> PyResult<PyObject> {
    match value {
        Value::Unset => {
            let d = PyDict::new_bound(py);
            d.set_item("kind", "unset")?;
            Ok(d.into_py(py))
        }
        Value::Omitted => {
            let d = PyDict::new_bound(py);
            d.set_item("kind", "omitted")?;
            Ok(d.into_py(py))
        }
        Value::Integer(v) => Ok(v.into_py(py)),
        Value::Real(v) => Ok(v.into_py(py)),
        Value::String(v) => Ok(v.into_py(py)),
        Value::Enum(v) => {
            let d = PyDict::new_bound(py);
            d.set_item("kind", "enum")?;
            d.set_item("value", v)?;
            Ok(d.into_py(py))
        }
        Value::Reference(v) => {
            let d = PyDict::new_bound(py);
            d.set_item("kind", "reference")?;
            d.set_item("value", *v)?;
            Ok(d.into_py(py))
        }
        Value::Binary(v) => {
            let d = PyDict::new_bound(py);
            d.set_item("kind", "binary")?;
            d.set_item("value", v)?;
            Ok(d.into_py(py))
        }
        Value::List(values) => {
            let list = PyList::empty_bound(py);
            for item in values {
                list.append(value_to_python(py, item)?)?;
            }
            Ok(list.into_py(py))
        }
        Value::Typed {
            keyword,
            parameters,
        } => {
            let d = PyDict::new_bound(py);
            d.set_item("kind", "typed")?;
            d.set_item("keyword", keyword)?;
            let params = PyList::empty_bound(py);
            for item in parameters {
                params.append(value_to_python(py, item)?)?;
            }
            d.set_item("parameters", params)?;
            Ok(d.into_py(py))
        }
    }
}

fn record_to_python<'py>(py: Python<'py>, record: &Record) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new_bound(py);
    d.set_item("keyword", &record.keyword)?;
    let params = PyList::empty_bound(py);
    for value in &record.parameters {
        params.append(value_to_python(py, value)?)?;
    }
    d.set_item("parameters", params)?;
    Ok(d)
}

fn style_to_python<'py>(py: Python<'py>, style: &CommonStyle) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new_bound(py);
    d.set_item("layer_code", style.layer_code)?;
    d.set_item("color_code", style.color_code)?;
    d.set_item("line_type_code", style.line_type_code)?;
    d.set_item("line_width_code", style.line_width_code)?;
    d.set_item("font_code", style.font_code)?;
    Ok(d)
}

fn point_to_python<'py>(py: Python<'py>, point: &Point2) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new_bound(py);
    d.set_item("x", point.x)?;
    d.set_item("y", point.y)?;
    Ok(d)
}

fn typed_feature_instance_to_python<'py>(
    py: Python<'py>,
    instance: &TypedFeatureInstance,
) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new_bound(py);
    d.set_item("id", instance.id)?;
    d.set_item("keyword", &instance.keyword)?;

    match &instance.feature {
        TypedFeature::DrawingAttribute(feature) => {
            d.set_item("kind", "drawing_attribute")?;
            d.set_item("project_name", &feature.project_name)?;
            d.set_item("construction_name", &feature.construction_name)?;
            d.set_item("contract_type", &feature.contract_type)?;
            d.set_item("drawing_name", &feature.drawing_name)?;
            d.set_item("drawing_number", &feature.drawing_number)?;
            d.set_item("drawing_type", &feature.drawing_type)?;
            d.set_item("drawing_scale", &feature.drawing_scale)?;
            d.set_item("drawing_year", feature.drawing_year)?;
            d.set_item("drawing_month", feature.drawing_month)?;
            d.set_item("drawing_day", feature.drawing_day)?;
            d.set_item("contractor_name", &feature.contractor_name)?;
            d.set_item("owner_name", &feature.owner_name)?;
        }
        TypedFeature::DrawingSheet(feature) => {
            d.set_item("kind", "drawing_sheet")?;
            d.set_item("name", &feature.name)?;
            d.set_item("sheet_type", feature.sheet_type)?;
            d.set_item("orientation", feature.orientation)?;
            d.set_item("free_x_mm", feature.free_x_mm)?;
            d.set_item("free_y_mm", feature.free_y_mm)?;
        }
        TypedFeature::Layer(feature) => {
            d.set_item("kind", "layer")?;
            d.set_item("name", &feature.name)?;
            d.set_item("visibility_flag", feature.visibility_flag)?;
        }
        TypedFeature::PreDefinedFont(feature) => {
            d.set_item("kind", "pre_defined_font")?;
            d.set_item("name", &feature.name)?;
        }
        TypedFeature::UserDefinedFont(feature) => {
            d.set_item("kind", "user_defined_font")?;
            d.set_item("name", &feature.name)?;
            d.set_item("segment_count", feature.segment_count)?;
            d.set_item("pitch", feature.pitch.clone())?;
        }
        TypedFeature::PreDefinedColour(feature) => {
            d.set_item("kind", "pre_defined_colour")?;
            d.set_item("name", &feature.name)?;
        }
        TypedFeature::UserDefinedColour(feature) => {
            d.set_item("kind", "user_defined_colour")?;
            d.set_item("red", feature.red)?;
            d.set_item("green", feature.green)?;
            d.set_item("blue", feature.blue)?;
        }
        TypedFeature::Width(feature) => {
            d.set_item("kind", "width")?;
            d.set_item("width", feature.width)?;
        }
        TypedFeature::TextFont(feature) => {
            d.set_item("kind", "text_font")?;
            d.set_item("name", &feature.name)?;
        }
        TypedFeature::SfigOrg(feature) => {
            d.set_item("kind", "sfig_org")?;
            d.set_item("name", &feature.name)?;
            d.set_item("kind_flag", feature.kind_flag)?;
        }
        TypedFeature::SfigLocate(feature) => {
            d.set_item("kind", "sfig_locate")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("name", &feature.name)?;
            d.set_item("position", point_to_python(py, &feature.position)?)?;
            d.set_item("angle_deg", feature.angle_deg)?;
            d.set_item("ratio_x", feature.ratio_x)?;
            d.set_item("ratio_y", feature.ratio_y)?;
        }
        TypedFeature::ExternallyDefinedSymbol(feature) => {
            d.set_item("kind", "externally_defined_symbol")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("color_flag", feature.color_flag)?;
            d.set_item("name", &feature.name)?;
            d.set_item("position", point_to_python(py, &feature.position)?)?;
            d.set_item("rotation_angle_deg", feature.rotation_angle_deg)?;
            d.set_item("scale", feature.scale)?;
        }
        TypedFeature::LinearDim(feature) => {
            d.set_item("kind", "linear_dim")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            let raw_parameters = PyList::empty_bound(py);
            for value in &feature.raw_parameters {
                raw_parameters.append(value_to_python(py, value)?)?;
            }
            d.set_item("raw_parameters", raw_parameters)?;
        }
        TypedFeature::CurveDim(feature) => {
            d.set_item("kind", "curve_dim")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            let raw_parameters = PyList::empty_bound(py);
            for value in &feature.raw_parameters {
                raw_parameters.append(value_to_python(py, value)?)?;
            }
            d.set_item("raw_parameters", raw_parameters)?;
        }
        TypedFeature::AngularDim(feature) => {
            d.set_item("kind", "angular_dim")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            let raw_parameters = PyList::empty_bound(py);
            for value in &feature.raw_parameters {
                raw_parameters.append(value_to_python(py, value)?)?;
            }
            d.set_item("raw_parameters", raw_parameters)?;
        }
        TypedFeature::RadiusDim(feature) => {
            d.set_item("kind", "radius_dim")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            let raw_parameters = PyList::empty_bound(py);
            for value in &feature.raw_parameters {
                raw_parameters.append(value_to_python(py, value)?)?;
            }
            d.set_item("raw_parameters", raw_parameters)?;
        }
        TypedFeature::DiameterDim(feature) => {
            d.set_item("kind", "diameter_dim")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            let raw_parameters = PyList::empty_bound(py);
            for value in &feature.raw_parameters {
                raw_parameters.append(value_to_python(py, value)?)?;
            }
            d.set_item("raw_parameters", raw_parameters)?;
        }
        TypedFeature::Label(feature) => {
            d.set_item("kind", "label")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("declared_vertex_count", feature.declared_vertex_count)?;
            let vertices = PyList::empty_bound(py);
            for point in &feature.vertices {
                vertices.append(point_to_python(py, point)?)?;
            }
            d.set_item("vertices", vertices)?;
            d.set_item("arrow_code", feature.arrow_code)?;
            d.set_item("arrow_scale", feature.arrow_scale)?;
        }
        TypedFeature::Balloon(feature) => {
            d.set_item("kind", "balloon")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("declared_vertex_count", feature.declared_vertex_count)?;
            let vertices = PyList::empty_bound(py);
            for point in &feature.vertices {
                vertices.append(point_to_python(py, point)?)?;
            }
            d.set_item("vertices", vertices)?;
            d.set_item("center", point_to_python(py, &feature.center)?)?;
            d.set_item("radius", feature.radius)?;
            d.set_item("arrow_code", feature.arrow_code)?;
            d.set_item("arrow_scale", feature.arrow_scale)?;
        }
        TypedFeature::ExternallyDefinedHatch(feature) => {
            d.set_item("kind", "externally_defined_hatch")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("name", &feature.name)?;
            d.set_item("out_id", feature.out_id)?;
            d.set_item("hole_count", feature.hole_count)?;
            d.set_item("in_ids", feature.in_ids.clone())?;
        }
        TypedFeature::FillAreaStyleColour(feature) => {
            d.set_item("kind", "fill_area_style_colour")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("out_id", feature.out_id)?;
            d.set_item("hole_count", feature.hole_count)?;
            d.set_item("in_ids", feature.in_ids.clone())?;
        }
        TypedFeature::FillAreaStyleHatching(feature) => {
            d.set_item("kind", "fill_area_style_hatching")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("hatch_number", feature.hatch_number)?;
            let hatch_patterns = PyList::empty_bound(py);
            for value in &feature.hatch_patterns {
                hatch_patterns.append(value_to_python(py, value)?)?;
            }
            d.set_item("hatch_patterns", hatch_patterns)?;
            d.set_item("out_id", feature.out_id)?;
            d.set_item("hole_count", feature.hole_count)?;
            d.set_item("in_ids", feature.in_ids.clone())?;
        }
        TypedFeature::FillAreaStyleTiles(feature) => {
            d.set_item("kind", "fill_area_style_tiles")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("name", &feature.name)?;
            d.set_item("hatch_color", feature.hatch_color)?;
            d.set_item(
                "hatch_pattern_position",
                point_to_python(py, &feature.hatch_pattern_position)?,
            )?;
            d.set_item("out_id", feature.out_id)?;
            d.set_item("hatch_pattern_vector1", feature.hatch_pattern_vector1)?;
            d.set_item(
                "hatch_pattern_vector1_angle_deg",
                feature.hatch_pattern_vector1_angle_deg,
            )?;
            d.set_item("hatch_pattern_vector2", feature.hatch_pattern_vector2)?;
            d.set_item(
                "hatch_pattern_vector2_angle_deg",
                feature.hatch_pattern_vector2_angle_deg,
            )?;
            d.set_item("hatch_pattern_scale_x", feature.hatch_pattern_scale_x)?;
            d.set_item("hatch_pattern_scale_y", feature.hatch_pattern_scale_y)?;
            d.set_item("hatch_pattern_angle_deg", feature.hatch_pattern_angle_deg)?;
            d.set_item("hole_count", feature.hole_count)?;
            d.set_item("in_ids", feature.in_ids.clone())?;
        }
        TypedFeature::PointMarker(feature) => {
            d.set_item("kind", "point_marker")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("position", point_to_python(py, &feature.position)?)?;
            d.set_item("marker_code", feature.marker_code)?;
            d.set_item("rotation_angle_deg", feature.rotation_angle_deg)?;
            d.set_item("scale", feature.scale)?;
        }
        TypedFeature::Line(feature) => {
            d.set_item("kind", "line")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("start", point_to_python(py, &feature.start)?)?;
            d.set_item("end", point_to_python(py, &feature.end)?)?;
        }
        TypedFeature::Polyline(feature) => {
            d.set_item("kind", "polyline")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("declared_point_count", feature.declared_point_count)?;
            let points = PyList::empty_bound(py);
            for point in &feature.points {
                points.append(point_to_python(py, point)?)?;
            }
            d.set_item("points", points)?;
        }
        TypedFeature::Circle(feature) => {
            d.set_item("kind", "circle")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("center", point_to_python(py, &feature.center)?)?;
            d.set_item("radius", feature.radius)?;
        }
        TypedFeature::Arc(feature) => {
            d.set_item("kind", "arc")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("center", point_to_python(py, &feature.center)?)?;
            d.set_item("radius", feature.radius)?;
            d.set_item("direction_flag", feature.direction_flag)?;
            d.set_item("start_angle_deg", feature.start_angle_deg)?;
            d.set_item("end_angle_deg", feature.end_angle_deg)?;
        }
        TypedFeature::Ellipse(feature) => {
            d.set_item("kind", "ellipse")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("center", point_to_python(py, &feature.center)?)?;
            d.set_item("radius_x", feature.radius_x)?;
            d.set_item("radius_y", feature.radius_y)?;
            d.set_item("rotation_angle_deg", feature.rotation_angle_deg)?;
        }
        TypedFeature::EllipseArc(feature) => {
            d.set_item("kind", "ellipse_arc")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("center", point_to_python(py, &feature.center)?)?;
            d.set_item("radius_x", feature.radius_x)?;
            d.set_item("radius_y", feature.radius_y)?;
            d.set_item("direction_flag", feature.direction_flag)?;
            d.set_item("rotation_angle_deg", feature.rotation_angle_deg)?;
            d.set_item("start_angle_deg", feature.start_angle_deg)?;
            d.set_item("end_angle_deg", feature.end_angle_deg)?;
        }
        TypedFeature::Text(feature) => {
            d.set_item("kind", "text")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("text", &feature.text)?;
            d.set_item("anchor", point_to_python(py, &feature.anchor)?)?;
            d.set_item("height", feature.height)?;
            d.set_item("width", feature.width)?;
            d.set_item("spacing", feature.spacing)?;
            d.set_item("angle_deg", feature.angle_deg)?;
            d.set_item("slant_deg", feature.slant_deg)?;
            d.set_item("base_point", feature.base_point)?;
            d.set_item("direction", feature.direction)?;
        }
        TypedFeature::Spline(feature) => {
            d.set_item("kind", "spline")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("open_close", feature.open_close)?;
            d.set_item("declared_point_count", feature.declared_point_count)?;
            let points = PyList::empty_bound(py);
            for point in &feature.points {
                points.append(point_to_python(py, point)?)?;
            }
            d.set_item("points", points)?;
        }
        TypedFeature::Clothoid(feature) => {
            d.set_item("kind", "clothoid")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("base", point_to_python(py, &feature.base)?)?;
            d.set_item("parameter", feature.parameter)?;
            d.set_item("direction_flag", feature.direction_flag)?;
            d.set_item("angle_deg", feature.angle_deg)?;
            d.set_item("start_length", feature.start_length)?;
            d.set_item("end_length", feature.end_length)?;
        }
        TypedFeature::CompositeCurve(feature) => {
            d.set_item("kind", "composite_curve")?;
            d.set_item("style", style_to_python(py, &feature.style)?)?;
            d.set_item("visibility_flag", feature.visibility_flag)?;
        }
    }

    Ok(d)
}

fn output_to_python<'py>(py: Python<'py>, output: &ParseOutput) -> PyResult<Bound<'py, PyDict>> {
    let root = PyDict::new_bound(py);
    root.set_item("format", output.document.format.as_str())?;

    let header = PyDict::new_bound(py);
    let header_entities = PyList::empty_bound(py);
    for entity in &output.document.header.entities {
        header_entities.append(record_to_python(py, entity)?)?;
    }
    header.set_item("entities", header_entities)?;

    if let Some(record) = output.document.header.find_keyword("FILE_DESCRIPTION") {
        header.set_item("file_description", record_to_python(py, record)?)?;
    }
    if let Some(record) = output.document.header.find_keyword("FILE_NAME") {
        header.set_item("file_name", record_to_python(py, record)?)?;
    }
    if let Some(record) = output.document.header.find_keyword("FILE_SCHEMA") {
        header.set_item("file_schema", record_to_python(py, record)?)?;
    }
    root.set_item("header", header)?;

    let entities = PyList::empty_bound(py);
    for entity in &output.document.entities {
        let d = PyDict::new_bound(py);
        d.set_item("id", entity.id)?;
        match &entity.body {
            EntityBody::Simple(record) => {
                d.set_item("body_type", "simple")?;
                d.set_item("record", record_to_python(py, record)?)?;
            }
            EntityBody::Complex(records) => {
                d.set_item("body_type", "complex")?;
                let record_list = PyList::empty_bound(py);
                for record in records {
                    record_list.append(record_to_python(py, record)?)?;
                }
                d.set_item("records", record_list)?;
            }
        }
        entities.append(d)?;
    }
    root.set_item("entities", entities)?;

    let warnings = PyList::empty_bound(py);
    for warning in &output.warnings {
        let d = PyDict::new_bound(py);
        d.set_item("code", &warning.code)?;
        d.set_item("message", &warning.message)?;
        warnings.append(d)?;
    }
    root.set_item("warnings", warnings)?;

    let typed_features = PyList::empty_bound(py);
    for feature in &output.document.typed_features {
        typed_features.append(typed_feature_instance_to_python(py, feature)?)?;
    }
    root.set_item("typed_features", typed_features)?;

    Ok(root)
}

fn parse_for_python(
    py: Python<'_>,
    format: FileFormat,
    input: &Bound<'_, PyAny>,
    strict: Option<bool>,
) -> PyResult<Py<PyDict>> {
    let bytes = resolve_python_input(input)?;
    let parsed =
        parse_from_bytes(format, &bytes, strict.unwrap_or(true)).map_err(parse_error_to_py)?;
    let output = output_to_python(py, &parsed)?;
    Ok(output.unbind())
}

#[pyfunction]
fn hello_from_bin() -> String {
    "Hello from ezsxf!".to_string()
}

#[allow(clippy::useless_conversion)]
#[pyfunction]
#[pyo3(signature = (input, strict=None))]
fn parse_p21(
    py: Python<'_>,
    input: &Bound<'_, PyAny>,
    strict: Option<bool>,
) -> PyResult<Py<PyDict>> {
    parse_for_python(py, FileFormat::P21, input, strict)
}

#[allow(clippy::useless_conversion)]
#[pyfunction]
#[pyo3(signature = (input, strict=None))]
fn parse_sfc(
    py: Python<'_>,
    input: &Bound<'_, PyAny>,
    strict: Option<bool>,
) -> PyResult<Py<PyDict>> {
    parse_for_python(py, FileFormat::Sfc, input, strict)
}

#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(hello_from_bin, m)?)?;
    m.add_function(wrap_pyfunction!(parse_p21, m)?)?;
    m.add_function(wrap_pyfunction!(parse_sfc, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::ffi::OsStr;
    use std::fs;
    use std::path::{Path, PathBuf};

    const HEADER: &str = "ISO-10303-21;
HEADER;
FILE_DESCRIPTION(('SCADEC level2 AP202_mode'),'1');
FILE_NAME('sample.p21','2007-06-29T07:56:58',('author'),('organization'),'translator$$3.1','system','');
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
            assert!(
                sfc.document
                    .typed_features
                    .iter()
                    .any(|item| matches!(item.feature, TypedFeature::DrawingSheet(_))),
                "SFC must contain drawing_sheet_feature: {}",
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
#10 = line_feature('2','2','1','1','0.000000','0.000000','10.000000','10.000000')
SXF*/
/*SXF
#11 = polyline_feature('1','4','3','1','4','(-5.000000,5.000000,5.000000,-5.000000)','(0.000000,0.000000,5.000000,0.000000)')
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
#15 = composite_curve_org_feature('8','8','2','1')
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
            TypedFeature::Line(_)
        ));
        assert!(matches!(
            output.document.typed_features[4].feature,
            TypedFeature::Polyline(_)
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
            TypedFeature::CompositeCurve(_)
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

        let output =
            parse_sfc_text(&source, true).expect("additional geometry features must parse");

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
#10 = sfig_org_feature('sfig','4')
SXF*/
/*SXF
#11 = sfig_locate_feature('1','sfig','0.000000','0.000000','0.00000000000000','2.00000000000000','2.00000000000000')
SXF*/
/*SXF
#12 = externally_defined_symbol_feature('1','1','11','40201050100000&&a0010010','1.300000','1.400000','0.00000000000000','1.00000000000000')
SXF*/
/*SXF
#13 = linear_dim_feature('3','1','3','1','1.000000')
SXF*/
/*SXF
#14 = curve_dim_feature('3','1','3','1','0.000000')
SXF*/
/*SXF
#15 = angular_dim_feature('3','1','3','1','0.000000')
SXF*/
/*SXF
#16 = radius_dim_feature('3','1','3','1','0.000000')
SXF*/
/*SXF
#17 = diameter_dim_feature('3','1','3','1','0.000000')
SXF*/
/*SXF
#18 = label_feature('1','1','1','1','2','(1.000000,3.000000)','(2.000000,4.000000)','1','3.00000000000000')
SXF*/
/*SXF
#19 = balloon_feature('1','1','1','1','2','(1.000000,3.000000)','(2.000000,4.000000)','0.000000','0.000000','10.000000','4','3.00000000000000')
SXF*/
/*SXF
#20 = externally_defined_hatch_feature('2','sxf_hatch_style_1','1','1','(2)')
SXF*/
/*SXF
#21 = fill_area_style_colour_feature('2','8','1','1','(2)')
SXF*/
/*SXF
#22 = fill_area_style_hatching_feature('3','2','(1,3,2,11.000000,12.000000,5.000000,60.0000000000000)','(10,1,11,1.000000,2.000000,5.000000,30.0000000000000)','1','1','(2)')
SXF*/
/*SXF
#23 = fill_area_style_tiles_feature('3','40201050100000&&a0010010','18','11.000000','12.000000','13.000000','30.0000000000000','15.000000','60.0000000000000','1.00000000000000','1.00000000000000','45.0000000000000','1','1','(2)')
SXF*/
/*SXF
#99 = drawing_sheet_feature('sheet','9','1','100','300')
SXF*/
ENDSEC;
END-ISO-10303-21;",
            header = HEADER
        );

        let output =
            parse_sfc_text(&source, true).expect("structured and hatch features must parse");

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
#9 = line_feature('1','2','1','1','0.000000','0.000000','10.000000','10.000000')
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

        let output =
            parse_sfc_text(&source, true).expect("strict mode should skip incomplete feature");
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
#5 = line_feature('1','1','9','1','0.000000','0.000000','10.000000','10.000000')
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
        let output = parse_sfc_text(&source, true)
            .expect("quote inside backslash-quoted string should parse");
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
}
