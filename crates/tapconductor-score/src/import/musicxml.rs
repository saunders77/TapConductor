use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read},
};

use quick_xml::{
    events::{BytesStart, Event},
    Reader,
};
use zip::ZipArchive;

use super::ImportOptions;
use crate::{
    ImportError, ImportWarning, NormalizedScore, NoteAttack, PartInfo, Rational, ScoreFormat,
    ScoreMetadata, SourceAnchor, SourceContext, SpelledPitch, Step, TieInfo, WarningCode,
};

pub(super) fn parse_mxl(
    bytes: &[u8],
    options: &ImportOptions,
) -> Result<NormalizedScore, ImportError> {
    let score_xml = extract_mxl_musicxml(bytes, options)?;
    parse_musicxml(&score_xml, options)
}

pub(super) fn extract_mxl_musicxml(
    bytes: &[u8],
    options: &ImportOptions,
) -> Result<Vec<u8>, ImportError> {
    let cursor = Cursor::new(bytes);
    let mut archive =
        ZipArchive::new(cursor).map_err(|error| ImportError::InvalidArchive(error.to_string()))?;

    let container = read_zip_entry(
        &mut archive,
        "META-INF/container.xml",
        options.max_decompressed_bytes.min(1024 * 1024),
    )?
    .ok_or(ImportError::MissingMxlRootfile)?;
    let root_path = parse_container_rootfile(&container, options.max_xml_depth)?
        .ok_or(ImportError::MissingMxlRootfile)?;
    validate_archive_path(&root_path)?;

    read_zip_entry(&mut archive, &root_path, options.max_decompressed_bytes)?
        .ok_or(ImportError::MissingMxlRootfile)
}

fn read_zip_entry<R: std::io::Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
    limit: u64,
) -> Result<Option<Vec<u8>>, ImportError> {
    let mut file = match archive.by_name(name) {
        Ok(file) => file,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(error) => return Err(ImportError::InvalidArchive(error.to_string())),
    };
    if file.size() > limit {
        return Err(ImportError::InputTooLarge {
            limit_name: "decompressed entry size",
            actual: file.size(),
            limit,
        });
    }

    let capacity = usize::try_from(file.size())
        .unwrap_or(0)
        .min(4 * 1024 * 1024);
    let mut bytes = Vec::with_capacity(capacity);
    file.by_ref()
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| ImportError::InvalidArchive(error.to_string()))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > limit {
        return Err(ImportError::InputTooLarge {
            limit_name: "decompressed entry size",
            actual: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            limit,
        });
    }
    Ok(Some(bytes))
}

fn validate_archive_path(path: &str) -> Result<(), ImportError> {
    let invalid = path.is_empty()
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.contains('\\')
        || path.split('/').any(|component| component == "..")
        || path.as_bytes().get(1) == Some(&b':');
    if invalid {
        Err(ImportError::InvalidArchive(format!(
            "unsafe rootfile path {path:?}"
        )))
    } else {
        Ok(())
    }
}

fn parse_container_rootfile(xml: &[u8], max_depth: usize) -> Result<Option<String>, ImportError> {
    let mut reader = Reader::from_reader(Cursor::new(xml));
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut depth = 0_usize;
    let mut fallback = None;
    loop {
        match reader
            .read_event_into(&mut buffer)
            .map_err(|error| ImportError::InvalidArchive(error.to_string()))?
        {
            Event::Start(element) => {
                depth = depth.saturating_add(1);
                if depth > max_depth {
                    return Err(ImportError::ResourceLimit {
                        kind: "XML nesting depth",
                        limit: max_depth,
                    });
                }
                if local_name(element.name().as_ref()) == "rootfile" {
                    let full_path = attribute(&element, "full-path")?;
                    let media_type = attribute(&element, "media-type")?;
                    if let Some(path) = full_path {
                        if media_type.as_deref() == Some("application/vnd.recordare.musicxml+xml") {
                            return Ok(Some(path));
                        }
                        if fallback.is_none()
                            && (path.to_ascii_lowercase().ends_with(".musicxml")
                                || path.to_ascii_lowercase().ends_with(".xml"))
                        {
                            fallback = Some(path);
                        }
                    }
                }
            }
            Event::Empty(element) => {
                if depth.saturating_add(1) > max_depth {
                    return Err(ImportError::ResourceLimit {
                        kind: "XML nesting depth",
                        limit: max_depth,
                    });
                }
                if local_name(element.name().as_ref()) == "rootfile" {
                    let full_path = attribute(&element, "full-path")?;
                    let media_type = attribute(&element, "media-type")?;
                    if let Some(path) = full_path {
                        if media_type.as_deref() == Some("application/vnd.recordare.musicxml+xml") {
                            return Ok(Some(path));
                        }
                        if fallback.is_none()
                            && (path.to_ascii_lowercase().ends_with(".musicxml")
                                || path.to_ascii_lowercase().ends_with(".xml"))
                        {
                            fallback = Some(path);
                        }
                    }
                }
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            Event::DocType(_) => {
                return Err(ImportError::InvalidArchive(
                    "DOCTYPE declarations are not accepted".to_owned(),
                ))
            }
            Event::Eof => return Ok(fallback),
            _ => {}
        }
        buffer.clear();
    }
}

pub(super) fn parse_musicxml(
    bytes: &[u8],
    options: &ImportOptions,
) -> Result<NormalizedScore, ImportError> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > options.max_decompressed_bytes {
        return Err(ImportError::InputTooLarge {
            limit_name: "decompressed MusicXML size",
            actual: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            limit: options.max_decompressed_bytes,
        });
    }

    let mut reader = Reader::from_reader(Cursor::new(bytes));
    reader.config_mut().trim_text(true);
    let mut state = XmlState::new(options);
    let mut buffer = Vec::new();
    let mut root_seen = false;
    let mut doctype_seen = false;

    loop {
        match reader
            .read_event_into(&mut buffer)
            .map_err(|error| ImportError::InvalidXml(error.to_string()))?
        {
            Event::Start(element) => {
                let tag = local_name(element.name().as_ref()).to_owned();
                if !root_seen {
                    check_root(&tag)?;
                    root_seen = true;
                }
                state.start(&tag, &element)?;
                state.path.push(tag);
                if state.path.len() > options.max_xml_depth {
                    return Err(ImportError::ResourceLimit {
                        kind: "XML nesting depth",
                        limit: options.max_xml_depth,
                    });
                }
            }
            Event::Empty(element) => {
                let tag = local_name(element.name().as_ref()).to_owned();
                if !root_seen {
                    check_root(&tag)?;
                    root_seen = true;
                }
                state.start(&tag, &element)?;
                state.path.push(tag.clone());
                if state.path.len() > options.max_xml_depth {
                    return Err(ImportError::ResourceLimit {
                        kind: "XML nesting depth",
                        limit: options.max_xml_depth,
                    });
                }
                state.end(&tag)?;
                state.path.pop();
            }
            Event::Text(text) => {
                let raw = std::str::from_utf8(text.as_ref())
                    .map_err(|error| ImportError::InvalidXml(error.to_string()))?;
                let value = quick_xml::escape::unescape(raw)
                    .map_err(|error| ImportError::InvalidXml(error.to_string()))?;
                state.text(value.trim())?;
            }
            Event::CData(text) => {
                let value = std::str::from_utf8(text.as_ref())
                    .map_err(|error| ImportError::InvalidXml(error.to_string()))?;
                state.text(value.trim())?;
            }
            Event::End(element) => {
                let tag = local_name(element.name().as_ref()).to_owned();
                state.end(&tag)?;
                state.path.pop();
            }
            Event::DocType(declaration) => {
                validate_musicxml_doctype(declaration.as_ref(), root_seen, &mut doctype_seen)?;
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }

    if !root_seen {
        return Err(ImportError::InvalidXml("empty XML document".to_owned()));
    }
    state.finish()
}

/// Accept the ordinary external MusicXML DTD marker without loading or
/// interpreting the referenced DTD. Internal subsets remain forbidden so a
/// score cannot declare custom entities (the usual XXE/entity-expansion
/// attack surface). `quick-xml` does not resolve external identifiers, and
/// unknown entity references in document text are rejected by `unescape`.
fn validate_musicxml_doctype(
    bytes: &[u8],
    root_seen: bool,
    doctype_seen: &mut bool,
) -> Result<(), ImportError> {
    if root_seen {
        return Err(ImportError::InvalidXml(
            "DOCTYPE must appear before the score root element".to_owned(),
        ));
    }
    if *doctype_seen {
        return Err(ImportError::InvalidXml(
            "multiple DOCTYPE declarations are not accepted".to_owned(),
        ));
    }
    let declaration = std::str::from_utf8(bytes)
        .map_err(|error| ImportError::InvalidXml(error.to_string()))?
        .trim();
    if declaration.len() > 4_096 {
        return Err(ImportError::InvalidXml(
            "DOCTYPE declaration is unreasonably large".to_owned(),
        ));
    }
    if declaration
        .bytes()
        .any(|byte| matches!(byte, b'[' | b']' | b'<' | b'>'))
    {
        return Err(ImportError::InvalidXml(
            "DOCTYPE internal subsets and entity declarations are not accepted".to_owned(),
        ));
    }

    let mut fields = declaration.splitn(2, char::is_whitespace);
    let root = fields.next().unwrap_or_default();
    let external_id = fields.next().unwrap_or_default().trim_start();
    if root != "score-partwise" {
        return Err(ImportError::UnsupportedDocument(format!(
            "DOCTYPE declares {root}; expected score-partwise"
        )));
    }
    if !external_id.is_empty()
        && !external_id.starts_with("PUBLIC ")
        && !external_id.starts_with("SYSTEM ")
    {
        return Err(ImportError::InvalidXml(
            "DOCTYPE may contain only a PUBLIC or SYSTEM external identifier".to_owned(),
        ));
    }

    *doctype_seen = true;
    Ok(())
}

fn check_root(tag: &str) -> Result<(), ImportError> {
    match tag {
        "score-partwise" => Ok(()),
        "score-timewise" => Err(ImportError::UnsupportedDocument(
            "score-timewise MusicXML; convert to score-partwise first".to_owned(),
        )),
        other => Err(ImportError::UnsupportedDocument(format!(
            "expected score-partwise MusicXML, found {other}"
        ))),
    }
}

fn local_name(bytes: &[u8]) -> &str {
    let local = bytes.rsplit(|byte| *byte == b':').next().unwrap_or(bytes);
    std::str::from_utf8(local).unwrap_or("")
}

fn attribute(element: &BytesStart<'_>, wanted: &str) -> Result<Option<String>, ImportError> {
    for attribute in element.attributes().with_checks(false) {
        let attribute = attribute.map_err(|error| ImportError::InvalidXml(error.to_string()))?;
        if local_name(attribute.key.as_ref()) == wanted {
            let raw = std::str::from_utf8(attribute.value.as_ref())
                .map_err(|error| ImportError::InvalidXml(error.to_string()))?;
            let value = quick_xml::escape::unescape(raw)
                .map_err(|error| ImportError::InvalidXml(error.to_string()))?;
            return Ok(Some(value.into_owned()));
        }
    }
    Ok(None)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MovementKind {
    Backup,
    Forward,
}

struct XmlState<'a> {
    options: &'a ImportOptions,
    path: Vec<String>,
    metadata: ScoreMetadata,
    definitions: BTreeMap<String, PartDefinition>,
    current_definition: Option<String>,
    creator_is_composer: bool,
    current_part: Option<PartBuilder>,
    current_measure: Option<MeasureBuilder>,
    current_note: Option<NoteBuilder>,
    movement: Option<(MovementKind, Option<i64>)>,
    parts: Vec<ParsedPart>,
    warnings: Vec<ImportWarning>,
    source_ids: BTreeSet<String>,
    note_count: usize,
}

#[derive(Clone, Default)]
struct PartDefinition {
    name: Option<String>,
    abbreviation: Option<String>,
}

struct PartBuilder {
    id: String,
    order: usize,
    divisions: i64,
    time: Option<(u32, u32)>,
    pending_time: Option<(Option<u32>, Option<u32>)>,
    transpose: i16,
    pending_transpose: Option<(i16, i16)>,
    active_endings: BTreeSet<u32>,
    measures: Vec<ParsedMeasure>,
}

struct MeasureBuilder {
    id: String,
    index: usize,
    implicit: bool,
    cursor: Rational,
    max_cursor: Rational,
    notes: Vec<RawNote>,
    note_ordinal: usize,
    last_onset_by_voice: BTreeMap<String, Rational>,
    inherited_endings: BTreeSet<u32>,
    ending_starts: BTreeSet<u32>,
    ending_stops: BTreeSet<u32>,
    clear_endings: bool,
    repeat_forward: bool,
    repeat_backward: Option<u32>,
}

struct NoteBuilder {
    xml_id: Option<String>,
    divisions: i64,
    transpose: i16,
    duration_units: Option<i64>,
    voice: String,
    staff: u16,
    chord: bool,
    rest: bool,
    grace: bool,
    cue: bool,
    hidden: bool,
    muted: bool,
    step: Option<Step>,
    alter: Rational,
    octave: Option<i8>,
    unpitched: bool,
    tie_start: bool,
    tie_stop: bool,
    tie_number: Option<u8>,
}

struct ParsedPart {
    id: String,
    order: usize,
    measures: Vec<ParsedMeasure>,
}

struct ParsedMeasure {
    id: String,
    duration: Rational,
    time: (u32, u32),
    notes: Vec<RawNote>,
    repeat_forward: bool,
    repeat_backward: Option<u32>,
    endings: BTreeSet<u32>,
}

#[derive(Clone)]
struct RawNote {
    source_id: String,
    xml_id: Option<String>,
    measure_index: usize,
    order: usize,
    onset: Rational,
    duration: Rational,
    staff: u16,
    voice: String,
    written_pitch: SpelledPitch,
    midi_pitch: u8,
    tie_start: bool,
    tie_stop: bool,
    tie_number: Option<u8>,
}

impl<'a> XmlState<'a> {
    fn new(options: &'a ImportOptions) -> Self {
        Self {
            options,
            path: Vec::new(),
            metadata: ScoreMetadata::default(),
            definitions: BTreeMap::new(),
            current_definition: None,
            creator_is_composer: false,
            current_part: None,
            current_measure: None,
            current_note: None,
            movement: None,
            parts: Vec::new(),
            warnings: Vec::new(),
            source_ids: BTreeSet::new(),
            note_count: 0,
        }
    }

    fn start(&mut self, tag: &str, element: &BytesStart<'_>) -> Result<(), ImportError> {
        match tag {
            "score-part" => {
                let id = attribute(element, "id")?.unwrap_or_default();
                self.definitions.entry(id.clone()).or_default();
                self.current_definition = Some(id);
            }
            "creator" => {
                self.creator_is_composer = attribute(element, "type")?
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("composer"));
            }
            "part" => {
                let id = attribute(element, "id")?
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| ImportError::InvalidXml("part is missing an id".to_owned()))?;
                if self.parts.iter().any(|part| part.id == id)
                    || self.current_part.as_ref().is_some_and(|part| part.id == id)
                {
                    return Err(ImportError::InvalidXml(format!("duplicate part id {id:?}")));
                }
                self.current_part = Some(PartBuilder {
                    id,
                    order: self.parts.len(),
                    divisions: 1,
                    time: None,
                    pending_time: None,
                    transpose: 0,
                    pending_transpose: None,
                    active_endings: BTreeSet::new(),
                    measures: Vec::new(),
                });
            }
            "measure" => {
                let part = self.current_part.as_ref().ok_or_else(|| {
                    ImportError::InvalidXml("measure appears outside a part".to_owned())
                })?;
                let index = part.measures.len();
                let id = attribute(element, "number")?
                    .filter(|number| !number.is_empty())
                    .unwrap_or_else(|| (index + 1).to_string());
                let implicit = attribute(element, "implicit")?
                    .is_some_and(|value| value.eq_ignore_ascii_case("yes"));
                self.current_measure = Some(MeasureBuilder {
                    id,
                    index,
                    implicit,
                    cursor: Rational::ZERO,
                    max_cursor: Rational::ZERO,
                    notes: Vec::new(),
                    note_ordinal: 0,
                    last_onset_by_voice: BTreeMap::new(),
                    inherited_endings: part.active_endings.clone(),
                    ending_starts: BTreeSet::new(),
                    ending_stops: BTreeSet::new(),
                    clear_endings: false,
                    repeat_forward: false,
                    repeat_backward: None,
                });
            }
            "time" => {
                if let Some(part) = &mut self.current_part {
                    part.pending_time = Some((None, None));
                }
            }
            "transpose" => {
                if let Some(part) = &mut self.current_part {
                    part.pending_transpose = Some((0, 0));
                }
            }
            "note" => {
                self.note_count = self.note_count.saturating_add(1);
                if self.note_count > self.options.max_notes {
                    return Err(ImportError::ResourceLimit {
                        kind: "note count",
                        limit: self.options.max_notes,
                    });
                }
                let part = self.current_part.as_ref().ok_or_else(|| {
                    ImportError::InvalidXml("note appears outside a part".to_owned())
                })?;
                self.current_note = Some(NoteBuilder {
                    xml_id: attribute(element, "id")?,
                    divisions: part.divisions,
                    transpose: part.transpose,
                    duration_units: None,
                    voice: "1".to_owned(),
                    staff: 1,
                    chord: false,
                    rest: false,
                    grace: false,
                    cue: false,
                    hidden: attribute(element, "print-object")?
                        .is_some_and(|value| value.eq_ignore_ascii_case("no")),
                    muted: false,
                    step: None,
                    alter: Rational::ZERO,
                    octave: None,
                    unpitched: false,
                    tie_start: false,
                    tie_stop: false,
                    tie_number: None,
                });
            }
            "chord" => {
                if let Some(note) = &mut self.current_note {
                    note.chord = true;
                }
            }
            "rest" => {
                if let Some(note) = &mut self.current_note {
                    note.rest = true;
                }
            }
            "grace" => {
                if let Some(note) = &mut self.current_note {
                    note.grace = true;
                }
            }
            "cue" => {
                if let Some(note) = &mut self.current_note {
                    note.cue = true;
                }
            }
            "unpitched" => {
                if let Some(note) = &mut self.current_note {
                    note.unpitched = true;
                }
            }
            "tie" | "tied" => {
                if let Some(note) = &mut self.current_note {
                    match attribute(element, "type")?.as_deref() {
                        Some("start") => note.tie_start = true,
                        Some("stop") => note.tie_stop = true,
                        Some("continue") => {
                            note.tie_start = true;
                            note.tie_stop = true;
                        }
                        _ => {}
                    }
                    if let Some(number) =
                        attribute(element, "number")?.and_then(|value| value.parse::<u8>().ok())
                    {
                        note.tie_number = Some(number);
                    }
                }
            }
            "backup" => self.movement = Some((MovementKind::Backup, None)),
            "forward" => self.movement = Some((MovementKind::Forward, None)),
            "repeat" => {
                if let Some(measure) = &mut self.current_measure {
                    match attribute(element, "direction")?.as_deref() {
                        Some("forward") => measure.repeat_forward = true,
                        Some("backward") => {
                            let times = attribute(element, "times")?
                                .and_then(|value| value.parse::<u32>().ok())
                                .filter(|times| *times >= 2)
                                .unwrap_or(2);
                            measure.repeat_backward = Some(times);
                        }
                        _ => {}
                    }
                }
            }
            "ending" => {
                if let Some(measure) = &mut self.current_measure {
                    let numbers = parse_ending_numbers(attribute(element, "number")?.as_deref());
                    match attribute(element, "type")?.as_deref() {
                        Some("start") => measure.ending_starts.extend(numbers),
                        Some("stop") => {
                            if numbers.is_empty() {
                                measure.clear_endings = true;
                            } else {
                                measure.ending_stops.extend(numbers);
                            }
                        }
                        Some("discontinue") => measure.clear_endings = true,
                        _ => {}
                    }
                }
            }
            "octave-shift" => {
                // Silently ignoring an ottava line would display the expected
                // notation while sounding the wrong octave in a live setting.
                // Fail closed until octave-shift spans are represented in the
                // normalized pitch timeline.
                return Err(ImportError::UnsupportedDocument(format!(
                    "MusicXML octave-shift at {}; ottava playback is not yet supported",
                    self.context_label()
                )));
            }
            "sound" => self.reject_navigation_attributes(element)?,
            _ => {}
        }
        Ok(())
    }

    fn text(&mut self, value: &str) -> Result<(), ImportError> {
        if value.is_empty() {
            return Ok(());
        }
        let Some(tag) = self.path.last().map(String::as_str) else {
            return Ok(());
        };
        match tag {
            "work-title" => self.metadata.title = Some(value.to_owned()),
            "movement-title" => self.metadata.movement_title = Some(value.to_owned()),
            "creator" if self.creator_is_composer => {
                self.metadata.composer = Some(value.to_owned())
            }
            "part-name" => {
                if let Some(id) = &self.current_definition {
                    self.definitions.entry(id.clone()).or_default().name = Some(value.to_owned());
                }
            }
            "part-abbreviation" => {
                if let Some(id) = &self.current_definition {
                    self.definitions.entry(id.clone()).or_default().abbreviation =
                        Some(value.to_owned());
                }
            }
            "divisions" if self.current_note.is_none() => {
                let divisions = parse_positive_i64(value, "divisions")?;
                if let Some(part) = &mut self.current_part {
                    part.divisions = divisions;
                }
            }
            "beats" => {
                let beats = value
                    .parse::<u32>()
                    .map_err(|_| ImportError::InvalidTiming {
                        context: self.context_label(),
                        message: format!("invalid beats value {value:?}"),
                    })?;
                if let Some((pending, _)) = self
                    .current_part
                    .as_mut()
                    .and_then(|part| part.pending_time.as_mut())
                {
                    *pending = Some(beats);
                }
            }
            "beat-type" => {
                let beat_type = value
                    .parse::<u32>()
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or_else(|| ImportError::InvalidTiming {
                        context: self.context_label(),
                        message: format!("invalid beat-type value {value:?}"),
                    })?;
                if let Some((_, pending)) = self
                    .current_part
                    .as_mut()
                    .and_then(|part| part.pending_time.as_mut())
                {
                    *pending = Some(beat_type);
                }
            }
            "chromatic" => {
                let chromatic = value
                    .parse::<i16>()
                    .map_err(|_| ImportError::InvalidTiming {
                        context: self.context_label(),
                        message: format!("invalid chromatic transposition {value:?}"),
                    })?;
                if let Some((pending, _)) = self
                    .current_part
                    .as_mut()
                    .and_then(|part| part.pending_transpose.as_mut())
                {
                    *pending = chromatic;
                }
            }
            "octave-change" => {
                let octaves = value
                    .parse::<i16>()
                    .map_err(|_| ImportError::InvalidTiming {
                        context: self.context_label(),
                        message: format!("invalid octave transposition {value:?}"),
                    })?;
                if let Some((_, pending)) = self
                    .current_part
                    .as_mut()
                    .and_then(|part| part.pending_transpose.as_mut())
                {
                    *pending = octaves;
                }
            }
            "duration" => {
                let duration = value
                    .parse::<i64>()
                    .ok()
                    .filter(|duration| *duration >= 0)
                    .ok_or_else(|| ImportError::InvalidTiming {
                        context: self.context_label(),
                        message: format!("invalid duration {value:?}"),
                    })?;
                if let Some((_, movement_duration)) = &mut self.movement {
                    *movement_duration = Some(duration);
                } else if let Some(note) = &mut self.current_note {
                    note.duration_units = Some(duration);
                }
            }
            "voice" => {
                if let Some(note) = &mut self.current_note {
                    note.voice = value.to_owned();
                }
            }
            "staff" => {
                if let Some(note) = &mut self.current_note {
                    note.staff = value
                        .parse::<u16>()
                        .ok()
                        .filter(|staff| *staff > 0)
                        .unwrap_or(1);
                }
            }
            "step" if self.inside("pitch") => {
                if let Some(note) = &mut self.current_note {
                    note.step = match value {
                        "C" => Some(Step::C),
                        "D" => Some(Step::D),
                        "E" => Some(Step::E),
                        "F" => Some(Step::F),
                        "G" => Some(Step::G),
                        "A" => Some(Step::A),
                        "B" => Some(Step::B),
                        _ => None,
                    };
                }
            }
            "alter" if self.inside("pitch") => {
                if let Some(note) = &mut self.current_note {
                    note.alter = Rational::parse_decimal(value)?;
                }
            }
            "octave" if self.inside("pitch") => {
                if let Some(note) = &mut self.current_note {
                    note.octave = value.parse::<i8>().ok();
                }
            }
            "mute" if self.inside("play") => {
                if let Some(note) = &mut self.current_note {
                    note.muted = value.eq_ignore_ascii_case("yes") || value == "1";
                }
            }
            "words" if self.inside("direction") => self.reject_navigation_words(value)?,
            _ => {}
        }
        Ok(())
    }

    fn end(&mut self, tag: &str) -> Result<(), ImportError> {
        match tag {
            "score-part" => self.current_definition = None,
            "creator" => self.creator_is_composer = false,
            "time" => {
                if let Some(part) = &mut self.current_part {
                    if let Some((Some(beats), Some(beat_type))) = part.pending_time.take() {
                        part.time = Some((beats, beat_type));
                    }
                }
            }
            "transpose" => {
                if let Some(part) = &mut self.current_part {
                    if let Some((chromatic, octaves)) = part.pending_transpose.take() {
                        part.transpose = chromatic.saturating_add(octaves.saturating_mul(12));
                    }
                }
            }
            "note" => self.finish_note()?,
            "backup" | "forward" => self.finish_movement()?,
            "measure" => self.finish_measure()?,
            "part" => self.finish_part(),
            _ => {}
        }
        Ok(())
    }

    fn inside(&self, tag: &str) -> bool {
        self.path.iter().any(|ancestor| ancestor == tag)
    }

    fn context_label(&self) -> String {
        let part = self
            .current_part
            .as_ref()
            .map(|part| part.id.as_str())
            .unwrap_or("?");
        let measure = self
            .current_measure
            .as_ref()
            .map(|measure| measure.id.as_str())
            .unwrap_or("?");
        format!("part {part}, measure {measure}")
    }

    fn reject_navigation_attributes(&self, element: &BytesStart<'_>) -> Result<(), ImportError> {
        for construct in ["dacapo", "dalsegno", "tocoda", "fine"] {
            if attribute(element, construct)?.is_some_and(|value| {
                !value.is_empty() && !value.eq_ignore_ascii_case("no") && value != "0"
            }) {
                return Err(ImportError::UnsupportedNavigation {
                    context: self.context_label(),
                    construct: format!("MusicXML sound/{construct}"),
                });
            }
        }
        Ok(())
    }

    fn reject_navigation_words(&self, words: &str) -> Result<(), ImportError> {
        let words = words.to_ascii_lowercase();
        if [
            "d.c",
            "d. c",
            "da capo",
            "d.s",
            "d. s",
            "dal segno",
            "to coda",
        ]
        .iter()
        .any(|needle| words.contains(needle))
        {
            Err(ImportError::UnsupportedNavigation {
                context: self.context_label(),
                construct: "textual D.C./D.S./Coda instruction".to_owned(),
            })
        } else {
            Ok(())
        }
    }

    fn finish_movement(&mut self) -> Result<(), ImportError> {
        let Some((kind, duration_units)) = self.movement.take() else {
            return Ok(());
        };
        let duration_units = duration_units.ok_or_else(|| ImportError::InvalidTiming {
            context: self.context_label(),
            message: format!("{kind:?} has no duration"),
        })?;
        let divisions = self
            .current_part
            .as_ref()
            .map(|part| part.divisions)
            .unwrap_or(1);
        let duration = Rational::new(duration_units, divisions)?;
        let context = self.context_label();
        let measure = self
            .current_measure
            .as_mut()
            .ok_or_else(|| ImportError::InvalidTiming {
                context: context.clone(),
                message: "backup/forward appears outside a measure".to_owned(),
            })?;
        match kind {
            MovementKind::Backup => {
                let next = measure.cursor.checked_sub(duration)?;
                if next.is_negative() {
                    return Err(ImportError::InvalidTiming {
                        context,
                        message: "backup moves before the beginning of the measure".to_owned(),
                    });
                }
                measure.cursor = next;
            }
            MovementKind::Forward => {
                measure.cursor = measure.cursor.checked_add(duration)?;
                measure.max_cursor = measure.max_cursor.max(measure.cursor);
            }
        }
        Ok(())
    }

    fn finish_note(&mut self) -> Result<(), ImportError> {
        let note = self.current_note.take().ok_or_else(|| {
            ImportError::InvalidXml("closing note without an open note".to_owned())
        })?;
        let part_id = self
            .current_part
            .as_ref()
            .map(|part| part.id.clone())
            .ok_or_else(|| ImportError::InvalidXml("note appears outside a part".to_owned()))?;
        let measure = self
            .current_measure
            .as_mut()
            .ok_or_else(|| ImportError::InvalidXml("note appears outside a measure".to_owned()))?;
        measure.note_ordinal += 1;

        let duration = if note.grace && note.duration_units.is_none() {
            Rational::ZERO
        } else {
            let units = note
                .duration_units
                .ok_or_else(|| ImportError::InvalidTiming {
                    context: format!("part {part_id}, measure {}", measure.id),
                    message: "non-grace note has no duration".to_owned(),
                })?;
            Rational::new(units, note.divisions)?
        };
        let onset = if note.chord {
            measure
                .last_onset_by_voice
                .get(&note.voice)
                .copied()
                .ok_or_else(|| ImportError::InvalidTiming {
                    context: format!("part {part_id}, measure {}", measure.id),
                    message: format!("chord note in voice {} has no preceding note", note.voice),
                })?
        } else {
            let onset = measure.cursor;
            measure
                .last_onset_by_voice
                .insert(note.voice.clone(), onset);
            measure.cursor = measure.cursor.checked_add(duration)?;
            measure.max_cursor = measure.max_cursor.max(measure.cursor);
            onset
        };
        let note_end = onset.checked_add(duration)?;
        measure.max_cursor = measure.max_cursor.max(note_end);

        let generated_id = if let Some(xml_id) = &note.xml_id {
            format!("{part_id}:{xml_id}")
        } else {
            format!(
                "{part_id}/measure:{}/note:{}",
                measure.index + 1,
                measure.note_ordinal
            )
        };
        let source_id = unique_id(&mut self.source_ids, generated_id);
        let context = SourceContext {
            part_id: Some(part_id.clone()),
            measure_id: Some(measure.id.clone()),
            measure_index: Some(measure.index),
            source_id: Some(source_id.clone()),
        };

        if note.rest {
            return Ok(());
        }
        if note.grace {
            self.warnings.push(ImportWarning::info(
                WarningCode::GraceNoteSkipped,
                "grace note is displayed but does not consume a tap in the MVP",
                context,
            ));
            return Ok(());
        }
        if note.cue && !self.options.include_cue_notes {
            self.warnings.push(ImportWarning::info(
                WarningCode::CueNoteSkipped,
                "cue note was excluded by import policy",
                context,
            ));
            return Ok(());
        }
        if note.hidden && !self.options.include_hidden_notes {
            self.warnings.push(ImportWarning::info(
                WarningCode::HiddenNoteSkipped,
                "note with print-object=\"no\" was excluded by import policy",
                context,
            ));
            return Ok(());
        }
        if note.muted {
            self.warnings.push(ImportWarning::info(
                WarningCode::HiddenNoteSkipped,
                "note explicitly muted by MusicXML playback data was excluded",
                context,
            ));
            return Ok(());
        }
        if note.unpitched {
            self.warnings.push(ImportWarning::warning(
                WarningCode::UnpitchedNoteSkipped,
                "unpitched percussion is not playable in piano mode",
                context,
            ));
            return Ok(());
        }

        let (Some(step), Some(octave)) = (note.step, note.octave) else {
            self.warnings.push(ImportWarning::warning(
                WarningCode::MissingPitch,
                "pitched note is missing a valid step or octave",
                context,
            ));
            return Ok(());
        };
        if note.alter.denominator() != 1 {
            self.warnings.push(ImportWarning::warning(
                WarningCode::MicrotonalPitchSkipped,
                "microtonal pitch cannot be represented by the MVP piano/MIDI 1.0 engine",
                context,
            ));
            return Ok(());
        }
        let midi_pitch = i16::from(octave)
            .saturating_add(1)
            .saturating_mul(12)
            .saturating_add(step.semitone())
            .saturating_add(note.alter.numerator() as i16)
            .saturating_add(note.transpose);
        let Ok(midi_pitch) = u8::try_from(midi_pitch) else {
            self.warnings.push(ImportWarning::warning(
                WarningCode::PitchOutOfRange,
                "concert pitch is outside the MIDI note range 0..=127",
                context,
            ));
            return Ok(());
        };
        if midi_pitch > 127 {
            self.warnings.push(ImportWarning::warning(
                WarningCode::PitchOutOfRange,
                "concert pitch is outside the MIDI note range 0..=127",
                context,
            ));
            return Ok(());
        }

        measure.notes.push(RawNote {
            source_id,
            xml_id: note.xml_id,
            measure_index: measure.index,
            order: measure.note_ordinal,
            onset,
            duration,
            staff: note.staff,
            voice: note.voice,
            written_pitch: SpelledPitch {
                step,
                alter: note.alter,
                octave,
            },
            midi_pitch,
            tie_start: note.tie_start,
            tie_stop: note.tie_stop,
            tie_number: note.tie_number,
        });
        Ok(())
    }

    fn finish_measure(&mut self) -> Result<(), ImportError> {
        let measure = self.current_measure.take().ok_or_else(|| {
            ImportError::InvalidXml("closing measure without an open measure".to_owned())
        })?;
        let part = self
            .current_part
            .as_mut()
            .ok_or_else(|| ImportError::InvalidXml("measure appears outside a part".to_owned()))?;

        let expected = if let Some((beats, beat_type)) = part.time {
            Rational::new(i64::from(beats).saturating_mul(4), i64::from(beat_type))?
        } else {
            Rational::ZERO
        };
        if !measure.implicit && !expected.is_zero() && measure.max_cursor > expected {
            self.warnings.push(ImportWarning::warning(
                WarningCode::OverfullMeasure,
                format!(
                    "actual extent {} exceeds the time-signature extent {}",
                    measure.max_cursor, expected
                ),
                SourceContext {
                    part_id: Some(part.id.clone()),
                    measure_id: Some(measure.id.clone()),
                    measure_index: Some(measure.index),
                    source_id: None,
                },
            ));
        }
        let duration = if measure.implicit {
            if measure.max_cursor.is_zero() {
                expected
            } else {
                measure.max_cursor
            }
        } else {
            measure.max_cursor.max(expected)
        };

        let mut endings = measure.inherited_endings.clone();
        endings.extend(measure.ending_starts.iter().copied());
        endings.extend(measure.ending_stops.iter().copied());

        part.active_endings
            .extend(measure.ending_starts.iter().copied());
        if measure.clear_endings {
            part.active_endings.clear();
        } else {
            for number in &measure.ending_stops {
                part.active_endings.remove(number);
            }
        }
        part.measures.push(ParsedMeasure {
            id: measure.id,
            duration,
            time: part.time.unwrap_or((4, 4)),
            notes: measure.notes,
            repeat_forward: measure.repeat_forward,
            repeat_backward: measure.repeat_backward,
            endings,
        });
        Ok(())
    }

    fn finish_part(&mut self) {
        if let Some(part) = self.current_part.take() {
            if part.measures.is_empty() {
                self.warnings.push(ImportWarning::warning(
                    WarningCode::EmptyPart,
                    "part contains no measures",
                    SourceContext {
                        part_id: Some(part.id.clone()),
                        ..SourceContext::default()
                    },
                ));
            }
            self.parts.push(ParsedPart {
                id: part.id,
                order: part.order,
                measures: part.measures,
            });
        }
    }

    fn finish(mut self) -> Result<NormalizedScore, ImportError> {
        if self.current_note.is_some()
            || self.current_measure.is_some()
            || self.current_part.is_some()
        {
            return Err(ImportError::InvalidXml(
                "document ended with an unclosed part, measure, or note".to_owned(),
            ));
        }
        if self.parts.is_empty() {
            return Err(ImportError::InvalidXml(
                "score contains no parts".to_owned(),
            ));
        }

        let parts = self
            .parts
            .iter()
            .map(|part| {
                let definition = self.definitions.get(&part.id).cloned().unwrap_or_default();
                PartInfo {
                    id: part.id.clone(),
                    name: definition.name.unwrap_or_else(|| part.id.clone()),
                    abbreviation: definition.abbreviation,
                    order: part.order,
                }
            })
            .collect();
        let templates = build_measure_templates(&self.parts, &mut self.warnings)?;
        let playback = expand_playback(&templates, self.options.max_playback_measures)?;
        let expanded = expand_notes(&self.parts, &templates, &playback)?;
        let attacks = resolve_ties(expanded, &mut self.warnings);

        let mut normalized = NormalizedScore::new(
            ScoreFormat::MusicXml,
            self.metadata,
            parts,
            attacks,
            playback.len(),
            self.warnings,
        );
        normalized.playback_measures = playback
            .iter()
            .map(|measure| {
                let template = &templates[measure.source_index];
                crate::PlaybackMeasureInfo {
                    source_measure_index: measure.source_index,
                    measure_id: template.id.clone(),
                    occurrence: measure.occurrence,
                    start: measure.start,
                    duration: template.duration,
                    beats: template.time.0,
                    beat_type: template.time.1,
                }
            })
            .collect();
        Ok(normalized)
    }
}

fn parse_positive_i64(value: &str, field: &str) -> Result<i64, ImportError> {
    value
        .parse::<i64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| ImportError::InvalidTiming {
            context: field.to_owned(),
            message: format!("expected a positive integer, found {value:?}"),
        })
}

fn parse_ending_numbers(value: Option<&str>) -> BTreeSet<u32> {
    let mut numbers = BTreeSet::new();
    for component in value
        .unwrap_or_default()
        .split(|character: char| character == ',' || character.is_whitespace())
    {
        if let Some((start, end)) = component.split_once('-') {
            if let (Ok(start), Ok(end)) = (start.parse::<u32>(), end.parse::<u32>()) {
                if start <= end && end.saturating_sub(start) <= 32 {
                    numbers.extend(start..=end);
                }
            }
        } else if let Ok(number) = component.parse::<u32>() {
            numbers.insert(number);
        }
    }
    numbers
}

fn unique_id(used: &mut BTreeSet<String>, base: String) -> String {
    if used.insert(base.clone()) {
        return base;
    }
    for suffix in 2_u32.. {
        let candidate = format!("{base}#{suffix}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!("u32 source ID suffix space exhausted")
}

#[derive(Clone)]
struct MeasureTemplate {
    id: String,
    duration: Rational,
    time: (u32, u32),
    repeat_forward: bool,
    repeat_backward: Option<u32>,
    endings: BTreeSet<u32>,
}

fn build_measure_templates(
    parts: &[ParsedPart],
    warnings: &mut Vec<ImportWarning>,
) -> Result<Vec<MeasureTemplate>, ImportError> {
    let measure_count = parts
        .iter()
        .map(|part| part.measures.len())
        .max()
        .unwrap_or(0);
    let mut templates = Vec::with_capacity(measure_count);
    for index in 0..measure_count {
        let measures: Vec<(&ParsedPart, &ParsedMeasure)> = parts
            .iter()
            .filter_map(|part| part.measures.get(index).map(|measure| (part, measure)))
            .collect();
        let id = measures
            .first()
            .map(|(_, measure)| measure.id.clone())
            .unwrap_or_else(|| (index + 1).to_string());
        let duration = measures
            .iter()
            .map(|(_, measure)| measure.duration)
            .max()
            .unwrap_or(Rational::ZERO);
        for (part, measure) in &measures {
            if !measure.duration.is_zero() && measure.duration != duration {
                warnings.push(ImportWarning::warning(
                    WarningCode::InconsistentMeasureDuration,
                    format!(
                        "part extent {} differs from aligned measure extent {}",
                        measure.duration, duration
                    ),
                    SourceContext {
                        part_id: Some(part.id.clone()),
                        measure_id: Some(measure.id.clone()),
                        measure_index: Some(index),
                        source_id: None,
                    },
                ));
            }
        }
        templates.push(MeasureTemplate {
            id,
            duration,
            time: measures.first().map(|(_, measure)| measure.time).unwrap_or((4, 4)),
            repeat_forward: measures.iter().any(|(_, measure)| measure.repeat_forward),
            repeat_backward: measures
                .iter()
                .filter_map(|(_, measure)| measure.repeat_backward)
                .max(),
            endings: measures
                .iter()
                .flat_map(|(_, measure)| measure.endings.iter().copied())
                .collect(),
        });
    }
    Ok(templates)
}

#[derive(Clone, Copy, Debug)]
struct RepeatSection {
    start: usize,
    end: usize,
    times: u32,
}

#[derive(Clone)]
struct PlaybackMeasure {
    source_index: usize,
    occurrence: u32,
    start: Rational,
}

fn expand_playback(
    measures: &[MeasureTemplate],
    max_playback_measures: usize,
) -> Result<Vec<PlaybackMeasure>, ImportError> {
    let mut repeat_stack = Vec::new();
    let mut sections = Vec::new();
    for (index, measure) in measures.iter().enumerate() {
        if measure.repeat_forward {
            repeat_stack.push(index);
        }
        if let Some(times) = measure.repeat_backward {
            sections.push(RepeatSection {
                start: repeat_stack.pop().unwrap_or(0),
                end: index,
                times,
            });
        }
    }
    sections.sort_by_key(|section| (section.start, section.end));

    let mut result = Vec::new();
    let mut occurrence_counts = vec![0_u32; measures.len()];
    let mut active_passes: BTreeMap<usize, u32> = BTreeMap::new();
    let mut pc = 0_usize;
    let mut playback_start = Rational::ZERO;
    let mut last_repeat_pass = None;
    let mut steps = 0_usize;
    let max_steps = max_playback_measures.saturating_mul(8).max(1024);

    while pc < measures.len() {
        steps = steps.saturating_add(1);
        if steps > max_steps {
            return Err(ImportError::ResourceLimit {
                kind: "repeat expansion steps",
                limit: max_steps,
            });
        }
        for section in sections.iter().filter(|section| section.start == pc) {
            active_passes.entry(section.end).or_insert(1);
        }
        let active_section = sections
            .iter()
            .filter(|section| {
                section.start <= pc && pc <= section.end && active_passes.contains_key(&section.end)
            })
            .max_by_key(|section| section.start);
        let pass = active_section
            .and_then(|section| active_passes.get(&section.end).copied())
            .or(last_repeat_pass)
            .unwrap_or(1);
        let include = measures[pc].endings.is_empty() || measures[pc].endings.contains(&pass);
        if include {
            if result.len() >= max_playback_measures {
                return Err(ImportError::ResourceLimit {
                    kind: "expanded playback measure count",
                    limit: max_playback_measures,
                });
            }
            occurrence_counts[pc] = occurrence_counts[pc].saturating_add(1);
            result.push(PlaybackMeasure {
                source_index: pc,
                occurrence: occurrence_counts[pc],
                start: playback_start,
            });
            playback_start = playback_start.checked_add(measures[pc].duration)?;
        }

        if let Some(section) = sections
            .iter()
            .filter(|section| section.end == pc && active_passes.contains_key(&section.end))
            .max_by_key(|section| section.start)
            .copied()
        {
            let current_pass = active_passes.get(&section.end).copied().unwrap_or(1);
            if current_pass < section.times {
                active_passes.insert(section.end, current_pass + 1);
                last_repeat_pass = None;
                pc = section.start;
                continue;
            }
            active_passes.remove(&section.end);
            last_repeat_pass = Some(current_pass);
        }

        if measures[pc].endings.is_empty() && last_repeat_pass.is_some() {
            last_repeat_pass = None;
        }
        pc += 1;
    }
    Ok(result)
}

#[derive(Clone)]
struct ExpandedNote {
    playback_order: usize,
    source_anchor: SourceAnchor,
    part_index: usize,
    written_pitch: SpelledPitch,
    midi_pitch: u8,
    onset: Rational,
    end: Rational,
    tie_start: bool,
    tie_stop: bool,
    tie_number: Option<u8>,
    source_order: usize,
}

fn expand_notes(
    parts: &[ParsedPart],
    templates: &[MeasureTemplate],
    playback: &[PlaybackMeasure],
) -> Result<Vec<ExpandedNote>, ImportError> {
    let estimated: usize = playback
        .iter()
        .map(|occurrence| {
            parts
                .iter()
                .filter_map(|part| part.measures.get(occurrence.source_index))
                .map(|measure| measure.notes.len())
                .sum::<usize>()
        })
        .sum();
    let mut notes = Vec::with_capacity(estimated);
    for (playback_order, occurrence) in playback.iter().enumerate() {
        for part in parts {
            let Some(measure) = part.measures.get(occurrence.source_index) else {
                continue;
            };
            for note in &measure.notes {
                let onset = occurrence.start.checked_add(note.onset)?;
                notes.push(ExpandedNote {
                    playback_order,
                    source_anchor: SourceAnchor {
                        source_id: note.source_id.clone(),
                        xml_id: note.xml_id.clone(),
                        part_id: part.id.clone(),
                        measure_id: templates[occurrence.source_index].id.clone(),
                        measure_index: note.measure_index,
                        occurrence: occurrence.occurrence,
                        offset: note.onset,
                        staff: note.staff,
                        voice: note.voice.clone(),
                    },
                    part_index: part.order,
                    written_pitch: note.written_pitch.clone(),
                    midi_pitch: note.midi_pitch,
                    onset,
                    end: onset.checked_add(note.duration)?,
                    tie_start: note.tie_start,
                    tie_stop: note.tie_stop,
                    tie_number: note.tie_number,
                    source_order: note.order,
                });
            }
        }
    }
    notes.sort_by(|left, right| {
        left.playback_order
            .cmp(&right.playback_order)
            .then(left.onset.cmp(&right.onset))
            .then(left.part_index.cmp(&right.part_index))
            .then(left.source_order.cmp(&right.source_order))
    });
    Ok(notes)
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct TieKey {
    part_id: String,
    staff: u16,
    voice: String,
    pitch: u8,
    number: Option<u8>,
}

fn resolve_ties(notes: Vec<ExpandedNote>, warnings: &mut Vec<ImportWarning>) -> Vec<NoteAttack> {
    let mut attacks: Vec<NoteAttack> = Vec::new();
    let mut open: BTreeMap<TieKey, usize> = BTreeMap::new();
    for note in notes {
        let key = TieKey {
            part_id: note.source_anchor.part_id.clone(),
            staff: note.source_anchor.staff,
            voice: note.source_anchor.voice.clone(),
            pitch: note.midi_pitch,
            number: note.tie_number,
        };

        if note.tie_stop {
            if let Some(index) = open.get(&key).copied() {
                let attack = &mut attacks[index];
                attack.end = attack.end.max(note.end);
                attack.tie.continuations.push(note.source_anchor.clone());
                if !note.tie_start {
                    open.remove(&key);
                }
                continue;
            }
            warnings.push(ImportWarning::warning(
                WarningCode::UnmatchedTieStop,
                "tie stop has no matching earlier attack; it was treated as a new attack",
                context_from_anchor(&note.source_anchor),
            ));
        }

        let index = attacks.len();
        attacks.push(NoteAttack {
            source_id: note.source_anchor.source_id.clone(),
            source_anchor: note.source_anchor.clone(),
            part_index: note.part_index,
            staff: note.source_anchor.staff,
            voice: note.source_anchor.voice.clone(),
            written_pitch: Some(note.written_pitch),
            midi_pitch: note.midi_pitch,
            midi_channel: None,
            onset: note.onset,
            end: note.end,
            tie: TieInfo {
                starts_tie: note.tie_start,
                continuations: Vec::new(),
            },
            velocity_hint: None,
        });
        if note.tie_start {
            if let Some(replaced) = open.insert(key, index) {
                warnings.push(ImportWarning::warning(
                    WarningCode::ReplacedOpenTie,
                    "a new tie start replaced an unterminated tie of the same pitch and voice",
                    context_from_anchor(&attacks[replaced].source_anchor),
                ));
            }
        }
    }

    for index in open.values().copied() {
        warnings.push(ImportWarning::warning(
            WarningCode::UnterminatedTie,
            "tie start has no matching stop in expanded playback order",
            context_from_anchor(&attacks[index].source_anchor),
        ));
    }
    attacks
}

fn context_from_anchor(anchor: &SourceAnchor) -> SourceContext {
    SourceContext {
        part_id: Some(anchor.part_id.clone()),
        measure_id: Some(anchor.measure_id.clone()),
        measure_index: Some(anchor.measure_index),
        source_id: Some(anchor.source_id.clone()),
    }
}
