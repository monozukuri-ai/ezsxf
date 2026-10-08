//! Thin Python boundary for the Rust SFC editor.

use crate::editor::{number, string, SfcDocument};
use crate::model::{FileFormat, Value};
use crate::writer::{write_bytes_atomic, SfcWriteOptions, WriteError};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};
use std::collections::BTreeMap;

fn error(error: WriteError) -> PyErr {
    PyValueError::new_err(error.to_string())
}
fn io_error(error: std::io::Error) -> PyErr {
    if error.kind() == std::io::ErrorKind::InvalidInput {
        PyValueError::new_err(error.to_string())
    } else {
        error.into()
    }
}
fn style(layer: i64, color: i64, line_type: i64, line_width: i64) -> Vec<Value> {
    vec![
        string(layer),
        string(color),
        string(line_type),
        string(line_width),
    ]
}
pub(crate) fn scalar(value: &Bound<'_, PyAny>) -> PyResult<Value> {
    if value.is_instance_of::<PyBool>() {
        return Err(PyTypeError::new_err("Boolean is not an editing parameter"));
    }
    if value.is_instance_of::<PyInt>() {
        Ok(Value::Integer(value.extract()?))
    } else if value.is_instance_of::<PyFloat>() {
        Ok(Value::Real(value.extract()?))
    } else if value.is_instance_of::<PyString>() {
        Ok(Value::String(value.extract()?))
    } else {
        Err(PyTypeError::new_err("Expected an integer, float or string"))
    }
}
pub(crate) fn real_number(value: &Bound<'_, PyAny>) -> PyResult<f64> {
    match scalar(value)? {
        Value::Integer(value) => Ok(value as f64),
        Value::Real(value) => Ok(value),
        _ => Err(PyTypeError::new_err("Expected an integer or float")),
    }
}
pub(crate) fn points(value: &Bound<'_, PyAny>) -> PyResult<Value> {
    if !value.is_instance_of::<PyList>() && !value.is_instance_of::<PyTuple>() {
        return Err(PyTypeError::new_err("points must be a list or tuple"));
    }
    value
        .iter()?
        .map(|point| {
            let point = point?;
            if !point.is_instance_of::<PyList>() && !point.is_instance_of::<PyTuple>() {
                return Err(PyTypeError::new_err("Each point must be a list or tuple"));
            }
            point
                .iter()?
                .map(|v| scalar(&v?))
                .collect::<PyResult<Vec<_>>>()
                .map(Value::List)
        })
        .collect::<PyResult<Vec<_>>>()
        .map(Value::List)
}

pub(crate) fn fields_from_dict(
    fields: &Bound<'_, PyDict>,
    skip_kind: bool,
) -> PyResult<BTreeMap<String, Value>> {
    let mut result = BTreeMap::new();
    for (key, value) in fields {
        let key = key.extract::<String>()?;
        if skip_kind && key == "kind" {
            continue;
        }
        let value = if matches!(key.as_str(), "points" | "vertices") {
            points(&value)?
        } else if matches!(key.as_str(), "start" | "end" | "center" | "anchor") {
            if !value.is_instance_of::<PyList>() && !value.is_instance_of::<PyTuple>() {
                return Err(PyTypeError::new_err(format!(
                    "{key} must be a coordinate pair"
                )));
            }
            Value::List(
                value
                    .iter()?
                    .map(|v| scalar(&v?))
                    .collect::<PyResult<Vec<_>>>()?,
            )
        } else {
            scalar(&value)?
        };
        result.insert(key, value);
    }
    Ok(result)
}

fn named_code(
    value: Option<&Bound<'_, PyAny>>,
    names: &[(&str, i64)],
    default: i64,
    field: &str,
) -> PyResult<i64> {
    let Some(value) = value else {
        return Ok(default);
    };
    match scalar(value)? {
        Value::Integer(code) => Ok(code),
        Value::String(name) => names
            .iter()
            .find(|(n, _)| name.trim().eq_ignore_ascii_case(n))
            .map(|(_, code)| *code)
            .ok_or_else(|| PyValueError::new_err(format!("Unknown {field}: {name}"))),
        _ => Err(PyTypeError::new_err(format!(
            "{field} must be a name or integer code"
        ))),
    }
}

#[pyclass(name = "SfcDocument", module = "ezsxf._core")]
struct PythonSfcDocument {
    document: SfcDocument,
}

#[pymethods]
#[allow(clippy::too_many_arguments)]
impl PythonSfcDocument {
    #[pyo3(signature = (*, unsupported="raise", report=false))]
    fn to_p21_bytes(&self, py: Python<'_>, unsupported: &str, report: bool) -> PyResult<PyObject> {
        if !matches!(unsupported, "raise" | "drop") {
            return Err(PyValueError::new_err("unsupported must be raise or drop"));
        }
        let (bytes, dropped) = crate::p21_writer::serialize_p21_report(
            self.document.snapshot(),
            unsupported == "drop",
        )
        .map_err(error)?;
        let bytes = PyBytes::new_bound(py, &bytes).unbind();
        if report {
            Ok((bytes, dropped).into_py(py))
        } else {
            Ok(bytes.into_py(py))
        }
    }
    fn save_p21(&self, path: &Bound<'_, PyAny>) -> PyResult<()> {
        let bytes = crate::p21_writer::serialize_p21(self.document.snapshot()).map_err(error)?;
        write_bytes_atomic(&crate::python_writer::fspath(path)?, &bytes).map_err(Into::into)
    }
    #[pyo3(signature = (destination, *, file_name=None))]
    fn save_p21_bundle<'py>(
        &self,
        py: Python<'py>,
        destination: &Bound<'_, PyAny>,
        file_name: Option<&str>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let report = self
            .document
            .save_p21_bundle(&crate::python_writer::fspath(destination)?, file_name)
            .map_err(io_error)?;
        let result = PyDict::new_bound(py);
        result.set_item("drawing", report.drawing)?;
        result.set_item("files", report.files)?;
        Ok(result)
    }
    #[pyo3(signature = (*, file_name=None))]
    fn to_p2z_bytes(&self, py: Python<'_>, file_name: Option<&str>) -> PyResult<Py<PyBytes>> {
        let bytes = self.document.to_p2z_bytes(file_name).map_err(io_error)?;
        Ok(PyBytes::new_bound(py, &bytes).unbind())
    }
    fn save_p2z(&self, path: &Bound<'_, PyAny>) -> PyResult<()> {
        self.document
            .save_p2z(&crate::python_writer::fspath(path)?)
            .map_err(io_error)
    }
    #[pyo3(signature = (elements, *, on_invalid="raise", into=None))]
    fn extend(
        &mut self,
        elements: &Bound<'_, PyAny>,
        on_invalid: &str,
        into: Option<i64>,
    ) -> PyResult<PyObject> {
        if !matches!(on_invalid, "raise" | "skip") {
            return Err(PyValueError::new_err("on_invalid must be raise or skip"));
        }
        let mut inputs = Vec::new();
        for (index, item) in elements.iter()?.enumerate() {
            let item = item.map_err(|e| {
                PyErr::from_type_bound(
                    e.get_type_bound(elements.py()),
                    format!("Element {index}: {e}"),
                )
            })?;
            let parsed = crate::python_bulk::element(&item, 0).map_err(|e| e.to_string());
            if on_invalid == "raise" {
                if let Err(reason) = &parsed {
                    return Err(PyValueError::new_err(format!("Element {index}: {reason}")));
                }
            }
            inputs.push(parsed);
        }
        let report = self
            .document
            .extend_structured(&inputs, on_invalid == "skip", into)
            .map_err(error)?;
        let py = elements.py();
        let ids: Vec<PyObject> = report
            .ids
            .into_iter()
            .map(|id| match id {
                None => py.None(),
                Some(crate::bulk_editor::BatchId::Single(id)) => id.into_py(py),
                Some(crate::bulk_editor::BatchId::Placements(ids)) => ids.into_py(py),
            })
            .collect();
        if on_invalid == "skip" {
            Ok((ids, report.rejected).into_py(py))
        } else {
            Ok(ids.into_py(py))
        }
    }
    fn validate_p21(&self) -> Vec<(i64, String)> {
        self.document.validate_p21()
    }
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        crate::python::output_to_python(py, self.document.snapshot())
    }
    #[pyo3(signature = (*, allow_external_references=false, literal_backslashes=false))]
    fn to_bytes(
        &self,
        py: Python<'_>,
        allow_external_references: bool,
        literal_backslashes: bool,
    ) -> PyResult<Py<PyBytes>> {
        let bytes = self
            .document
            .to_bytes(SfcWriteOptions {
                allow_external_references,
                literal_backslashes,
            })
            .map_err(error)?;
        Ok(PyBytes::new_bound(py, &bytes).unbind())
    }
    #[pyo3(signature = (path, *, allow_external_references=false, literal_backslashes=false))]
    fn save(
        &self,
        path: &Bound<'_, PyAny>,
        allow_external_references: bool,
        literal_backslashes: bool,
    ) -> PyResult<()> {
        let bytes = self
            .document
            .to_bytes(SfcWriteOptions {
                allow_external_references,
                literal_backslashes,
            })
            .map_err(error)?;
        write_bytes_atomic(&crate::python_writer::fspath(path)?, &bytes).map_err(Into::into)
    }
    #[pyo3(signature = (name, *, visible=true))]
    fn add_layer(&mut self, name: &str, visible: bool) -> PyResult<i64> {
        self.document.add_layer(name, visible).map_err(error)
    }
    fn rename_layer(&mut self, code: i64, name: &str) -> PyResult<()> {
        self.document.rename_layer(code, name).map_err(error)
    }
    fn add_font(&mut self, name: &str) -> PyResult<i64> {
        self.document.add_font(name).map_err(error)
    }
    fn add_color(&mut self, color: &Bound<'_, PyAny>) -> PyResult<i64> {
        if color.is_instance_of::<PyString>() {
            return self
                .document
                .add_named_color(&color.extract::<String>()?)
                .map_err(error);
        }
        if !color.is_instance_of::<PyTuple>() && !color.is_instance_of::<PyList>() {
            return Err(PyTypeError::new_err(
                "color must be a predefined name or an RGB tuple/list",
            ));
        }
        let rgb = color
            .iter()?
            .map(|v| match scalar(&v?)? {
                Value::Integer(value) => Ok(value),
                _ => Err(PyTypeError::new_err("RGB components must be integers")),
            })
            .collect::<PyResult<Vec<_>>>()?;
        let rgb: [i64; 3] = rgb
            .try_into()
            .map_err(|_| PyValueError::new_err("RGB needs exactly three components"))?;
        self.document.add_rgb_color(rgb).map_err(error)
    }
    #[pyo3(signature = (name, *, pattern=None))]
    fn add_line_type(&mut self, name: &str, pattern: Option<&Bound<'_, PyAny>>) -> PyResult<i64> {
        let pattern = pattern
            .map(|value| {
                if !value.is_instance_of::<PyList>() && !value.is_instance_of::<PyTuple>() {
                    return Err(PyTypeError::new_err("pattern must be a list or tuple"));
                }
                value
                    .iter()?
                    .map(|v| real_number(&v?))
                    .collect::<PyResult<Vec<_>>>()
            })
            .transpose()?;
        self.document
            .add_line_type(name, pattern.as_deref())
            .map_err(error)
    }
    fn add_line_width(&mut self, width_mm: &Bound<'_, PyAny>) -> PyResult<i64> {
        self.document
            .add_line_width(real_number(width_mm)?)
            .map_err(error)
    }
    #[pyo3(signature = (destination, *, file_name=None, literal_backslashes=false))]
    fn save_bundle<'py>(
        &self,
        py: Python<'py>,
        destination: &Bound<'_, PyAny>,
        file_name: Option<&str>,
        literal_backslashes: bool,
    ) -> PyResult<Bound<'py, PyDict>> {
        let report = self.document.save_bundle_with_options(
            &crate::python_writer::fspath(destination)?,
            file_name,
            SfcWriteOptions {
                allow_external_references: true,
                literal_backslashes,
            },
        )?;
        let result = PyDict::new_bound(py);
        result.set_item("drawing", report.drawing)?;
        result.set_item("files", report.files)?;
        Ok(result)
    }
    fn saf_bytes(&self, py: Python<'_>) -> PyResult<Option<Py<PyBytes>>> {
        Ok(self
            .document
            .saf_bytes()
            .map_err(error)?
            .map(|b| PyBytes::new_bound(py, &b).unbind()))
    }
    #[pyo3(signature = (entity_id, name, value, *, attribute_type="STR", unit=None, group=None, set_name="ezsxf attributes", set_version="1.0", designed_by="ezsxf", figure_name="figure"))]
    fn set_attribute(
        &mut self,
        entity_id: i64,
        name: &str,
        value: &str,
        attribute_type: &str,
        unit: Option<String>,
        group: Option<Vec<String>>,
        set_name: &str,
        set_version: &str,
        designed_by: &str,
        figure_name: &str,
    ) -> PyResult<String> {
        self.document
            .set_attribute(
                entity_id,
                figure_name,
                (set_name, set_version, designed_by),
                crate::saf::SafAttribute {
                    name: name.into(),
                    value: value.into(),
                    attribute_type: Some(attribute_type.into()),
                    unit,
                    group: group.unwrap_or_default(),
                },
            )
            .map_err(error)
    }
    fn get_attributes<'py>(&self, py: Python<'py>, entity_id: i64) -> PyResult<Bound<'py, PyList>> {
        let result = PyList::empty_bound(py);
        for (set, a) in self.document.attributes(entity_id).map_err(error)? {
            let row = PyDict::new_bound(py);
            row.set_item("set_name", set.name)?;
            row.set_item("set_version", set.version)?;
            row.set_item("designed_by", set.designed_by)?;
            row.set_item("name", a.name)?;
            row.set_item("value", a.value)?;
            row.set_item("attribute_type", a.attribute_type)?;
            row.set_item("unit", a.unit)?;
            row.set_item("group", a.group)?;
            result.append(row)?;
        }
        Ok(result)
    }
    #[pyo3(signature = (entity_id, name, *, group=None, set_name="ezsxf attributes", set_version="1.0", designed_by="ezsxf"))]
    fn remove_attribute(
        &mut self,
        entity_id: i64,
        name: &str,
        group: Option<Vec<String>>,
        set_name: &str,
        set_version: &str,
        designed_by: &str,
    ) -> PyResult<()> {
        self.document
            .remove_attribute(
                entity_id,
                (set_name, set_version, designed_by),
                name,
                &group.unwrap_or_default(),
            )
            .map_err(error)
    }
    fn remove_attachment(&mut self, entity_id: i64) -> PyResult<()> {
        self.document.remove_attachment(entity_id).map_err(error)
    }
    #[pyo3(signature = (entity_id, figure_name, name, value, *, attribute_type="STR", unit=None))]
    fn set_single_attribute(
        &mut self,
        entity_id: i64,
        figure_name: &str,
        name: &str,
        value: &str,
        attribute_type: &str,
        unit: Option<String>,
    ) -> PyResult<String> {
        self.document
            .set_single_attribute(
                entity_id,
                figure_name,
                crate::saf::SafAttribute {
                    name: name.into(),
                    value: value.into(),
                    attribute_type: Some(attribute_type.into()),
                    unit,
                    group: vec![],
                },
            )
            .map_err(error)
    }
    #[pyo3(signature = (entity_id, name, *, attribute_type=None, unit=None))]
    fn set_text_attribute(
        &mut self,
        entity_id: i64,
        name: &str,
        attribute_type: Option<&str>,
        unit: Option<&str>,
    ) -> PyResult<String> {
        self.document
            .set_text_attribute(entity_id, name, attribute_type, unit)
            .map_err(error)
    }
    #[pyo3(signature = (source, *, file_name=None))]
    fn add_dependency(
        &mut self,
        source: &Bound<'_, PyAny>,
        file_name: Option<&str>,
    ) -> PyResult<String> {
        self.document
            .add_dependency(&crate::python_writer::fspath(source)?, file_name)
            .map_err(error)
    }
    #[pyo3(signature = (image, anchor, width_mm, height_mm, *, angle=0.0, layer=1, file_name=None))]
    fn add_image(
        &mut self,
        image: &Bound<'_, PyAny>,
        anchor: (f64, f64),
        width_mm: f64,
        height_mm: f64,
        angle: f64,
        layer: i64,
        file_name: Option<&str>,
    ) -> PyResult<i64> {
        self.document
            .add_image(
                &crate::python_writer::fspath(image)?,
                file_name,
                anchor,
                width_mm,
                height_mm,
                angle,
                layer,
            )
            .map_err(error)
    }
    #[pyo3(signature = (entity_id, anchor, width_mm, height_mm, *, angle=0.0, image=None, file_name=None))]
    fn update_image(
        &mut self,
        entity_id: i64,
        anchor: (f64, f64),
        width_mm: f64,
        height_mm: f64,
        angle: f64,
        image: Option<&Bound<'_, PyAny>>,
        file_name: Option<&str>,
    ) -> PyResult<()> {
        let path = image.map(crate::python_writer::fspath).transpose()?;
        self.document
            .update_image(
                entity_id,
                path.as_deref(),
                file_name,
                anchor,
                width_mm,
                height_mm,
                angle,
            )
            .map_err(error)
    }
    #[pyo3(signature = (start, end, *, layer=1, color=1, line_type=1, line_width=1))]
    fn add_line(
        &mut self,
        start: (f64, f64),
        end: (f64, f64),
        layer: i64,
        color: i64,
        line_type: i64,
        line_width: i64,
    ) -> PyResult<i64> {
        let mut p = style(layer, color, line_type, line_width);
        p.extend([
            number(start.0),
            number(start.1),
            number(end.0),
            number(end.1),
        ]);
        self.document.add_element("line_feature", p).map_err(error)
    }
    #[pyo3(signature = (center, radius, *, layer=1, color=1, line_type=1, line_width=1))]
    fn add_circle(
        &mut self,
        center: (f64, f64),
        radius: f64,
        layer: i64,
        color: i64,
        line_type: i64,
        line_width: i64,
    ) -> PyResult<i64> {
        let mut p = style(layer, color, line_type, line_width);
        p.extend([number(center.0), number(center.1), number(radius)]);
        self.document
            .add_element("circle_feature", p)
            .map_err(error)
    }
    #[pyo3(signature = (center, radius, start_angle, end_angle, *, direction=0, layer=1, color=1, line_type=1, line_width=1))]
    fn add_arc(
        &mut self,
        center: (f64, f64),
        radius: f64,
        start_angle: f64,
        end_angle: f64,
        direction: i64,
        layer: i64,
        color: i64,
        line_type: i64,
        line_width: i64,
    ) -> PyResult<i64> {
        let mut p = style(layer, color, line_type, line_width);
        p.extend([
            number(center.0),
            number(center.1),
            number(radius),
            string(direction),
            number(start_angle),
            number(end_angle),
        ]);
        self.document.add_element("arc_feature", p).map_err(error)
    }
    #[pyo3(signature = (points, *, layer=1, color=1, line_type=1, line_width=1))]
    fn add_polyline(
        &mut self,
        points: Vec<(f64, f64)>,
        layer: i64,
        color: i64,
        line_type: i64,
        line_width: i64,
    ) -> PyResult<i64> {
        self.document
            .add_polyline([layer, color, line_type, line_width], &points)
            .map_err(error)
    }
    #[pyo3(signature = (text, anchor, *, height=3.5, width=None, spacing=0.0, angle=0.0, slant=0.0, base_point=1, direction=1, layer=1, color=1, font=1))]
    fn add_text(
        &mut self,
        text: &str,
        anchor: (f64, f64),
        height: f64,
        width: Option<f64>,
        spacing: f64,
        angle: f64,
        slant: f64,
        base_point: i64,
        direction: i64,
        layer: i64,
        color: i64,
        font: i64,
    ) -> PyResult<i64> {
        let width =
            match width {
                Some(value) => value,
                None if direction == 1 => {
                    crate::authoring::estimate_text_width(text, height, spacing).map_err(error)?
                }
                None => return Err(PyValueError::new_err(
                    "Automatic text width supports horizontal text; supply width for vertical text",
                )),
            };
        self.document
            .add_element(
                "text_string_feature",
                vec![
                    string(layer),
                    string(color),
                    string(font),
                    string(text),
                    number(anchor.0),
                    number(anchor.1),
                    number(height),
                    number(width),
                    number(spacing),
                    number(angle),
                    number(slant),
                    string(base_point),
                    string(direction),
                ],
            )
            .map_err(error)
    }
    #[pyo3(signature = (entity_id, **changes))]
    fn update_element(
        &mut self,
        entity_id: i64,
        changes: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        let mut values = BTreeMap::new();
        if let Some(changes) = changes {
            for (key, value) in changes {
                let key = key.extract::<String>()?;
                let value = if matches!(key.as_str(), "points" | "vertices") {
                    points(&value)?
                } else {
                    scalar(&value)?
                };
                values.insert(key, value);
            }
        }
        self.document
            .update_element(entity_id, &values)
            .map_err(error)
    }
    fn remove_element(&mut self, entity_id: i64) -> PyResult<()> {
        self.document.remove_element(entity_id).map_err(error)
    }
    #[pyo3(signature = (kind, **fields))]
    fn add_feature(&mut self, kind: &str, fields: Option<&Bound<'_, PyDict>>) -> PyResult<i64> {
        let changes = fields
            .map(|fields| fields_from_dict(fields, false))
            .transpose()?
            .unwrap_or_default();
        self.document.add_feature(kind, &changes).map_err(error)
    }
    #[pyo3(signature = (name, entity_ids, placements))]
    fn create_part(
        &mut self,
        name: &str,
        entity_ids: Vec<i64>,
        placements: &Bound<'_, PyAny>,
    ) -> PyResult<Vec<i64>> {
        let mut inputs = Vec::new();
        for item in placements.iter()? {
            let item = item?;
            let item = item.downcast::<PyDict>()?;
            for key in item.keys() {
                let key = key.extract::<String>()?;
                if !matches!(key.as_str(), "position" | "angle" | "scale" | "layer") {
                    return Err(PyValueError::new_err(format!(
                        "Unknown placement field {key}"
                    )));
                }
            }
            let position = item.get_item("position")?.ok_or_else(|| {
                PyValueError::new_err("Each placement needs an explicit position")
            })?;
            let pair = |value: &Bound<'_, PyAny>| -> PyResult<Vec<Value>> {
                if !value.is_instance_of::<PyList>() && !value.is_instance_of::<PyTuple>() {
                    return Err(PyTypeError::new_err("Expected a coordinate pair"));
                }
                let values = value
                    .iter()?
                    .map(|v| real_number(&v?).map(number))
                    .collect::<PyResult<Vec<_>>>()?;
                if values.len() != 2 {
                    return Err(PyValueError::new_err("Expected two coordinates"));
                }
                Ok(values)
            };
            let position = pair(&position)?;
            let scale = item
                .get_item("scale")?
                .map(|v| pair(&v))
                .transpose()?
                .unwrap_or_else(|| vec![number(1.0), number(1.0)]);
            let layer = match item.get_item("layer")?.map(|v| scalar(&v)).transpose()? {
                None => string(1),
                Some(Value::Integer(value)) => string(value),
                _ => return Err(PyTypeError::new_err("layer must be an integer code")),
            };
            let angle = item
                .get_item("angle")?
                .map(|v| real_number(&v))
                .transpose()?
                .unwrap_or(0.0);
            inputs.push(vec![
                layer,
                position[0].clone(),
                position[1].clone(),
                number(angle),
                scale[0].clone(),
                scale[1].clone(),
            ]);
        }
        self.document
            .create_part(name, &entity_ids, &inputs)
            .map_err(error)
    }
    #[pyo3(signature = (name, entity_ids, *, kind=3, position=(0.0,0.0), angle=0.0, scale=(1.0,1.0), layer=0))]
    fn group_elements(
        &mut self,
        name: &str,
        entity_ids: Vec<i64>,
        kind: i64,
        position: (f64, f64),
        angle: f64,
        scale: (f64, f64),
        layer: i64,
    ) -> PyResult<i64> {
        self.document
            .group_elements(
                name,
                &entity_ids,
                kind,
                &[
                    string(layer),
                    number(position.0),
                    number(position.1),
                    number(angle),
                    number(scale.0),
                    number(scale.1),
                ],
            )
            .map_err(error)
    }
    fn ungroup(&mut self, placement_id: i64) -> PyResult<()> {
        self.document.ungroup(placement_id).map_err(error)
    }
    fn add_to_group(&mut self, placement_id: i64, entity_ids: Vec<i64>) -> PyResult<()> {
        self.document
            .add_to_group(placement_id, &entity_ids)
            .map_err(error)
    }
    #[pyo3(signature = (placement_id, *, position=(0.0,0.0), angle=0.0, scale=(1.0,1.0), layer=1))]
    fn place_part(
        &mut self,
        placement_id: i64,
        position: (f64, f64),
        angle: f64,
        scale: (f64, f64),
        layer: i64,
    ) -> PyResult<i64> {
        self.document
            .place_part(
                placement_id,
                &[
                    string(layer),
                    number(position.0),
                    number(position.1),
                    number(angle),
                    number(scale.0),
                    number(scale.1),
                ],
            )
            .map_err(error)
    }
    fn rename_group(&mut self, placement_id: i64, name: &str) -> PyResult<()> {
        self.document
            .rename_group(placement_id, name)
            .map_err(error)
    }
    #[pyo3(signature = (entity_ids, *, visible=false, color=1, line_type=1, line_width=1))]
    fn add_composite_curve(
        &mut self,
        entity_ids: Vec<i64>,
        visible: bool,
        color: i64,
        line_type: i64,
        line_width: i64,
    ) -> PyResult<i64> {
        self.document
            .add_composite_curve(&entity_ids, [color, line_type, line_width], visible)
            .map_err(error)
    }
    #[pyo3(signature = (outer, *, holes=None, layer=1, color=1))]
    fn add_fill(
        &mut self,
        outer: i64,
        holes: Option<Vec<i64>>,
        layer: i64,
        color: i64,
    ) -> PyResult<i64> {
        self.document
            .add_fill(outer, &holes.unwrap_or_default(), layer, color)
            .map_err(error)
    }
    #[pyo3(signature = (outer, patterns, *, holes=None, layer=1))]
    fn add_hatch(
        &mut self,
        outer: i64,
        patterns: &Bound<'_, PyAny>,
        holes: Option<Vec<i64>>,
        layer: i64,
    ) -> PyResult<i64> {
        let rows = points(patterns)?;
        let Value::List(rows) = rows else {
            unreachable!()
        };
        let rows = rows
            .into_iter()
            .map(|r| {
                let Value::List(v) = r else { unreachable!() };
                v
            })
            .collect::<Vec<_>>();
        self.document
            .add_hatch(outer, &holes.unwrap_or_default(), layer, &rows)
            .map_err(error)
    }
    #[pyo3(signature = (entity_id, outer, *, holes=None))]
    fn update_hatch_boundaries(
        &mut self,
        entity_id: i64,
        outer: i64,
        holes: Option<Vec<i64>>,
    ) -> PyResult<()> {
        self.document
            .update_hatch_boundaries(entity_id, outer, &holes.unwrap_or_default())
            .map_err(error)
    }
    fn release_composite_curve(&mut self, entity_id: i64) -> PyResult<()> {
        self.document
            .release_composite_curve(entity_id)
            .map_err(error)
    }
    fn update_hatch_patterns(
        &mut self,
        entity_id: i64,
        patterns: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let Value::List(rows) = points(patterns)? else {
            unreachable!()
        };
        let rows = rows
            .into_iter()
            .map(|r| {
                let Value::List(v) = r else { unreachable!() };
                v
            })
            .collect::<Vec<_>>();
        self.document
            .update_hatch_patterns(entity_id, &rows)
            .map_err(error)
    }
}

#[pyfunction]
#[allow(clippy::too_many_arguments)] // Stable keyword-only Python constructor.
#[pyo3(signature = (file_name="drawing.sfc", *, name="drawing", paper=None, orientation=None, width_mm=None, height_mm=None, timestamp=None, target="sfc"))]
fn new_sfc(
    py: Python<'_>,
    file_name: &str,
    name: &str,
    paper: Option<&Bound<'_, PyAny>>,
    orientation: Option<&Bound<'_, PyAny>>,
    width_mm: Option<&Bound<'_, PyAny>>,
    height_mm: Option<&Bound<'_, PyAny>>,
    timestamp: Option<String>,
    target: &str,
) -> PyResult<PythonSfcDocument> {
    if !matches!(target, "sfc" | "p21") {
        return Err(PyValueError::new_err("target must be sfc or p21"));
    }
    let timestamp = match timestamp {
        Some(s) => s,
        None => py
            .import_bound("datetime")?
            .getattr("datetime")?
            .call_method0("now")?
            .call_method0("isoformat")?
            .extract()?,
    };
    let mut result = PythonSfcDocument {
        document: SfcDocument::new_with_paper(
            file_name,
            name,
            named_code(
                paper,
                &[
                    ("A0", 0),
                    ("A1", 1),
                    ("A2", 2),
                    ("A3", 3),
                    ("A4", 4),
                    ("FREE", 9),
                ],
                9,
                "paper",
            )?,
            named_code(
                orientation,
                &[("portrait", 0), ("landscape", 1)],
                1,
                "orientation",
            )?,
            (
                width_mm.map(real_number).transpose()?,
                height_mm.map(real_number).transpose()?,
            ),
            &timestamp,
        )
        .map_err(error)?,
    };
    result.document.target_p21 = target == "p21";
    Ok(result)
}
#[pyfunction]
fn edit_sfc(parsed: &Bound<'_, PyDict>) -> PyResult<PythonSfcDocument> {
    let bytes = crate::python_writer::encode_python(parsed, true, false)?;
    let output = crate::parser::parse_from_bytes(FileFormat::Sfc, &bytes, true)
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(PythonSfcDocument {
        document: SfcDocument::from_output(output).map_err(error)?,
    })
}
#[pyfunction]
fn edit_sfc_bundle(source: &Bound<'_, PyAny>) -> PyResult<PythonSfcDocument> {
    Ok(PythonSfcDocument {
        document: SfcDocument::from_bundle(&crate::python_writer::fspath(source)?)
            .map_err(error)?,
    })
}
#[pyfunction]
fn validate_saf(data: &[u8]) -> PyResult<()> {
    crate::saf::SafDocument::parse(data).map_err(error)?;
    Ok(())
}
#[pyfunction(name = "estimate_text_width")]
#[pyo3(signature = (text, height=3.5, spacing=0.0))]
fn python_estimate_text_width(text: &str, height: f64, spacing: f64) -> PyResult<f64> {
    crate::authoring::estimate_text_width(text, height, spacing).map_err(error)
}
pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PythonSfcDocument>()?;
    module.add_function(wrap_pyfunction!(new_sfc, module)?)?;
    module.add_function(wrap_pyfunction!(edit_sfc, module)?)?;
    module.add_function(wrap_pyfunction!(edit_sfc_bundle, module)?)?;
    module.add_function(wrap_pyfunction!(validate_saf, module)?)?;
    module.add_function(wrap_pyfunction!(python_estimate_text_width, module)?)?;
    Ok(())
}
