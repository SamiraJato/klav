//! Exemplo: monta um compasso 4/4 de lead sheet (uma voz + cifras), valida e serializa em JSON.
//!
//! Rodar com:  cargo run -p klav-core --example lead_sheet

use klav_core::{
    Accidental, ChordOrRest, ChordSymbol, Clef, Duration, DurationBase, ElementId, Fraction,
    Measure, Note, NoteId, Pitch, Score, StaffType, TimeSignature, Voice,
};

fn quarter(id: u64, start: Fraction, note_id: u64, step: char, octave: i8) -> ChordOrRest {
    ChordOrRest {
        id: ElementId(id),
        start,
        duration: Duration::new(DurationBase::Quarter, 0),
        is_rest: false,
        notes: vec![Note {
            id: NoteId(note_id),
            pitch: Pitch::new(step, octave, None),
            velocity: 80,
            tie_to: None,
        }],
        tuplet: None,
    }
}

fn main() {
    let q = Fraction::new(1, 4);

    // Melodia: C4 D4 E4 F4, com cifras C e G7.
    let voice = Voice {
        voice_id: 0,
        elements: vec![
            quarter(1, Fraction::ZERO, 10, 'C', 4),
            quarter(2, q, 11, 'D', 4),
            quarter(3, q + q, 12, 'E', 4),
            quarter(4, q + q + q, 13, 'F', 4),
        ],
    };

    let measure = Measure {
        index: 0,
        time_signature: Some(TimeSignature { numerator: 4, denominator: 4 }),
        key_signature: Some(0),
        chord_symbols: vec![
            ChordSymbol { start: Fraction::ZERO, text: "C".into() },
            ChordSymbol { start: q + q, text: "G7".into() },
        ],
        voices: vec![voice],
        tuplets: vec![],
    };

    let score = Score {
        title: "Exemplo Lead Sheet".into(),
        tempo_bpm: 120,
        staff_type: StaffType::Single(Clef::Treble),
        measures: vec![measure],
    };

    // Valida os invariantes do modelo.
    match score.validate() {
        Ok(()) => println!("Partitura válida ✅"),
        Err(e) => {
            eprintln!("Partitura inválida: {e:?}");
            std::process::exit(1);
        }
    }

    // Demonstra o pitch derivado.
    let do_sharp = Pitch::new('C', 4, Some(Accidental::Sharp));
    println!("Dó#4 -> MIDI {:?}", do_sharp.midi_number());

    // Serializa em JSON (formato nativo .klav é JSON via serde).
    let json = serde_json::to_string_pretty(&score).expect("serialização JSON");
    println!("\nJSON do Score:\n{json}");

    // Round-trip: desserializa de volta e confirma igualdade.
    let back: Score = serde_json::from_str(&json).expect("desserialização JSON");
    assert_eq!(back, score);
    println!("\nRound-trip JSON OK ✅");
}
