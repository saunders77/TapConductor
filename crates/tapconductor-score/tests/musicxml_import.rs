// Copyright (c) 2026 Michael Saunders
use std::{collections::BTreeSet, path::PathBuf};

use serde_json::json;
use tapconductor_score::{
    import_musicxml, import_path, ImportError, ImportOptions, Rational, WarningCode,
};

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    std::fs::read(path).unwrap()
}

#[test]
fn groups_chords_voices_staves_and_parts_by_exact_position() {
    let score =
        import_musicxml(&fixture("cross_parts.musicxml"), &ImportOptions::default()).unwrap();

    let summary: Vec<_> = score
        .tap_events
        .iter()
        .map(|event| {
            json!({
                "position": event.position.absolute.to_string(),
                "pitches": event.attacks.iter().map(|attack| attack.midi_pitch).collect::<Vec<_>>()
            })
        })
        .collect();
    let expected: serde_json::Value =
        serde_json::from_slice(&fixture("cross_parts_events.json")).unwrap();
    assert_eq!(serde_json::Value::Array(summary), expected);

    assert_eq!(
        score.metadata.title.as_deref(),
        Some("Cross-part exact timing")
    );
    assert_eq!(
        score.metadata.composer.as_deref(),
        Some("TapConductor Test")
    );
    assert_eq!(score.parts[0].abbreviation.as_deref(), Some("Pno."));
    assert_eq!(
        score.tap_events[3].position.absolute,
        Rational::new(13, 3).unwrap()
    );
    assert_eq!(score.tap_events[0].position.absolute, Rational::ZERO);
}

#[test]
fn import_path_auto_detects_repository_fixture() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cross_parts.musicxml");
    let score = import_path(path, &ImportOptions::default()).unwrap();
    assert_eq!(score.tap_events.len(), 4);
}

#[test]
fn active_part_filtering_rebuilds_positions_without_rounding() {
    let score =
        import_musicxml(&fixture("cross_parts.musicxml"), &ImportOptions::default()).unwrap();
    let piano = BTreeSet::from(["P1".to_owned()]);
    let events = score.tap_events_for_parts(&piano).unwrap();
    // The quarter-note position at 1 belongs only to P2 and therefore disappears entirely.
    assert_eq!(events.len(), 3);
    assert_eq!(
        events[0]
            .attacks
            .iter()
            .map(|attack| attack.midi_pitch)
            .collect::<Vec<_>>(),
        vec![60, 64]
    );
}

#[test]
fn tied_continuation_does_not_create_a_tap_and_extends_initial_attack() {
    let score = import_musicxml(&fixture("ties.musicxml"), &ImportOptions::default()).unwrap();
    assert_eq!(
        score
            .tap_events
            .iter()
            .map(|event| event.attacks[0].midi_pitch)
            .collect::<Vec<_>>(),
        vec![60, 62, 64]
    );
    assert!(!score
        .tap_events
        .iter()
        .any(|event| event.position.absolute == Rational::from_integer(2)));

    let tied = score
        .attacks
        .iter()
        .find(|attack| attack.midi_pitch == 60)
        .unwrap();
    assert_eq!(tied.end, Rational::from_integer(3));
    assert_eq!(tied.tie.continuations.len(), 1);
    assert!(tied.tie.continuations[0].source_id.ends_with("tie-stop"));
}

#[test]
fn expands_basic_repeat_with_first_and_second_endings() {
    let score = import_musicxml(
        &fixture("repeats_endings.musicxml"),
        &ImportOptions::default(),
    )
    .unwrap();
    assert_eq!(score.playback_measure_count, 4);
    assert_eq!(
        score
            .tap_events
            .iter()
            .map(|event| event.attacks[0].midi_pitch)
            .collect::<Vec<_>>(),
        vec![60, 62, 60, 64]
    );
    assert_eq!(score.tap_events[0].position.occurrence, 1);
    assert_eq!(score.tap_events[2].position.occurrence, 2);
    assert_ne!(score.tap_events[0].id, score.tap_events[2].id);
}

#[test]
fn transposes_to_concert_pitch_and_reports_policy_skips() {
    let score = import_musicxml(
        &fixture("import_policy.musicxml"),
        &ImportOptions::default(),
    )
    .unwrap();
    assert_eq!(score.attacks.len(), 2);
    assert_eq!(score.attacks[0].midi_pitch, 58);
    assert_eq!(score.attacks[1].midi_pitch, 63);
    let codes: BTreeSet<_> = score.warnings.iter().map(|warning| warning.code).collect();
    assert!(codes.contains(&WarningCode::CueNoteSkipped));
    assert!(codes.contains(&WarningCode::HiddenNoteSkipped));
    assert!(!codes.contains(&WarningCode::GraceNoteSkipped));
    assert!(codes.contains(&WarningCode::UnpitchedNoteSkipped));

    let options = ImportOptions {
        include_cue_notes: true,
        include_hidden_notes: true,
        ..ImportOptions::default()
    };
    let included = import_musicxml(&fixture("import_policy.musicxml"), &options).unwrap();
    assert_eq!(
        included
            .attacks
            .iter()
            .map(|attack| attack.midi_pitch)
            .collect::<Vec<_>>(),
        vec![58, 60, 62, 63]
    );
}

#[test]
fn grace_note_groups_are_distinct_tap_moments_before_the_principal_note() {
    let xml = br#"
        <score-partwise>
          <part-list><score-part id="P"><part-name>Piano</part-name></score-part></part-list>
          <part id="P"><measure number="1">
            <attributes><divisions>1</divisions></attributes>
            <note id="grace-c"><grace/><pitch><step>C</step><octave>4</octave></pitch></note>
            <note id="grace-d"><grace/><pitch><step>D</step><octave>4</octave></pitch></note>
            <note id="grace-e"><chord/><grace/><pitch><step>E</step><octave>4</octave></pitch></note>
            <note id="principal"><pitch><step>F</step><octave>4</octave></pitch><duration>1</duration></note>
          </measure></part>
        </score-partwise>"#;
    let score = import_musicxml(xml, &ImportOptions::default()).unwrap();

    assert_eq!(score.tap_events.len(), 3);
    assert!(score
        .tap_events
        .iter()
        .all(|event| event.position.absolute == Rational::ZERO));
    assert_eq!(
        score
            .tap_events
            .iter()
            .map(|event| event.position.position_order)
            .collect::<Vec<_>>(),
        vec![0, 1, u32::MAX],
    );
    assert_eq!(
        score
            .tap_events
            .iter()
            .map(|event| event
                .attacks
                .iter()
                .map(|attack| attack.midi_pitch)
                .collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        vec![vec![60], vec![62, 64], vec![65]],
    );
    assert_eq!(score.playback_measures[0].duration, Rational::ONE);
}

#[test]
fn numbers_an_implicit_half_bar_pickup_as_the_last_two_beats() {
    let xml = br#"
        <score-partwise>
          <part-list><score-part id="P"><part-name>Piano</part-name></score-part></part-list>
          <part id="P">
            <measure number="0" implicit="yes">
              <attributes><divisions>1</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
              <note><pitch><step>C</step><octave>4</octave></pitch><duration>1</duration></note>
              <note><pitch><step>D</step><octave>4</octave></pitch><duration>1</duration></note>
            </measure>
            <measure number="1">
              <note><pitch><step>E</step><octave>4</octave></pitch><duration>4</duration></note>
            </measure>
          </part>
        </score-partwise>"#;
    let score = import_musicxml(xml, &ImportOptions::default()).unwrap();
    let beats = score.playback_beats();

    assert_eq!(
        beats
            .iter()
            .take(4)
            .map(|beat| (beat.absolute, beat.beat_index))
            .collect::<Vec<_>>(),
        vec![
            (Rational::ZERO, 2),
            (Rational::ONE, 3),
            (Rational::from_integer(2), 0),
            (Rational::from_integer(3), 1),
        ]
    );
}

#[test]
fn bundled_rachmaninoff_starts_on_beat_three_after_a_six_beat_count_in() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("assets")
        .join("demo")
        .join("All-Night Vigil - Rachmaninoff 1915.musicxml");
    let score = import_path(&path, &ImportOptions::default()).unwrap();
    let beats = score.playback_beats();

    assert_eq!(score.tap_events[0].position.absolute, Rational::ZERO);
    assert_eq!(beats[0].absolute, Rational::ZERO);
    assert_eq!(beats[0].beat_index, 2);
    assert_eq!(beats[0].beats_in_measure + beats[0].beat_index, 6);
    assert_eq!(beats[1].beat_index, 3);
    assert_eq!(beats[2].absolute, Rational::from_integer(2));
    assert_eq!(beats[2].beat_index, 0);
}

#[test]
fn imports_staccato_articulation_on_the_attack() {
    let xml = br#"
        <score-partwise>
          <part-list><score-part id="P"><part-name>Piano</part-name></score-part></part-list>
          <part id="P"><measure number="1">
            <attributes><divisions>1</divisions></attributes>
            <note id="staccato-c">
              <pitch><step>C</step><octave>4</octave></pitch><duration>1</duration>
              <notations><articulations><staccato/></articulations></notations>
            </note>
            <note id="plain-d">
              <pitch><step>D</step><octave>4</octave></pitch><duration>1</duration>
            </note>
          </measure></part>
        </score-partwise>"#;
    let score = import_musicxml(xml, &ImportOptions::default()).unwrap();

    assert!(score.attacks[0].staccato);
    assert!(!score.attacks[1].staccato);
}

#[test]
fn fails_closed_on_timing_that_moves_before_measure_start() {
    let xml = br#"
        <score-partwise>
          <part-list><score-part id="P"><part-name>P</part-name></score-part></part-list>
          <part id="P"><measure number="1">
            <attributes><divisions>1</divisions></attributes>
            <backup><duration>1</duration></backup>
          </measure></part>
        </score-partwise>"#;
    assert!(matches!(
        import_musicxml(xml, &ImportOptions::default()),
        Err(ImportError::InvalidTiming { .. })
    ));
}

#[test]
fn fails_closed_on_unimplemented_navigation() {
    let xml = br#"
        <score-partwise>
          <part-list><score-part id="P"><part-name>P</part-name></score-part></part-list>
          <part id="P"><measure number="1"><sound dacapo="yes"/></measure></part>
        </score-partwise>"#;
    assert!(matches!(
        import_musicxml(xml, &ImportOptions::default()),
        Err(ImportError::UnsupportedNavigation { .. })
    ));
}

#[test]
fn imports_ottava_and_keeps_the_musicxml_sounding_pitch() {
    let xml = br#"
        <score-partwise>
          <part-list><score-part id="P"><part-name>P</part-name></score-part></part-list>
          <part id="P"><measure number="1">
            <attributes><divisions>1</divisions></attributes>
            <direction><direction-type><octave-shift type="down" size="8"/></direction-type></direction>
            <note><pitch><step>C</step><octave>6</octave></pitch><duration>1</duration></note>
            <direction><direction-type><octave-shift type="stop" size="8"/></direction-type></direction>
          </measure></part>
        </score-partwise>"#;
    let score = import_musicxml(xml, &ImportOptions::default()).unwrap();
    assert_eq!(
        score.tap_events[0].attacks[0].midi_pitch, 84,
        "MusicXML pitch is already the performed pitch; octave-shift is visual notation",
    );
}

#[test]
fn enforces_xml_depth_limit() {
    let options = ImportOptions {
        max_xml_depth: 2,
        ..ImportOptions::default()
    };
    assert!(matches!(
        import_musicxml(b"<score-partwise><a><b/></a></score-partwise>", &options),
        Err(ImportError::ResourceLimit { .. })
    ));
}

#[test]
fn accepts_standard_musicxml_doctype_without_enabling_entities() {
    let standard = br#"<?xml version="1.0" encoding="UTF-8"?>
        <!DOCTYPE score-partwise PUBLIC "-//Recordare//DTD MusicXML 4.0 Partwise//EN" "http://www.musicxml.org/dtds/partwise.dtd">
        <score-partwise>
          <part-list><score-part id="P"><part-name>Piano</part-name></score-part></part-list>
          <part id="P"><measure number="1"><attributes><divisions>1</divisions></attributes>
            <note><pitch><step>C</step><octave>4</octave></pitch><duration>1</duration></note>
          </measure></part>
        </score-partwise>"#;
    let score = import_musicxml(standard, &ImportOptions::default()).unwrap();
    assert_eq!(score.tap_events.len(), 1);

    let internal_entity = br#"<!DOCTYPE score-partwise [<!ENTITY xxe SYSTEM "file:///etc/passwd">]>
        <score-partwise/>"#;
    let error = import_musicxml(internal_entity, &ImportOptions::default()).unwrap_err();
    assert!(matches!(error, ImportError::InvalidXml(_)));
    assert!(error.to_string().contains("internal subsets"));
}
