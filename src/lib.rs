#![allow(clippy::useless_conversion)]

//! Rust core for the `ezsxf` Python extension.

mod document;
mod features;
mod model;
mod parser;
mod python;

pub use model::*;
pub use parser::{parse_p21_text, parse_sfc_text};

use pyo3::prelude::*;

#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    python::register(m)
}

#[cfg(test)]
mod tests;
