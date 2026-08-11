// Copyright (c) 2026 Michael Saunders
mod midi;
mod musicxml;

use std::{fs::File, io::Read, path::Path};

use crate::{ImportError, NormalizedScore};

/// Resource limits and import policy. Defaults are intentionally conservative for desktop scores
/// while still accommodating large orchestral MusicXML files.
#[derive(Clone, Debug)]
pub struct ImportOptions {
    pub include_cue_notes: bool,
    pub include_hidden_notes: bool,
    pub max_input_bytes: u64,
    pub max_decompressed_bytes: u64,
    pub max_archive_entries: usize,
    pub max_xml_depth: usize,
    pub max_notes: usize,
    pub max_playback_measures: usize,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            include_cue_notes: false,
            include_hidden_notes: false,
            max_input_bytes: 64 * 1024 * 1024,
            max_decompressed_bytes: 128 * 1024 * 1024,
            max_archive_entries: 40_096,
            max_xml_depth: 256,
            max_notes: 1_000_000,
            max_playback_measures: 100_000,
        }
    }
}

pub fn import_path(
    path: impl AsRef<Path>,
    options: &ImportOptions,
) -> Result<NormalizedScore, ImportError> {
    let path = path.as_ref();
    let mut file = File::open(path).map_err(|source| ImportError::Io {
        path: path.to_owned(),
        source,
    })?;
    let metadata = file.metadata().map_err(|source| ImportError::Io {
        path: path.to_owned(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(ImportError::UnsupportedDocument(
            "the selected path is not a regular file".to_owned(),
        ));
    }
    if metadata.len() > options.max_input_bytes {
        return Err(ImportError::InputTooLarge {
            limit_name: "input size",
            actual: metadata.len(),
            limit: options.max_input_bytes,
        });
    }
    let capacity = usize::try_from(metadata.len().min(options.max_input_bytes)).unwrap_or(0);
    let mut bytes = Vec::with_capacity(capacity);
    file.by_ref()
        .take(options.max_input_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|source| ImportError::Io {
            path: path.to_owned(),
            source,
        })?;
    import_bytes(&bytes, options)
}

/// Auto-detect MusicXML, compressed MusicXML, or a Standard MIDI File by file signature/content.
pub fn import_bytes(bytes: &[u8], options: &ImportOptions) -> Result<NormalizedScore, ImportError> {
    enforce_input_limit(bytes, options)?;
    if bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06") {
        import_mxl(bytes, options)
    } else if bytes.starts_with(b"MThd") {
        import_midi(bytes, options)
    } else {
        import_musicxml(bytes, options)
    }
}

pub fn import_musicxml(
    bytes: &[u8],
    options: &ImportOptions,
) -> Result<NormalizedScore, ImportError> {
    enforce_input_limit(bytes, options)?;
    musicxml::parse_musicxml(bytes, options)
}

pub fn import_mxl(bytes: &[u8], options: &ImportOptions) -> Result<NormalizedScore, ImportError> {
    enforce_input_limit(bytes, options)?;
    musicxml::parse_mxl(bytes, options)
}

pub fn import_midi(bytes: &[u8], options: &ImportOptions) -> Result<NormalizedScore, ImportError> {
    enforce_input_limit(bytes, options)?;
    midi::parse_midi(bytes, options)
}

/// Return the uncompressed MusicXML document used by the notation view.
///
/// The same MXL container/rootfile resolution and decompression limits as native score import are
/// used. MIDI has no canonical MusicXML notation document, so it returns `None`.
pub fn display_musicxml_text(
    bytes: &[u8],
    options: &ImportOptions,
) -> Result<Option<String>, ImportError> {
    enforce_input_limit(bytes, options)?;
    let xml = if bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06") {
        musicxml::extract_mxl_musicxml(bytes, options)?
    } else if bytes.starts_with(b"MThd") {
        return Ok(None);
    } else {
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > options.max_decompressed_bytes {
            return Err(ImportError::InputTooLarge {
                limit_name: "decompressed MusicXML size",
                actual: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
                limit: options.max_decompressed_bytes,
            });
        }
        bytes.to_vec()
    };
    // Keep one accepted text encoding in the MVP. The semantic parser uses the same UTF-8 policy,
    // so the display and performance timelines cannot disagree about decoded source text.
    let text = String::from_utf8(xml)
        .map_err(|error| ImportError::InvalidXml(format!("MusicXML is not UTF-8: {error}")))?;
    Ok(Some(text.trim_start_matches('\u{feff}').to_owned()))
}

fn enforce_input_limit(bytes: &[u8], options: &ImportOptions) -> Result<(), ImportError> {
    let actual = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if actual > options.max_input_bytes {
        Err(ImportError::InputTooLarge {
            limit_name: "input size",
            actual,
            limit: options.max_input_bytes,
        })
    } else {
        Ok(())
    }
}
