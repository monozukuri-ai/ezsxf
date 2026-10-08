//! Conversion of nested Python dictionaries into the Rust batch input model.
use crate::bulk_editor::{BatchElement, Leaf, Placement};
use crate::model::Value;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use std::collections::BTreeMap;

fn err(s: impl Into<String>) -> PyErr {
    PyValueError::new_err(s.into())
}
fn required<'py>(d: &Bound<'py, PyDict>, key: &str) -> PyResult<Bound<'py, PyAny>> {
    d.get_item(key)?
        .ok_or_else(|| err(format!("Missing {key}")))
}
fn allowed(d: &Bound<'_, PyDict>, names: &[&str]) -> PyResult<()> {
    for key in d.keys() {
        let key = key.extract::<String>()?;
        if !names.contains(&key.as_str()) {
            return Err(err(format!("Unsupported field {key}")));
        }
    }
    Ok(())
}
fn pair(v: &Bound<'_, PyAny>) -> PyResult<(f64, f64)> {
    if !v.is_instance_of::<PyList>() && !v.is_instance_of::<PyTuple>() {
        return Err(err("Expected a coordinate/scale pair"));
    }
    let values = v
        .iter()?
        .map(|v| crate::python_editor::real_number(&v?))
        .collect::<PyResult<Vec<_>>>()?;
    if values.len() != 2 {
        return Err(err("Expected two coordinates"));
    }
    Ok((values[0], values[1]))
}
fn integer(d: &Bound<'_, PyDict>, key: &str, default: i64) -> PyResult<i64> {
    let Some(v) = d.get_item(key)? else {
        return Ok(default);
    };
    match crate::python_editor::scalar(&v)? {
        Value::Integer(n) => Ok(n),
        _ => Err(err(format!("{key} must be an integer"))),
    }
}
fn placement(d: &Bound<'_, PyDict>, explicit: bool) -> PyResult<Placement> {
    let mut result = Placement::default();
    match d.get_item("position")? {
        Some(v) => result.position = pair(&v)?,
        None if explicit => return Err(err("Each placement needs an explicit position")),
        _ => {}
    }
    if let Some(v) = d.get_item("angle")? {
        result.angle = crate::python_editor::real_number(&v)?;
    }
    if let Some(v) = d.get_item("scale")? {
        result.scale = if v.is_instance_of::<PyList>() || v.is_instance_of::<PyTuple>() {
            pair(&v)?
        } else {
            let n = crate::python_editor::real_number(&v)?;
            (n, n)
        };
    }
    result.layer = integer(d, "layer", 1)?;
    Ok(result)
}
fn boundary(value: &Bound<'_, PyAny>) -> PyResult<Vec<Leaf>> {
    let entries = value.iter()?.collect::<PyResult<Vec<_>>>()?;
    if entries.is_empty() {
        return Err(err("A boundary cannot be empty"));
    }
    if entries[0].is_instance_of::<PyDict>() {
        entries
            .iter()
            .enumerate()
            .map(|(i, v)| {
                {
                    let d = v.downcast::<PyDict>()?;
                    let kind = required(d, "kind")?.extract::<String>()?;
                    let fields = crate::python_editor::fields_from_dict(d, true)?;
                    Ok((kind, fields))
                }
                .map_err(|e: PyErr| err(format!("boundary[{i}]: {e}")))
            })
            .collect()
    } else {
        let mut coordinates = entries.iter().map(pair).collect::<PyResult<Vec<_>>>()?;
        if coordinates.first() != coordinates.last() {
            coordinates.push(coordinates[0]);
        }
        let points = Value::List(
            coordinates
                .into_iter()
                .map(|(x, y)| Value::List(vec![Value::Real(x), Value::Real(y)]))
                .collect(),
        );
        Ok(vec![(
            "polyline".into(),
            BTreeMap::from([("points".into(), points)]),
        )])
    }
}
pub(crate) fn element(value: &Bound<'_, PyAny>, depth: usize) -> PyResult<BatchElement> {
    if depth > 32 {
        return Err(err("Nested batch elements exceed depth 32"));
    }
    let d = value
        .downcast::<PyDict>()
        .map_err(|_| err("Expected an element dict"))?;
    let kind = required(d, "kind")?.extract::<String>()?;
    Ok(match kind.as_str() {
        "fill" | "hatch" => {
            allowed(d, &["kind", "outer", "holes", "layer", "color", "patterns"])?;
            let outer = boundary(&required(d, "outer")?)?;
            let holes = if let Some(v) = d.get_item("holes")? {
                v.iter()?
                    .map(|v| boundary(&v?))
                    .collect::<PyResult<Vec<_>>>()?
            } else {
                Vec::new()
            };
            let patterns = if kind == "hatch" {
                let v = required(d, "patterns")?;
                let Value::List(rows) = crate::python_editor::points(&v)? else {
                    unreachable!()
                };
                Some(
                    rows.into_iter()
                        .map(|v| match v {
                            Value::List(row) => row,
                            _ => unreachable!(),
                        })
                        .collect(),
                )
            } else {
                if d.contains("patterns")? {
                    return Err(err("patterns require kind=hatch"));
                }
                None
            };
            BatchElement::Fill {
                outer,
                holes,
                layer: integer(d, "layer", 1)?,
                color: integer(d, "color", 1)?,
                patterns,
            }
        }
        "composite_curve" => {
            allowed(
                d,
                &[
                    "kind",
                    "elements",
                    "visible",
                    "color",
                    "line_type",
                    "line_width",
                ],
            )?;
            BatchElement::Composite {
                boundary: boundary(&required(d, "elements")?)?,
                codes: [
                    integer(d, "color", 1)?,
                    integer(d, "line_type", 1)?,
                    integer(d, "line_width", 1)?,
                ],
                visible: d
                    .get_item("visible")?
                    .map(|v| v.extract::<bool>())
                    .transpose()?
                    .unwrap_or(false),
            }
        }
        "part" | "partial_drawing" | "group" => {
            allowed(
                d,
                &[
                    "kind",
                    "name",
                    "elements",
                    "placements",
                    "position",
                    "angle",
                    "scale",
                    "layer",
                ],
            )?;
            let name = required(d, "name")?.extract::<String>()?;
            let elements = required(d, "elements")?
                .iter()?
                .enumerate()
                .map(|(i, v)| {
                    element(&v?, depth + 1).map_err(|e| err(format!("elements[{i}]: {e}")))
                })
                .collect::<PyResult<Vec<_>>>()?;
            let (flag, placements) = if kind == "part" {
                if ["position", "angle", "scale", "layer"]
                    .iter()
                    .any(|k| d.contains(*k).unwrap_or(false))
                {
                    return Err(err("Part transforms belong in placements"));
                }
                let placements = required(d, "placements")?
                    .iter()?
                    .enumerate()
                    .map(|(i, v)| {
                        {
                            let v = v?;
                            let row = v.downcast::<PyDict>()?;
                            allowed(row, &["position", "angle", "scale", "layer"])?;
                            placement(row, true)
                        }
                        .map_err(|e: PyErr| err(format!("placements[{i}]: {e}")))
                    })
                    .collect::<PyResult<Vec<_>>>()?;
                (4, placements)
            } else {
                if d.contains("placements")? {
                    return Err(err(
                        "Use position/angle/scale/layer for group or partial_drawing",
                    ));
                }
                (
                    if kind == "group" { 3 } else { 1 },
                    vec![placement(d, false)?],
                )
            };
            BatchElement::Figure {
                name,
                kind: flag,
                elements,
                placements,
            }
        }
        "placement" | "part_placement" => {
            allowed(d, &["kind", "name", "position", "angle", "scale", "layer"])?;
            BatchElement::Placement {
                name: required(d, "name")?.extract()?,
                placement: placement(d, false)?,
            }
        }
        _ => BatchElement::Feature((kind, crate::python_editor::fields_from_dict(d, true)?)),
    })
}
