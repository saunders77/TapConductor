use std::collections::BTreeSet;

use tapconductor_score::{
    display_musicxml_text, import_midi, ImportError, ImportOptions, Rational, ScoreFormat,
    WarningCode,
};

fn push_track(file: &mut Vec<u8>, events: &[u8]) {
    file.extend_from_slice(b"MTrk");
    file.extend_from_slice(&(events.len() as u32).to_be_bytes());
    file.extend_from_slice(events);
}

fn two_track_midi() -> Vec<u8> {
    let mut file = Vec::new();
    file.extend_from_slice(b"MThd");
    file.extend_from_slice(&6_u32.to_be_bytes());
    file.extend_from_slice(&1_u16.to_be_bytes());
    file.extend_from_slice(&2_u16.to_be_bytes());
    file.extend_from_slice(&480_u16.to_be_bytes());

    let track_one = [
        0x00, 0xff, 0x03, 0x05, b'P', b'i', b'a', b'n', b'o', // name
        0x00, 0x90, 60, 100, // C4 on at tick 0
        0x83, 0x60, 0x80, 60, 0, // off at tick 480
        0x00, 0xff, 0x2f, 0x00,
    ];
    let track_two = [
        0x00, 0xff, 0x03, 0x05, b'C', b'h', b'o', b'i', b'r', // name
        0x00, 0x91, 64, 80, // E4 on at tick 0, channel 2
        0x83, 0x60, 0x91, 64, 0, // velocity-zero Note On closes it
        0x00, 0xff, 0x2f, 0x00,
    ];
    push_track(&mut file, &track_one);
    push_track(&mut file, &track_two);
    file
}

#[test]
fn imports_type_one_and_groups_equal_ticks_across_tracks() {
    let bytes = two_track_midi();
    let score = import_midi(&bytes, &ImportOptions::default()).unwrap();
    assert_eq!(score.format, ScoreFormat::Midi);
    assert_eq!(score.parts.len(), 2);
    assert_eq!(score.parts[0].name, "Piano");
    assert_eq!(score.tap_events.len(), 1);
    assert_eq!(
        score.tap_events[0]
            .attacks
            .iter()
            .map(|attack| (attack.midi_pitch, attack.velocity_hint))
            .collect::<Vec<_>>(),
        vec![(60, Some(100)), (64, Some(80))]
    );
    assert_eq!(score.attacks[0].end, Rational::ONE);
    assert_eq!(
        display_musicxml_text(&bytes, &ImportOptions::default()).unwrap(),
        None
    );

    let choir = BTreeSet::from(["midi-track-2".to_owned()]);
    let choir_events = score.tap_events_for_parts(&choir).unwrap();
    assert_eq!(choir_events[0].attacks[0].midi_pitch, 64);
}

#[test]
fn unmatched_note_on_gets_bounded_fallback_duration_and_warning() {
    let mut file = Vec::new();
    file.extend_from_slice(b"MThd");
    file.extend_from_slice(&6_u32.to_be_bytes());
    file.extend_from_slice(&0_u16.to_be_bytes());
    file.extend_from_slice(&1_u16.to_be_bytes());
    file.extend_from_slice(&480_u16.to_be_bytes());
    push_track(&mut file, &[0x00, 0x90, 60, 90, 0x00, 0xff, 0x2f, 0x00]);
    let score = import_midi(&file, &ImportOptions::default()).unwrap();
    assert_eq!(score.attacks[0].end, Rational::new(1, 4).unwrap());
    assert!(score
        .warnings
        .iter()
        .any(|warning| warning.code == WarningCode::MidiNoteWithoutOff));
}

#[test]
fn rejects_sequential_type_two_files() {
    let mut file = Vec::new();
    file.extend_from_slice(b"MThd");
    file.extend_from_slice(&6_u32.to_be_bytes());
    file.extend_from_slice(&2_u16.to_be_bytes());
    file.extend_from_slice(&1_u16.to_be_bytes());
    file.extend_from_slice(&480_u16.to_be_bytes());
    push_track(&mut file, &[0x00, 0xff, 0x2f, 0x00]);
    assert!(matches!(
        import_midi(&file, &ImportOptions::default()),
        Err(ImportError::UnsupportedMidiFormat)
    ));
}
