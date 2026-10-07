//! SXF delivery: one P21 with its referenced SAF/raster dependencies.
//! P2Z follows OCF implementation convention §26 (ZIP, no encryption).
use crate::bundle::{portable_name, publish_bundle, SfcBundleReport};
use crate::editor::SfcDocument;
use crate::model::FileFormat;
use crate::p21_writer::serialize_p21_with_dependencies;
use crate::writer::{write_bytes_atomic, SfcWriteOptions};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Cursor, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;

fn invalid(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, error.to_string())
}

impl SfcDocument {
    fn prepare_p21_files(
        &self,
        file_name: Option<&str>,
    ) -> io::Result<(String, BTreeMap<String, Vec<u8>>)> {
        let source = self
            .snapshot()
            .document
            .header
            .find_keyword("FILE_NAME")
            .and_then(|r| r.parameters.first())
            .and_then(|v| v.as_string())
            .ok_or_else(|| invalid("Missing FILE_NAME"))?;
        let name = file_name.map(str::to_owned).unwrap_or_else(|| {
            Path::new(&source)
                .with_extension("p21")
                .to_string_lossy()
                .into_owned()
        });
        portable_name(&name)?;
        if !name.to_ascii_lowercase().ends_with(".p21") {
            return Err(invalid("P21 drawing filename must end in .p21"));
        }
        // Reuse SFC dependency/SAF-ID/image validation before constructing AP202.
        let sfc_name = Path::new(&name)
            .with_extension("sfc")
            .to_string_lossy()
            .into_owned();
        let (_, mut files) = self.prepare_sfc_files(Some(&sfc_name), SfcWriteOptions::default())?;
        let source_bytes = files.remove(&sfc_name).unwrap();
        let output = crate::parser::parse_from_bytes(FileFormat::Sfc, &source_bytes, true)
            .map_err(invalid)?;
        let saf_name = Path::new(&name)
            .with_extension("SAF")
            .to_string_lossy()
            .into_owned();
        if let Some(bytes) = files.get_mut(&saf_name) {
            *bytes = self
                .saf
                .as_ref()
                .unwrap()
                .to_bytes(&name)
                .map_err(invalid)?;
        }
        let drawing =
            serialize_p21_with_dependencies(&output, true, Some(&name)).map_err(invalid)?;
        if files.insert(name.clone(), drawing).is_some() {
            return Err(invalid("Dependency collides with P21"));
        }
        let mut names = BTreeSet::new();
        for filename in files.keys() {
            portable_name(filename)?;
            if !names.insert(filename.to_uppercase()) {
                return Err(invalid("Output filenames collide on Windows"));
            }
        }
        Ok((name, files))
    }
    pub fn save_p21_bundle(
        &self,
        destination: &Path,
        file_name: Option<&str>,
    ) -> io::Result<SfcBundleReport> {
        let (name, files) = self.prepare_p21_files(file_name)?;
        publish_bundle(destination, &name, &files)
    }
    pub fn to_p2z_bytes(&self, file_name: Option<&str>) -> io::Result<Vec<u8>> {
        let (drawing, files) = self.prepare_p21_files(file_name)?;
        // No unrelated files, arbitrary attachments or external DTDs in P2Z.
        // They remain available through the uncompressed bundle API.
        for (name, bytes) in &files {
            match Path::new(name)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str()
            {
                "p21" if name == &drawing => {}
                "saf"
                    if self.saf.is_some()
                        && name == &Path::new(&drawing).with_extension("SAF").to_string_lossy() => {
                }
                "tif" | "tiff" | "jpg" | "jpeg" => {
                    crate::image::validate_image(name, bytes).map_err(invalid)?
                }
                _ => {
                    return Err(invalid(format!(
                        "P2Z accepts only the drawing, SAF and TIFF/JPEG dependencies: {name}"
                    )))
                }
            }
        }
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);
        for (name, bytes) in &files {
            archive.start_file(name, options).map_err(invalid)?;
            archive.write_all(bytes)?;
        }
        Ok(archive.finish().map_err(invalid)?.into_inner())
    }
    pub fn save_p2z(&self, path: &Path) -> io::Result<()> {
        let filename = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| invalid("P2Z filename must be Unicode"))?;
        portable_name(filename)?;
        if !filename.to_ascii_lowercase().ends_with(".p2z") {
            return Err(invalid("P2Z filename must end in .p2z"));
        }
        let drawing = Path::new(filename)
            .with_extension("p21")
            .to_string_lossy()
            .into_owned();
        let bytes = self.to_p2z_bytes(Some(&drawing))?;
        write_bytes_atomic(path, &bytes)
    }
}
