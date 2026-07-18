//! Public parser output and typed SXF feature model.

// SXF Ver.3.1 common predefined element code ranges (別冊「共通既定義要素編」).
pub(crate) const COMMON_PREDEFINED_LINE_TYPE_MAX_CODE: i64 = 15;
pub(crate) const COMMON_PREDEFINED_LINE_WIDTH_MAX_CODE: i64 = 9;
pub(crate) const COMMON_PREDEFINED_COLOR_MAX_CODE: i64 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFormat {
    P21,
    Sfc,
}

impl FileFormat {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::P21 => "p21",
            Self::Sfc => "sfc",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SfcVersionTag {
    V2,
    V3,
    V31,
}

impl SfcVersionTag {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::V2 => "2",
            Self::V3 => "3",
            Self::V31 => "3.1",
        }
    }

    pub(crate) fn marker(self) -> &'static str {
        match self {
            Self::V2 => "SXF",
            Self::V3 => "SXF3",
            Self::V31 => "SXF3.1",
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
    pub(crate) fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Integer(v) => Some(*v),
            Value::String(v) => v.trim().parse::<i64>().ok(),
            _ => None,
        }
    }

    pub(crate) fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Integer(v) => Some(*v as f64),
            Value::Real(v) => Some(*v),
            Value::String(v) => parse_sfc_decimal(v),
            _ => None,
        }
    }

    pub(crate) fn as_string(&self) -> Option<String> {
        match self {
            Value::String(v) => Some(v.clone()),
            _ => None,
        }
    }

    pub(crate) fn as_f64_list(&self) -> Option<Vec<f64>> {
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

    pub(crate) fn as_i64_list(&self) -> Option<Vec<i64>> {
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
        values.push(parse_sfc_decimal(part)?);
    }
    Some(values)
}

fn parse_sfc_decimal(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let unsigned = text
        .strip_prefix('+')
        .or_else(|| text.strip_prefix('-'))
        .unwrap_or(text);
    if unsigned.is_empty() {
        return None;
    }
    let mut dot_count = 0_usize;
    let mut digit_count = 0_usize;
    for ch in unsigned.chars() {
        if ch.is_ascii_digit() {
            digit_count += 1;
        } else if ch == '.' {
            dot_count += 1;
            if dot_count > 1 {
                return None;
            }
        } else {
            return None;
        }
    }
    if digit_count == 0 {
        return None;
    }
    let value = text.parse::<f64>().ok()?;
    value.is_finite().then_some(value)
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
        values.push(part.trim().parse::<i64>().ok()?);
    }
    Some(values)
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
    pub sfc_version: Option<SfcVersionTag>,
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
    pub sfc_model: Option<SfcModel>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypedFeatureInstance {
    pub id: i64,
    pub keyword: String,
    pub feature: TypedFeature,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SfcCodeBinding {
    pub code: i64,
    pub entity_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SfcCodeTables {
    pub layers: Vec<SfcCodeBinding>,
    pub line_types: Vec<SfcCodeBinding>,
    pub colors: Vec<SfcCodeBinding>,
    pub line_widths: Vec<SfcCodeBinding>,
    pub text_fonts: Vec<SfcCodeBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SfcSheetModel {
    pub entity_id: i64,
    pub component_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SfcSfigDefinition {
    pub entity_id: i64,
    pub name: String,
    pub kind_flag: i64,
    pub component_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SfcAttributeMechanism {
    AttributeFile {
        figure_id: String,
        attribute_file_name: Option<String>,
    },
    SingleAttribute {
        figure_id: String,
        figure_name: Option<String>,
        attribute_name: Option<String>,
        attribute_value: Option<String>,
        attribute_type: Option<String>,
        unit: Option<String>,
    },
    TextAttribute {
        figure_id: String,
        attribute_name: String,
        attribute_type: Option<String>,
        unit: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SfcAttributeAttachment {
    pub definition_id: i64,
    pub name: String,
    pub kind_flag: i64,
    pub component_ids: Vec<i64>,
    pub placement_ids: Vec<i64>,
    pub mechanism: SfcAttributeMechanism,
    pub resolved_attribute_file_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SfcCompositeCurveDefinition {
    pub code: i64,
    pub entity_id: i64,
    pub component_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SfcSfigReference {
    pub placement_id: i64,
    pub definition_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SfcHatchReference {
    pub hatch_id: i64,
    pub outer_definition_id: i64,
    pub inner_definition_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SfcModel {
    pub code_tables: SfcCodeTables,
    pub sheet: Option<SfcSheetModel>,
    pub sfig_definitions: Vec<SfcSfigDefinition>,
    pub attribute_attachments: Vec<SfcAttributeAttachment>,
    pub composite_curve_definitions: Vec<SfcCompositeCurveDefinition>,
    pub sfig_references: Vec<SfcSfigReference>,
    pub hatch_references: Vec<SfcHatchReference>,
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
pub struct ExtensionLine {
    pub present_flag: i64,
    pub base: Point2,
    pub start: Point2,
    pub end: Point2,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DimensionArrow {
    pub code: i64,
    pub direction_flag: i64,
    pub position: Point2,
    pub scale: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LeaderArrow {
    pub code: i64,
    pub scale: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FeatureText {
    pub present_flag: i64,
    pub font_code: i64,
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
    pub start: Point2,
    pub end: Point2,
    pub extension_line1: ExtensionLine,
    pub extension_line2: ExtensionLine,
    pub arrow1: DimensionArrow,
    pub arrow2: DimensionArrow,
    pub text: FeatureText,
    pub raw_parameters: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CurveDimFeature {
    pub style: CommonStyle,
    pub center: Point2,
    pub radius: f64,
    pub start_angle_deg: f64,
    pub end_angle_deg: f64,
    pub extension_line1: ExtensionLine,
    pub extension_line2: ExtensionLine,
    pub arrow1: DimensionArrow,
    pub arrow2: DimensionArrow,
    pub text: FeatureText,
    pub raw_parameters: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AngularDimFeature {
    pub style: CommonStyle,
    pub center: Point2,
    pub radius: f64,
    pub start_angle_deg: f64,
    pub end_angle_deg: f64,
    pub extension_line1: ExtensionLine,
    pub extension_line2: ExtensionLine,
    pub arrow1: DimensionArrow,
    pub arrow2: DimensionArrow,
    pub text: FeatureText,
    pub raw_parameters: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RadiusDimFeature {
    pub style: CommonStyle,
    pub start: Point2,
    pub end: Point2,
    pub arrow: DimensionArrow,
    pub text: FeatureText,
    pub raw_parameters: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiameterDimFeature {
    pub style: CommonStyle,
    pub start: Point2,
    pub end: Point2,
    pub arrow1: DimensionArrow,
    pub arrow2: DimensionArrow,
    pub text: FeatureText,
    pub raw_parameters: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LabelFeature {
    pub style: CommonStyle,
    pub declared_vertex_count: Option<usize>,
    pub vertices: Vec<Point2>,
    pub arrow: LeaderArrow,
    pub text: FeatureText,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BalloonFeature {
    pub style: CommonStyle,
    pub declared_vertex_count: Option<usize>,
    pub vertices: Vec<Point2>,
    pub center: Point2,
    pub radius: f64,
    pub arrow: LeaderArrow,
    pub text: FeatureText,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HatchLinePattern {
    pub color_code: i64,
    pub line_type_code: i64,
    pub line_width_code: i64,
    pub start: Point2,
    pub spacing: f64,
    pub angle_deg: f64,
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
    pub patterns: Vec<HatchLinePattern>,
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
    pub(crate) fn style(&self) -> Option<&CommonStyle> {
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

    pub(crate) fn requires_pre_sheet_order(&self) -> bool {
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
    pub(crate) fn new(
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
