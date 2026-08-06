// Copyright (c) 2026 Michael Saunders
use std::io::{Cursor, Write};

use tapconductor_score::{
    display_musicxml_text, import_bytes, ImportError, ImportOptions, ScoreFormat,
};
use zip::{write::SimpleFileOptions, ZipWriter};

const CONTAINER: &str = r#"<?xml version="1.0"?>
<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="scores/main.musicxml" media-type="application/vnd.recordare.musicxml+xml"/>
  </rootfiles>
</container>"#;

fn mxl(container: &str, score_path: &str, score: &[u8]) -> Vec<u8> {
    let cursor = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(cursor);
    let options = SimpleFileOptions::default();
    writer
        .start_file("META-INF/container.xml", options)
        .unwrap();
    writer.write_all(container.as_bytes()).unwrap();
    writer.start_file(score_path, options).unwrap();
    writer.write_all(score).unwrap();
    writer.finish().unwrap().into_inner()
}

#[test]
fn imports_mxl_and_exposes_same_uncompressed_document_for_osmd() {
    let xml = include_bytes!("fixtures/cross_parts.musicxml");
    let bytes = mxl(CONTAINER, "scores/main.musicxml", xml);
    let score = import_bytes(&bytes, &ImportOptions::default()).unwrap();
    assert_eq!(score.format, ScoreFormat::MusicXml);
    assert_eq!(score.tap_events.len(), 4);

    let display = display_musicxml_text(&bytes, &ImportOptions::default())
        .unwrap()
        .unwrap();
    assert_eq!(display.as_bytes(), xml);
}

#[test]
fn rejects_traversal_rootfile_before_opening_it() {
    let unsafe_container = CONTAINER.replace("scores/main.musicxml", "../score.musicxml");
    let bytes = mxl(
        &unsafe_container,
        "../score.musicxml",
        include_bytes!("fixtures/cross_parts.musicxml"),
    );
    assert!(matches!(
        import_bytes(&bytes, &ImportOptions::default()),
        Err(ImportError::InvalidArchive(_))
    ));
}

#[test]
fn display_helper_returns_none_for_midi() {
    assert_eq!(
        display_musicxml_text(b"MThd", &ImportOptions::default()).unwrap(),
        None
    );
}

#[test]
fn applies_decompressed_size_cap() {
    let bytes = mxl(CONTAINER, "scores/main.musicxml", &[b' '; 2048]);
    let options = ImportOptions {
        max_decompressed_bytes: 1024,
        ..ImportOptions::default()
    };
    assert!(matches!(
        display_musicxml_text(&bytes, &options),
        Err(ImportError::InputTooLarge { .. })
    ));
}
