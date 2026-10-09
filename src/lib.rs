#![allow(clippy::useless_conversion)]

//! Rust core for the `ezsxf` Python extension.

mod attributes;
mod authoring;
mod bulk_editor;
mod bundle;
mod complex_editor;
mod document;
mod editor;
mod features;
mod image;
mod model;
mod p21_bundle;
mod p21_model;
mod p21_writer;
mod parser;
mod python;
mod python_bulk;
mod python_editor;
mod python_writer;
mod saf;
mod style_editor;
mod writer;

pub use authoring::estimate_text_width;
pub use bundle::{write_sfc_bundle, SfcBundleReport};
pub use editor::SfcDocument;
pub use model::*;
pub use p21_writer::serialize_p21;
pub use parser::{parse_p21_text, parse_sfc_text};
pub use saf::{SafAttribute, SafDocument, SafFigure, SafSet};
pub use writer::{serialize_sfc, write_sfc, SfcWriteOptions, WriteError};

use pyo3::prelude::*;

#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    python::register(m)
}

#[cfg(test)]
mod tests;
