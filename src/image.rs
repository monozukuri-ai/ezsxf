//! Raster placement per common attribute set §3-4: clockwise closed rectangle,
//! lower-left origin. Bytes are copied, never transcoded or pixel-decoded.

use crate::editor::SfcDocument;
use crate::saf::SafAttribute;
use crate::writer::WriteError;
use std::path::Path;

fn error(value: impl std::fmt::Display) -> WriteError {
    WriteError(value.to_string())
}

fn rectangle(
    anchor: (f64, f64),
    width: f64,
    height: f64,
    angle: f64,
) -> Result<Vec<(f64, f64)>, WriteError> {
    if ![anchor.0, anchor.1, width, height, angle]
        .iter()
        .all(|v| v.is_finite())
        || width <= 0.
        || height <= 0.
    {
        return Err(error(
            "Image placement requires finite coordinates, angle and positive dimensions",
        ));
    }
    let radians = angle.to_radians();
    let (s, c) = radians.sin_cos();
    let point = |x: f64, y: f64| {
        (
            ((anchor.0 + x * c - y * s) * 1_000_000.).round() / 1_000_000.,
            ((anchor.1 + x * s + y * c) * 1_000_000.).round() / 1_000_000.,
        )
    };
    let points = vec![
        point(0., 0.),
        point(0., height),
        point(width, height),
        point(width, 0.),
        point(0., 0.),
    ];
    if points.windows(2).any(|p| p[0] == p[1]) {
        return Err(error(
            "Image rectangle collapses at SFC's six-decimal coordinate precision",
        ));
    }
    Ok(points)
}

pub(crate) fn validate_image(name: &str, bytes: &[u8]) -> Result<(), WriteError> {
    let extension = Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "tif" | "tiff" => validate_tiff(bytes),
        "jpg" | "jpeg" => validate_jpeg(bytes),
        _ => Err(error(
            "SXF images must be TIFF or JPEG; no image conversion is performed",
        )),
    }
}

fn validate_tiff(bytes: &[u8]) -> Result<(), WriteError> {
    let little = match bytes.get(..4) {
        Some(b"II\x2a\x00") => true,
        Some(b"MM\x00\x2a") => false,
        _ => return Err(error("Expected a classic TIFF header")),
    };
    let short = |offset: usize| -> Result<u16, WriteError> {
        let array: [u8; 2] = bytes
            .get(offset..offset + 2)
            .ok_or_else(|| error("Truncated TIFF"))?
            .try_into()
            .unwrap();
        Ok(if little {
            u16::from_le_bytes(array)
        } else {
            u16::from_be_bytes(array)
        })
    };
    let long = |offset: usize| -> Result<u32, WriteError> {
        let array: [u8; 4] = bytes
            .get(offset..offset + 4)
            .ok_or_else(|| error("Truncated TIFF"))?
            .try_into()
            .unwrap();
        Ok(if little {
            u32::from_le_bytes(array)
        } else {
            u32::from_be_bytes(array)
        })
    };
    let offset = long(4)? as usize;
    let count = short(offset)? as usize;
    let mut tags = std::collections::BTreeMap::new();
    for i in 0..count {
        let item = offset + 2 + i * 12;
        let tag = short(item)?;
        let kind = short(item + 2)?;
        let count = long(item + 4)? as usize;
        if !matches!(
            tag,
            256 | 257 | 258 | 259 | 262 | 273 | 274 | 277 | 278 | 279 | 322 | 323 | 324 | 325
        ) {
            continue;
        }
        if !matches!(kind, 3 | 4) || count == 0 || count > bytes.len() / 2 {
            return Err(error("Unsupported TIFF metadata"));
        }
        let size = if kind == 3 { 2 } else { 4 };
        let value_offset = if count * size <= 4 {
            item + 8
        } else {
            long(item + 8)? as usize
        };
        let values = (0..count)
            .map(|n| {
                if kind == 3 {
                    short(value_offset + n * size).map(u32::from)
                } else {
                    long(value_offset + n * size)
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        if tags.insert(tag, values).is_some() {
            return Err(error("Duplicate TIFF metadata tag"));
        }
    }
    let first = |tag: u16, default: u32| {
        tags.get(&tag)
            .and_then(|v| v.first())
            .copied()
            .unwrap_or(default)
    };
    if first(256, 0) == 0
        || first(257, 0) == 0
        || first(256, 0) > 13_000
        || first(257, 0) > 13_000
        || first(258, 1) != 1
        || first(259, 1) != 4
        || first(277, 1) != 1
        || first(262, 2) > 1
        || first(274, 1) != 1
        || first(278, 0) == 0
        || [322, 323, 324, 325]
            .iter()
            .any(|tag| tags.contains_key(tag))
        || long(offset + 2 + count * 12)? != 0
    {
        return Err(error("SXF TIFF requires one page, G4 strips, monochrome 1-bit pixels, normal orientation and dimensions <=13000"));
    }
    let offsets = tags
        .get(&273)
        .ok_or_else(|| error("Missing TIFF strip offsets"))?;
    let lengths = tags
        .get(&279)
        .ok_or_else(|| error("Missing TIFF strip lengths"))?;
    let expected = first(257, 0).div_ceil(first(278, 0)) as usize;
    if offsets.len() != lengths.len()
        || offsets.len() != expected
        || offsets.iter().zip(lengths).any(|(o, l)| {
            *l == 0
                || (*o as usize)
                    .checked_add(*l as usize)
                    .is_none_or(|end| end > bytes.len())
        })
    {
        return Err(error("Invalid TIFF strip bounds"));
    }
    Ok(())
}

fn validate_jpeg(bytes: &[u8]) -> Result<(), WriteError> {
    if !bytes.starts_with(&[0xff, 0xd8]) || !bytes.ends_with(&[0xff, 0xd9]) {
        return Err(error("Expected complete JPEG SOI/EOI markers"));
    }
    let mut offset = 2;
    let mut dimensions = false;
    while offset + 4 <= bytes.len() {
        if bytes[offset] != 0xff {
            return Err(error("Invalid JPEG marker"));
        }
        while bytes.get(offset) == Some(&0xff) {
            offset += 1;
        }
        let marker = *bytes.get(offset).ok_or_else(|| error("Truncated JPEG"))?;
        offset += 1;
        let header = bytes
            .get(offset..offset + 2)
            .ok_or_else(|| error("Truncated JPEG segment"))?;
        let length = u16::from_be_bytes(header.try_into().unwrap()) as usize;
        if length < 2 || offset + length > bytes.len() {
            return Err(error("Invalid JPEG segment bounds"));
        }
        if marker == 0xda {
            let scan = &bytes[offset..offset + length];
            if !dimensions
                || length < 6
                || !matches!(scan[2], 1 | 3)
                || length != 6 + 2 * scan[2] as usize
                || offset + length + 2 >= bytes.len()
            {
                return Err(error("Invalid JPEG scan header or missing scan data"));
            }
            return Ok(());
        }
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            let frame = &bytes[offset..offset + length];
            if length < 8
                || frame[2] != 8
                || frame[3..5] == [0, 0]
                || frame[5..7] == [0, 0]
                || !matches!(frame[7], 1 | 3)
                || length != 8 + 3 * frame[7] as usize
            {
                return Err(error("Unsupported JPEG frame"));
            }
            dimensions = true;
        }
        offset += length;
    }
    Err(error("JPEG has no supported frame/scan"))
}

impl SfcDocument {
    pub(crate) fn validate_image_links(
        &self,
        output: &crate::model::ParseOutput,
    ) -> Result<(), WriteError> {
        let model = output.document.sfc_model.as_ref().unwrap();
        let check = |attachment: &crate::model::SfcAttributeAttachment,
                     name: &str|
         -> Result<(), WriteError> {
            if attachment.component_ids.len() != 1 {
                return Err(error("Image attribute requires one polyline rectangle"));
            }
            let feature = output
                .document
                .typed_features
                .iter()
                .find(|f| f.id == attachment.component_ids[0])
                .ok_or_else(|| error("Missing image polyline"))?;
            let crate::model::TypedFeature::Polyline(polyline) = &feature.feature else {
                return Err(error("Image attribute requires a polyline rectangle"));
            };
            validate_rectangle(&polyline.points)?;
            crate::bundle::portable_name(name).map_err(error)
        };
        // SXF 3.1 common images use ATRU; legacy 3.0 bundles may use SAF/ATRF.
        for attachment in &model.attribute_attachments {
            if let crate::model::SfcAttributeMechanism::SingleAttribute {
                attribute_name: Some(name),
                attribute_value: Some(value),
                attribute_type,
                unit,
                ..
            } = &attachment.mechanism
            {
                if name == "画像" {
                    if attribute_type.as_deref().is_some_and(|v| v != "STR") || unit.is_some() {
                        return Err(error("Image ATRU requires STR type and no unit"));
                    }
                    check(attachment, value)?;
                }
            }
        }
        let Some(saf) = &self.saf else {
            return Ok(());
        };
        for figure in &saf.figures {
            for (set_id, attrs) in &figure.sets {
                for attribute in attrs.iter().filter(|a| a.name == "画像") {
                    let set = saf
                        .sets
                        .iter()
                        .find(|s| &s.id == set_id)
                        .ok_or_else(|| error("Unknown image attribute set"))?;
                    if set.name != "フィーチャ定義属性セット"
                        || !matches!(set.version.as_str(), "0" | "1.0")
                        || set.designed_by != "SCADEC"
                        || !attribute.group.is_empty()
                        || attribute
                            .attribute_type
                            .as_deref()
                            .is_some_and(|v| v != "STR")
                    {
                        return Err(error("Invalid image attribute set/type/group"));
                    }
                    let attachment = model.attribute_attachments.iter().find(|a| matches!(&a.mechanism,crate::model::SfcAttributeMechanism::AttributeFile {figure_id,..} if figure_id == &figure.id)).ok_or_else(|| error("Missing image ATRF"))?;
                    check(attachment, &attribute.value)?;
                }
            }
        }
        Ok(())
    }
    fn image_dependency(
        &mut self,
        source: &Path,
        file_name: Option<&str>,
    ) -> Result<String, WriteError> {
        let name = self.add_dependency(source, file_name)?;
        validate_image(&name, &self.dependencies[&name])?;
        Ok(name)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn add_image(
        &mut self,
        source: &Path,
        file_name: Option<&str>,
        anchor: (f64, f64),
        width: f64,
        height: f64,
        angle: f64,
        layer: i64,
    ) -> Result<i64, WriteError> {
        let points = rectangle(anchor, width, height, angle)?;
        let mut next = self.clone();
        let name = next.image_dependency(source, file_name)?;
        let id = next.add_polyline([layer, 1, 1, 1], &points)?;
        next.set_single_attribute(
            id,
            "image",
            SafAttribute {
                name: "画像".into(),
                value: name,
                attribute_type: Some("STR".into()),
                unit: None,
                group: vec![],
            },
        )?;
        *self = next;
        Ok(id)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn update_image(
        &mut self,
        id: i64,
        source: Option<&Path>,
        file_name: Option<&str>,
        anchor: (f64, f64),
        width: f64,
        height: f64,
        angle: f64,
    ) -> Result<(), WriteError> {
        self.editable_index(id)?;
        let mechanism = self
            .attachment(id)
            .ok_or_else(|| error("Element has no image attribute"))?
            .mechanism
            .clone();
        let legacy = match &mechanism {
            crate::model::SfcAttributeMechanism::SingleAttribute {
                attribute_name: Some(name),
                attribute_value: Some(_),
                ..
            } if name == "画像" => None,
            crate::model::SfcAttributeMechanism::AttributeFile { .. } => {
                let images: Vec<_> = self
                    .attributes(id)?
                    .into_iter()
                    .filter(|(s, a)| s.name == "フィーチャ定義属性セット" && a.name == "画像")
                    .collect();
                if images.len() != 1 {
                    return Err(error(
                        "Element does not have one unambiguous SAF image attribute",
                    ));
                }
                images.into_iter().next()
            }
            _ => return Err(error("Element does not have an image attribute")),
        };
        let points = rectangle(anchor, width, height, angle)?;
        let mut next = self.clone();
        let changes = [(
            "points".into(),
            crate::model::Value::List(
                points
                    .into_iter()
                    .map(|(x, y)| {
                        crate::model::Value::List(vec![
                            crate::model::Value::Real(x),
                            crate::model::Value::Real(y),
                        ])
                    })
                    .collect(),
            ),
        )]
        .into_iter()
        .collect();
        next.update_element(id, &changes)?;
        if let Some(source) = source {
            let name = next.image_dependency(source, file_name)?;
            if let Some((set, mut attribute)) = legacy {
                attribute.value = name;
                next.set_attribute(
                    id,
                    "image",
                    (&set.name, &set.version, &set.designed_by),
                    attribute,
                )?;
            } else if let crate::model::SfcAttributeMechanism::SingleAttribute {
                figure_name: Some(figure_name),
                ..
            } = mechanism
            {
                next.set_single_attribute(
                    id,
                    &figure_name,
                    SafAttribute {
                        name: "画像".into(),
                        value: name,
                        attribute_type: Some("STR".into()),
                        unit: None,
                        group: vec![],
                    },
                )?;
            } else {
                return Err(error("Image ATRU is missing its figure name"));
            }
        } else if file_name.is_some() {
            return Err(error("file_name requires a new image source"));
        }
        *self = next;
        Ok(())
    }
}

fn validate_rectangle(points: &[crate::model::Point2]) -> Result<(), WriteError> {
    if points.len() != 5 || points[0] != points[4] {
        return Err(error(
            "Image requires a closed five-point clockwise rectangle",
        ));
    }
    let u = (points[1].x - points[0].x, points[1].y - points[0].y);
    let v = (points[3].x - points[0].x, points[3].y - points[0].y);
    let lengths = u.0.hypot(u.1) + v.0.hypot(v.1);
    if u.0 * v.1 - u.1 * v.0 >= 0.
        || (u.0 * v.0 + u.1 * v.1).abs() > 0.000002 * lengths
        || (points[2].x - (points[0].x + u.0 + v.0)).abs() > 0.000003
        || (points[2].y - (points[0].y + u.1 + v.1)).abs() > 0.000003
    {
        return Err(error("Image polyline must be a clockwise rectangle"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raster_rectangle_is_clockwise_and_closed() {
        assert_eq!(
            rectangle((80., 40.), 20., 10., 0.).unwrap(),
            vec![(80., 40.), (80., 50.), (100., 50.), (100., 40.), (80., 40.)]
        );
        assert!(rectangle((0., 0.), 0., 1., 0.).is_err());
        assert!(rectangle((0., 0.), 1., 1., f64::NAN).is_err());
        assert!(validate_image("bad.tif", b"opaque").is_err());
        assert!(validate_image("bad.bmp", b"BM").is_err());
    }
}
