//! Transactional SFC/SAF/dependency delivery (attribute specification §§2-3, 3-4).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use encoding_rs::SHIFT_JIS;

use crate::model::*;
use crate::parser::parse_from_bytes;
use crate::writer::{encode_document, serialize_sfc, SfcWriteOptions};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SfcBundleReport {
    pub drawing: String,
    pub files: Vec<String>,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

fn validation(error: impl std::fmt::Display) -> io::Error {
    invalid(error.to_string())
}

/// Copy a validated drawing and its referenced SAF/images into a new directory.
/// All input/dependency checks complete before the directory is published.
/// `file_name` also updates FILE_NAME, ATRF names and SAF sxfFile consistently.
pub fn write_sfc_bundle(
    source: &Path,
    destination: &Path,
    file_name: Option<&str>,
    extra_files: &[String],
) -> io::Result<SfcBundleReport> {
    let files = prepare_bundle(source, file_name, extra_files)?;
    let drawing = file_name.unwrap_or(source.file_name().unwrap().to_str().unwrap());
    publish_bundle(destination, drawing, &files)
}

pub(crate) fn prepare_bundle(
    source: &Path,
    file_name: Option<&str>,
    extra_files: &[String],
) -> io::Result<BTreeMap<String, Vec<u8>>> {
    if !fs::symlink_metadata(source)?.is_file() {
        return Err(invalid("Source must be a regular SFC file"));
    }
    let source_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| invalid("SFC filename must be Unicode"))?;
    portable_name(source_name)?;
    let drawing_name = file_name.unwrap_or(source_name);
    portable_name(drawing_name)?;
    if !drawing_name.to_ascii_lowercase().ends_with(".sfc") {
        return Err(invalid("Bundle drawing filename must end in .sfc"));
    }
    let root = source
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let output = parse_from_bytes(FileFormat::Sfc, &fs::read(source)?, true).map_err(validation)?;
    // Check omission, precision and all duplicated views before changing names.
    serialize_sfc(
        &output,
        SfcWriteOptions {
            allow_external_references: true,
            ..SfcWriteOptions::default()
        },
    )
    .map_err(validation)?;
    let model = output.document.sfc_model.as_ref().unwrap();
    let mut saf_names = BTreeSet::new();
    let mut figure_ids = BTreeSet::new();
    let mut dependencies: BTreeSet<String> = extra_files.iter().cloned().collect();
    for attachment in &model.attribute_attachments {
        match &attachment.mechanism {
            SfcAttributeMechanism::AttributeFile { figure_id, .. } => {
                let name = attachment
                    .resolved_attribute_file_name
                    .as_ref()
                    .ok_or_else(|| invalid("Unresolved SAF filename"))?;
                saf_names.insert(name.clone());
                if !figure_ids.insert(figure_id.clone()) {
                    return Err(invalid(format!(
                        "Duplicate drawing ATRF Figure id {figure_id}"
                    )));
                }
            }
            SfcAttributeMechanism::SingleAttribute {
                attribute_name: Some(name),
                attribute_value: Some(value),
                ..
            } if matches!(name.as_str(), "画像" | "ファイル名") => {
                dependencies.insert(value.clone());
            }
            _ => {}
        }
    }
    if saf_names.len() > 1 {
        return Err(invalid("A bundle supports one SAF file per drawing"));
    }
    let mut document = output.document.clone();
    let header = document
        .header
        .entities
        .iter_mut()
        .find(|record| record.keyword.eq_ignore_ascii_case("FILE_NAME"))
        .ok_or_else(|| invalid("Missing FILE_NAME"))?;
    header.parameters[0] = Value::String(drawing_name.into());
    let mut files = BTreeMap::new();
    if let Some(saf_name) = saf_names.first() {
        let source_saf = resolve_file(root, saf_name)?;
        let bytes = fs::read(source_saf)?;
        let (mut text, shift_jis) = decode_xml(&bytes)?;
        let xml = roxmltree::Document::parse_with_options(
            &text,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                nodes_limit: 1_000_000,
                entity_resolver: None,
            },
        )
        .map_err(validation)?;
        let element = xml.root_element();
        if element.tag_name().name() != "SxfAttributeXML"
            || element.tag_name().namespace().is_some()
            || !matches!(element.attribute("version"), Some("3.0" | "3.1"))
            || element.attribute("date").is_none()
            || element.attribute("application").is_none()
        {
            return Err(invalid("Invalid SAF SxfAttributeXML root metadata"));
        }
        let name_attribute = element
            .attributes()
            .find(|attribute| attribute.name() == "sxfFile")
            .ok_or_else(|| invalid("SAF is missing sxfFile"))?;
        if file_key(name_attribute.value()) != file_key(source_name) {
            return Err(invalid(
                "SAF sxfFile does not match the source drawing filename",
            ));
        }
        let mut saf_ids = BTreeSet::new();
        for figure in element
            .children()
            .filter(|node| node.has_tag_name("Figure"))
        {
            let id = figure
                .attribute("id")
                .filter(|id| !id.is_empty())
                .ok_or_else(|| invalid("SAF Figure is missing id"))?;
            if !saf_ids.insert(id.to_string()) {
                return Err(invalid(format!("Duplicate SAF Figure id {id}")));
            }
        }
        if saf_ids != figure_ids {
            return Err(invalid("SAF Figure IDs do not match drawing ATRF IDs"));
        }
        for node in element
            .descendants()
            .filter(|node| node.has_tag_name("Attr"))
        {
            if matches!(node.attribute("name"), Some("画像" | "ファイル名")) {
                let name = node.text().unwrap_or_default().trim();
                portable_name(name)?;
                dependencies.insert(name.into());
            }
        }
        if let Some(dtd) = external_dtd(&text[..element.range().start])? {
            dependencies.insert(dtd);
        }
        let range = name_attribute.range_value();
        drop(xml);
        // Preserve every other XML byte spelling and opaque vendor attribute.
        text.replace_range(range, &xml_escape(drawing_name));
        let saf_bytes = if shift_jis {
            let (encoded, _, errors) = SHIFT_JIS.encode(&text);
            if errors {
                return Err(invalid("Renamed SAF is not representable in Shift-JIS"));
            }
            let (decoded, errors) = SHIFT_JIS.decode_without_bom_handling(&encoded);
            if errors || decoded != text {
                return Err(invalid("SAF encoding would change character values"));
            }
            encoded.into_owned()
        } else {
            text.into_bytes()
        };
        let target_saf = if file_name.is_some() {
            format!(
                "{}.SAF",
                Path::new(drawing_name)
                    .file_stem()
                    .unwrap()
                    .to_str()
                    .unwrap()
            )
        } else {
            saf_name.clone()
        };
        portable_name(&target_saf)?;
        files.insert(target_saf.clone(), saf_bytes);
        let names: BTreeMap<_, _> = model
            .attribute_attachments
            .iter()
            .filter_map(|attachment| match &attachment.mechanism {
                SfcAttributeMechanism::AttributeFile { figure_id, .. } if file_name.is_some() => {
                    Some((
                        attachment.name.clone(),
                        format!("$$ATRF$${figure_id}$${target_saf}"),
                    ))
                }
                _ => None,
            })
            .collect();
        for entity in &mut document.entities {
            let EntityBody::Simple(record) = &mut entity.body else {
                unreachable!()
            };
            let index = if record.keyword.eq_ignore_ascii_case("sfig_org_feature") {
                Some(0)
            } else if record.keyword.eq_ignore_ascii_case("sfig_locate_feature") {
                Some(1)
            } else {
                None
            };
            if let Some(index) = index {
                if let Value::String(name) = &mut record.parameters[index] {
                    if let Some(replacement) = names.get(name) {
                        *name = replacement.clone();
                    }
                }
            }
        }
    }
    for name in dependencies {
        portable_name(&name)?;
        if file_key(&name) == file_key(source_name)
            || saf_names.iter().any(|saf| file_key(saf) == file_key(&name))
        {
            return Err(invalid("Dependency collides with the drawing or SAF"));
        }
        if files
            .insert(name.clone(), fs::read(resolve_file(root, &name)?)?)
            .is_some()
        {
            return Err(invalid(format!("Duplicate output filename {name}")));
        }
    }
    let (drawing, _) = encode_document(
        &document,
        SfcWriteOptions {
            allow_external_references: true,
            ..SfcWriteOptions::default()
        },
    )
    .map_err(validation)?;
    if files.insert(drawing_name.into(), drawing).is_some() {
        return Err(invalid("Drawing filename collides with a dependency"));
    }
    Ok(files)
}

pub(crate) fn publish_bundle(
    destination: &Path,
    drawing: &str,
    files: &BTreeMap<String, Vec<u8>>,
) -> io::Result<SfcBundleReport> {
    match fs::symlink_metadata(destination) {
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Bundle destination must not exist",
            ))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut portable_names = BTreeSet::new();
    for name in files.keys() {
        portable_name(name)?;
        if !portable_names.insert(file_key(name)) {
            return Err(invalid("Output filenames collide on Windows"));
        }
    }
    publish_directory(destination, files)?;
    Ok(SfcBundleReport {
        drawing: drawing.into(),
        files: files.keys().cloned().collect(),
    })
}

pub(crate) fn portable_name(name: &str) -> io::Result<()> {
    let reserved = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if name.is_empty()
        || name.encode_utf16().count() > 255
        || matches!(name, "." | "..")
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|ch| ch.is_control() || "\\/:*?\"<>|".contains(ch))
        || matches!(reserved.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((reserved.starts_with("COM") || reserved.starts_with("LPT"))
            && reserved.chars().count() == 4
            && matches!(reserved.chars().nth(3), Some('1'..='9' | '¹' | '²' | '³')))
    {
        return Err(invalid(format!(
            "Expected a portable local filename: {name:?}"
        )));
    }
    Ok(())
}

// Conservative Unicode folding catches portable filename collisions as well
// as ASCII extension/case variants used by the supplied Windows samples.
fn file_key(name: &str) -> String {
    name.to_uppercase()
}

fn resolve_file(root: &Path, name: &str) -> io::Result<PathBuf> {
    portable_name(name)?;
    let mut candidates = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_str()
            .is_some_and(|value| file_key(value) == file_key(name))
        {
            candidates.push(entry.path());
        }
    }
    if candidates.len() != 1 {
        return Err(invalid(format!(
            "Missing or ambiguous bundle dependency {name}"
        )));
    }
    let path = candidates.pop().unwrap();
    if !fs::symlink_metadata(&path)?.is_file() {
        return Err(invalid(format!(
            "Dependency must be a regular file: {name}"
        )));
    }
    Ok(path)
}

pub(crate) fn decode_xml(bytes: &[u8]) -> io::Result<(String, bool)> {
    let prefix = String::from_utf8_lossy(&bytes[..bytes.len().min(200)]).to_ascii_lowercase();
    let declaration = prefix.split("?>").next().unwrap_or_default();
    let shift_jis = declaration.contains("shift_jis")
        || declaration.contains("shift-jis")
        || declaration.contains("windows-31j")
        || declaration.contains("cp932");
    if shift_jis {
        let (text, errors) = SHIFT_JIS.decode_without_bom_handling(bytes);
        if errors {
            return Err(invalid("Invalid Shift-JIS SAF"));
        }
        Ok((text.into_owned(), true))
    } else {
        if declaration.contains("encoding")
            && !declaration.contains("utf-8")
            && !declaration.contains("utf8")
        {
            return Err(invalid("SAF encoding must be UTF-8 or Shift-JIS/CP932"));
        }
        String::from_utf8(bytes.to_vec())
            .map(|text| (text, false))
            .map_err(validation)
    }
}

pub(crate) fn external_dtd(prefix: &str) -> io::Result<Option<String>> {
    let Some(start) = prefix.find("<!DOCTYPE") else {
        return Ok(None);
    };
    let clause = &prefix[start..];
    let tail = clause
        .strip_prefix("<!DOCTYPE")
        .unwrap()
        .trim_start()
        .strip_prefix("SxfAttributeXML")
        .ok_or_else(|| invalid("Unsupported SAF DOCTYPE"))?
        .trim_start()
        .strip_prefix("SYSTEM")
        .ok_or_else(|| invalid("Only local SYSTEM SAF DTDs are supported"))?
        .trim_start();
    let quote = tail
        .chars()
        .next()
        .filter(|ch| matches!(ch, '\'' | '"'))
        .ok_or_else(|| invalid("Invalid SAF DTD filename"))?;
    let end = tail[1..]
        .find(quote)
        .ok_or_else(|| invalid("Invalid SAF DTD filename"))?
        + 1;
    let name = &tail[1..end];
    if !tail[end + 1..].trim_start().starts_with('>') {
        return Err(invalid("Internal SAF DTD declarations are not supported"));
    }
    portable_name(name)?;
    Ok(Some(name.into()))
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn publish_directory(destination: &Path, files: &BTreeMap<String, Vec<u8>>) -> io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let stem = destination
        .file_name()
        .ok_or_else(|| invalid("Bundle destination must name a directory"))?;
    let staging = loop {
        let mut name = std::ffi::OsString::from(".");
        name.push(stem);
        name.push(format!(
            ".ezsxf-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let path = parent.join(name);
        match fs::create_dir(&path) {
            Ok(()) => break path,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    let result = (|| {
        for (name, bytes) in files {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(staging.join(name))?;
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        rename_directory_exclusive(&staging, destination)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn rename_directory_exclusive(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let source = std::ffi::CString::new(source.as_os_str().as_bytes()).map_err(validation)?;
    let destination =
        std::ffi::CString::new(destination.as_os_str().as_bytes()).map_err(validation)?;
    // Both C strings are owned and alive throughout the syscall. No overwrite.
    #[cfg(target_os = "linux")]
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    #[cfg(target_os = "macos")]
    let result =
        unsafe { libc::renamex_np(source.as_ptr(), destination.as_ptr(), libc::RENAME_EXCL) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(windows)]
fn rename_directory_exclusive(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    // std::fs::rename may replace an empty directory on Windows. Resolve the
    // existing parents to retain extended-length paths, then request no replace.
    let source = source.canonicalize()?;
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()?;
    let destination = parent.join(
        destination
            .file_name()
            .ok_or_else(|| invalid("Bundle destination must name a directory"))?,
    );
    let mut source: Vec<u16> = source.as_os_str().encode_wide().collect();
    let mut destination: Vec<u16> = destination.as_os_str().encode_wide().collect();
    if source.contains(&0) || destination.contains(&0) {
        return Err(invalid("Bundle path contains a NUL character"));
    }
    source.push(0);
    destination.push(0);
    // Both UTF-16 strings are terminated and live throughout the call. Zero
    // flags exclude MOVEFILE_REPLACE_EXISTING and cross-volume copying.
    if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), 0) } != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn rename_directory_exclusive(_: &Path, _: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Atomic bundle publication is supported on Linux, macOS and Windows",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "ezsxf-bundle-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            fs::write(
                root.join("writer-fixture.sfc"),
                include_str!("../tests/fixtures/writer_all_features.sfc"),
            )
            .unwrap();
            fs::write(root.join("writer-fixture.SAF"), "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<SxfAttributeXML version=\"3.1\" date=\"2026-10-05\" application=\"test\" sxfFile=\"writer-fixture.sfc\"><Figure id=\"102\"><AttributeSet name=\"sample\" version=\"1\" designedBy=\"test\"><Attr name=\"画像\">image.bmp</Attr><Attr name=\"note\">keep &amp; opaque</Attr></AttributeSet></Figure></SxfAttributeXML>").unwrap();
            fs::write(root.join("image.bmp"), b"opaque image bytes").unwrap();
            Self(root)
        }
        fn save(&self, rename: Option<&str>) -> io::Result<SfcBundleReport> {
            write_sfc_bundle(
                &self.0.join("writer-fixture.sfc"),
                &self.0.join("output"),
                rename,
                &[],
            )
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn bundle_rename_preserves_ids_and_updates_both_references() {
        let f = Fixture::new();
        let original = fs::read(f.0.join("writer-fixture.sfc")).unwrap();
        let report = f.save(Some("renamed.sfc")).unwrap();
        assert_eq!(
            report.files,
            vec!["image.bmp", "renamed.SAF", "renamed.sfc"]
        );
        let output = parse_from_bytes(
            FileFormat::Sfc,
            &fs::read(f.0.join("output/renamed.sfc")).unwrap(),
            true,
        )
        .unwrap();
        let original_model = parse_from_bytes(FileFormat::Sfc, &original, true).unwrap();
        assert_eq!(
            output
                .document
                .entities
                .iter()
                .map(|e| e.id)
                .collect::<Vec<_>>(),
            original_model
                .document
                .entities
                .iter()
                .map(|e| e.id)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            output
                .document
                .sfc_model
                .unwrap()
                .attribute_attachments
                .iter()
                .find(|a| matches!(a.mechanism, SfcAttributeMechanism::AttributeFile { .. }))
                .unwrap()
                .resolved_attribute_file_name
                .as_deref(),
            Some("renamed.SAF")
        );
        let saf = fs::read_to_string(f.0.join("output/renamed.SAF")).unwrap();
        assert!(saf.contains("sxfFile=\"renamed.sfc\""));
        assert!(saf.contains("keep &amp; opaque"));
        assert_eq!(fs::read(f.0.join("writer-fixture.sfc")).unwrap(), original);
        assert_eq!(
            fs::read(f.0.join("output/image.bmp")).unwrap(),
            b"opaque image bytes"
        );
    }
    #[test]
    fn bundle_default_keeps_raw_entities_and_existing_directory() {
        let f = Fixture::new();
        f.save(None).unwrap();
        let original = parse_from_bytes(
            FileFormat::Sfc,
            &fs::read(f.0.join("writer-fixture.sfc")).unwrap(),
            true,
        )
        .unwrap();
        let saved = parse_from_bytes(
            FileFormat::Sfc,
            &fs::read(f.0.join("output/writer-fixture.sfc")).unwrap(),
            true,
        )
        .unwrap();
        assert_eq!(saved, original);
        assert_eq!(
            f.save(None).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read_dir(f.0.join("output")).unwrap().count(), 3);
    }

    #[test]
    fn bundle_exclusive_publish_rejects_directory_created_after_validation() {
        let f = Fixture::new();
        let destination = f.0.join("output");
        fs::create_dir(&destination).unwrap();
        let files = BTreeMap::from([("drawing.sfc".into(), b"new content".to_vec())]);
        assert!(publish_directory(&destination, &files).is_err());
        assert!(destination.is_dir());
        assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
        assert_eq!(fs::read_dir(&f.0).unwrap().count(), 4);
    }
    #[test]
    fn bundle_invalid_and_missing_dependencies_publish_nothing() {
        let f = Fixture::new();
        let path = f.0.join("writer-fixture.SAF");
        let original = fs::read_to_string(&path).unwrap();
        for mutation in [
            original.replace("id=\"102\"", "id=\"103\""),
            original.replace("image.bmp", "../image.bmp"),
            original.replace("writer-fixture.sfc", "wrong.sfc"),
            original.replace("image.bmp", "missing.bmp"),
            original.replace(
                "</SxfAttributeXML>",
                "<Figure id=\"102\"/></SxfAttributeXML>",
            ),
            "<broken".into(),
        ] {
            fs::write(&path, mutation).unwrap();
            assert!(f.save(None).is_err());
            assert!(!f.0.join("output").exists());
            assert_eq!(fs::read_dir(&f.0).unwrap().count(), 3);
        }
    }
    #[test]
    fn bundle_cp932_xml_and_case_insensitive_dependency_resolution() {
        let f = Fixture::new();
        let path = f.0.join("writer-fixture.SAF");
        let source = fs::read_to_string(&path)
            .unwrap()
            .replace("UTF-8", "Shift_JIS")
            .replace("image.bmp", "IMAGE.BMP");
        let (bytes, _, errors) = SHIFT_JIS.encode(&source);
        assert!(!errors);
        fs::write(path, bytes).unwrap();
        f.save(Some("日本語.sfc")).unwrap();
        let saved = fs::read(f.0.join("output/日本語.SAF")).unwrap();
        let (text, errors) = SHIFT_JIS.decode_without_bom_handling(&saved);
        assert!(!errors);
        assert!(text.contains("日本語.sfc"));
        assert!(f.0.join("output/IMAGE.BMP").is_file());
    }
    #[test]
    fn bundle_copies_local_dtd_but_rejects_remote_and_internal_entities() {
        let f = Fixture::new();
        let path = f.0.join("writer-fixture.SAF");
        let source = fs::read_to_string(&path).unwrap();
        fs::write(
            f.0.join("attributes.dtd"),
            b"<!ELEMENT SxfAttributeXML ANY>",
        )
        .unwrap();
        let with_dtd = source.replace(
            "<SxfAttributeXML",
            "<!DOCTYPE SxfAttributeXML SYSTEM \"attributes.dtd\">\n<SxfAttributeXML",
        );
        fs::write(&path, &with_dtd).unwrap();
        let report = f.save(None).unwrap();
        assert!(report.files.contains(&"attributes.dtd".into()));
        for dtd in [
            "https://example.invalid/attributes.dtd",
            "../attributes.dtd",
        ] {
            fs::write(&path, with_dtd.replace("attributes.dtd", dtd)).unwrap();
            assert!(
                write_sfc_bundle(&f.0.join("writer-fixture.sfc"), &f.0.join("bad"), None, &[])
                    .is_err()
            );
            assert!(!f.0.join("bad").exists());
        }
    }
}
