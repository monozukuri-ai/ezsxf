//! Conversion and consistency checks for the existing parse-result dictionary.

use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyFloat, PyInt, PyList, PyString};

use crate::model::*;
use crate::python::output_to_python;
use crate::writer::{encode_document, write_bytes_atomic, SfcWriteOptions};

fn required<'py>(dict: &Bound<'py, PyDict>, key: &str) -> PyResult<Bound<'py, PyAny>> {
    dict.get_item(key)?
        .ok_or_else(|| PyValueError::new_err(format!("Missing SFC field {key}")))
}

fn value_from_python(value: &Bound<'_, PyAny>, depth: usize) -> PyResult<Value> {
    if depth > 128 {
        return Err(PyValueError::new_err(
            "SFC value nesting exceeds 128 levels",
        ));
    }
    if value.is_instance_of::<PyBool>() {
        return Err(PyTypeError::new_err("Boolean is not an SFC parameter"));
    }
    if value.is_instance_of::<PyString>() {
        return Ok(Value::String(value.extract()?));
    }
    if value.is_instance_of::<PyInt>() {
        return Ok(Value::Integer(value.extract()?));
    }
    if value.is_instance_of::<PyFloat>() {
        return Ok(Value::Real(value.extract()?));
    }
    if let Ok(list) = value.downcast::<PyList>() {
        return list
            .iter()
            .map(|item| value_from_python(&item, depth + 1))
            .collect::<PyResult<Vec<_>>>()
            .map(Value::List);
    }
    if let Ok(dict) = value.downcast::<PyDict>() {
        let kind = required(dict, "kind")?.extract::<String>()?;
        let (result, size) = match kind.as_str() {
            "unset" => (Value::Unset, 1),
            "omitted" => (Value::Omitted, 1),
            "enum" => (Value::Enum(required(dict, "value")?.extract()?), 2),
            "binary" => (Value::Binary(required(dict, "value")?.extract()?), 2),
            "reference" => (Value::Reference(integer(&required(dict, "value")?)?), 2),
            "typed" => (
                Value::Typed {
                    keyword: required(dict, "keyword")?.extract()?,
                    parameters: parameters(&required(dict, "parameters")?, depth + 1)?,
                },
                3,
            ),
            _ => {
                return Err(PyValueError::new_err(format!(
                    "Unknown SFC value kind {kind}"
                )))
            }
        };
        if dict.len() != size {
            return Err(PyValueError::new_err("Unexpected SFC value fields"));
        }
        return Ok(result);
    }
    Err(PyTypeError::new_err("Invalid SFC parameter type"))
}

fn integer(value: &Bound<'_, PyAny>) -> PyResult<i64> {
    if !value.is_instance_of::<PyInt>() || value.is_instance_of::<PyBool>() {
        return Err(PyTypeError::new_err("SFC ID must be an integer"));
    }
    value.extract()
}

fn parameters(value: &Bound<'_, PyAny>, depth: usize) -> PyResult<Vec<Value>> {
    value
        .downcast::<PyList>()?
        .iter()
        .map(|item| value_from_python(&item, depth + 1))
        .collect()
}

fn record(value: &Bound<'_, PyAny>) -> PyResult<Record> {
    let dict = value.downcast::<PyDict>()?;
    if dict.len() != 2 {
        return Err(PyValueError::new_err("Unexpected SFC record fields"));
    }
    Ok(Record {
        keyword: required(dict, "keyword")?.extract()?,
        parameters: parameters(&required(dict, "parameters")?, 0)?,
    })
}

pub(crate) fn encode_python(
    parsed: &Bound<'_, PyDict>,
    allow_external_references: bool,
    literal_backslashes: bool,
) -> PyResult<Vec<u8>> {
    if required(parsed, "format")?.extract::<String>()? != "sfc" {
        return Err(PyValueError::new_err(
            "SFC serialization requires an SFC parse result",
        ));
    }
    if !required(parsed, "warnings")?
        .downcast::<PyList>()?
        .is_empty()
    {
        return Err(PyValueError::new_err(
            "Cannot save an SFC parse result containing warnings",
        ));
    }
    let header = required(parsed, "header")?.downcast_into::<PyDict>()?;
    let header_entities = required(&header, "entities")?
        .downcast_into::<PyList>()?
        .iter()
        .map(|item| record(&item))
        .collect::<PyResult<Vec<_>>>()?;
    let entities = required(parsed, "entities")?
        .downcast_into::<PyList>()?
        .iter()
        .map(|value| {
            let entity = value.downcast::<PyDict>()?;
            let body_type = required(entity, "body_type")?.extract::<String>()?;
            if body_type != "simple" || entity.len() != 4 {
                return Err(PyValueError::new_err(
                    "SFC entity must be a simple record with id, sfc_version, body_type and record",
                ));
            }
            let tag = match required(entity, "sfc_version")?
                .extract::<String>()?
                .as_str()
            {
                "2" => SfcVersionTag::V2,
                "3" => SfcVersionTag::V3,
                "3.1" => SfcVersionTag::V31,
                _ => return Err(PyValueError::new_err("Invalid SFC version marker")),
            };
            Ok(EntityInstance {
                id: integer(&required(entity, "id")?)?,
                body: EntityBody::Simple(record(&required(entity, "record")?)?),
                sfc_version: Some(tag),
            })
        })
        .collect::<PyResult<Vec<_>>>()?;
    let document = ParsedDocument {
        format: FileFormat::Sfc,
        header: HeaderSection {
            entities: header_entities,
        },
        entities,
        typed_features: Vec::new(),
        sfc_model: None,
    };
    let (bytes, validated) = encode_document(
        &document,
        SfcWriteOptions {
            allow_external_references,
            literal_backslashes,
        },
    )
    .map_err(|error| PyValueError::new_err(error.to_string()))?;
    let fresh = output_to_python(parsed.py(), &validated)?;
    for key in ["typed_features", "model"] {
        if !required(parsed, key)?.eq(required(&fresh, key)?)? {
            return Err(PyValueError::new_err(format!(
                "SFC {key} disagrees with entities; reparse the source before saving",
            )));
        }
    }
    // The parser exposes the required headers twice; a change to an alias must
    // never be silently discarded by saving header.entities instead.
    let fresh_header = required(&fresh, "header")?.downcast_into::<PyDict>()?;
    for key in ["file_description", "file_name", "file_schema"] {
        if !required(&header, key)?.eq(required(&fresh_header, key)?)? {
            return Err(PyValueError::new_err(format!(
                "SFC header.{key} disagrees with header.entities"
            )));
        }
    }
    Ok(bytes)
}

/// Validate a complete SFC parse result and return Shift-JIS/CP932 bytes.
/// External SAF references require explicit opt-in; dependencies are not copied.
/// literal_backslashes selects nonstandard CAD spelling and rejects ambiguous strings.
#[pyfunction]
#[pyo3(signature = (parsed, *, allow_external_references=false, literal_backslashes=false))]
fn serialize_sfc(
    parsed: &Bound<'_, PyDict>,
    allow_external_references: bool,
    literal_backslashes: bool,
) -> PyResult<Py<PyBytes>> {
    let bytes = encode_python(parsed, allow_external_references, literal_backslashes)?;
    Ok(PyBytes::new_bound(parsed.py(), &bytes).unbind())
}

/// Validate and atomically resave a complete SFC parse result to a file.
/// Header metadata and external-reference names are preserved unchanged.
/// literal_backslashes selects nonstandard CAD spelling and rejects ambiguous strings.
#[pyfunction]
#[pyo3(signature = (parsed, path, *, allow_external_references=false, literal_backslashes=false))]
fn write_sfc(
    parsed: &Bound<'_, PyDict>,
    path: &Bound<'_, PyAny>,
    allow_external_references: bool,
    literal_backslashes: bool,
) -> PyResult<()> {
    let bytes = encode_python(parsed, allow_external_references, literal_backslashes)?;
    let path = parsed
        .py()
        .import_bound("os")?
        .getattr("fspath")?
        .call1((path,))?
        .extract::<std::path::PathBuf>()?;
    write_bytes_atomic(&path, &bytes).map_err(Into::into)
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(serialize_sfc, module)?)?;
    module.add_function(wrap_pyfunction!(write_sfc, module)?)?;
    module.add_function(wrap_pyfunction!(write_sfc_bundle, module)?)?;
    module.add_function(wrap_pyfunction!(serialize_p21, module)?)?;
    module.add_function(wrap_pyfunction!(write_p21, module)?)?;
    Ok(())
}

fn p21_bytes(parsed: &Bound<'_, PyDict>) -> PyResult<Vec<u8>> {
    let bytes = encode_python(parsed, false, false)?;
    let output = crate::parser::parse_from_bytes(FileFormat::Sfc, &bytes, true)
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    crate::p21_writer::serialize_p21(&output)
        .map_err(|error| PyValueError::new_err(error.to_string()))
}

#[pyfunction]
fn serialize_p21(parsed: &Bound<'_, PyDict>) -> PyResult<Py<PyBytes>> {
    Ok(PyBytes::new_bound(parsed.py(), &p21_bytes(parsed)?).unbind())
}

#[pyfunction]
fn write_p21(parsed: &Bound<'_, PyDict>, path: &Bound<'_, PyAny>) -> PyResult<()> {
    write_bytes_atomic(&fspath(path)?, &p21_bytes(parsed)?).map_err(Into::into)
}

pub(crate) fn fspath(path: &Bound<'_, PyAny>) -> PyResult<std::path::PathBuf> {
    path.py()
        .import_bound("os")?
        .getattr("fspath")?
        .call1((path,))?
        .extract()
}

#[pyfunction]
#[pyo3(signature = (source, destination, *, file_name=None, extra_files=None))]
fn write_sfc_bundle<'py>(
    source: &Bound<'py, PyAny>,
    destination: &Bound<'py, PyAny>,
    file_name: Option<&str>,
    extra_files: Option<Vec<String>>,
) -> PyResult<Bound<'py, PyDict>> {
    let report = crate::bundle::write_sfc_bundle(
        &fspath(source)?,
        &fspath(destination)?,
        file_name,
        &extra_files.unwrap_or_default(),
    )
    .map_err(|error| {
        if error.kind() == std::io::ErrorKind::InvalidInput {
            PyValueError::new_err(error.to_string())
        } else {
            error.into()
        }
    })?;
    let result = PyDict::new_bound(source.py());
    result.set_item("drawing", report.drawing)?;
    result.set_item("files", report.files)?;
    Ok(result)
}
