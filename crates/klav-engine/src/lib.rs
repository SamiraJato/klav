//! # klav-engine
//!
//! Motor de playback do klav — parte pura e determinística: converte um [`Score`] numa
//! sequência de **eventos MIDI temporizados** (`NoteOn`/`NoteOff`).
//!
//! O tempo musical é mantido em [`Fraction`] (unidade = semibreve), exato e sem float.
//! A conversão para segundos é feita à parte, a partir do `tempo_bpm`.
//!
//! Tratamentos aplicados:
//! - **Ties:** uma cadeia de notas ligadas (`tie_to`) vira um único par NoteOn/NoteOff
//!   sustentado; as notas-alvo da ligadura não disparam evento próprio.
//! - **Tuplets:** a duração soante é escalada por `normal/actual` da quiáltera.
//! - **Pausas:** não geram eventos.
//!
//! O relógio assíncrono e o canal de posição para a UI (ver `docs/arquitetura.md` §4)
//! ficam numa camada posterior, específica de plataforma.

use std::collections::{HashMap, HashSet};

use klav_core::{Fraction, Score};

/// Mensagem MIDI mínima necessária para playback de teclas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiMessage {
    NoteOn { key: u8, velocity: u8 },
    NoteOff { key: u8 },
}

impl MidiMessage {
    /// Ordem de desempate quando dois eventos caem no mesmo instante:
    /// NoteOff antes de NoteOn, evitando sobreposição espúria no mesmo tick.
    fn tie_break(&self) -> u8 {
        match self {
            MidiMessage::NoteOff { .. } => 0,
            MidiMessage::NoteOn { .. } => 1,
        }
    }
}

/// Um evento MIDI posicionado no tempo musical absoluto (desde o início da peça).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MidiEvent {
    /// Posição absoluta em frações de semibreve a partir do início do `Score`.
    pub time: Fraction,
    pub message: MidiMessage,
}

impl MidiEvent {
    /// Instante do evento em segundos, dado o andamento em BPM.
    pub fn seconds(&self, tempo_bpm: u16) -> f64 {
        whole_to_seconds(self.time, tempo_bpm)
    }
}

/// Converte um instante em frações de semibreve para segundos.
///
/// BPM = semínimas por minuto; uma semibreve = 4 semínimas, logo
/// `segundos = tempo_whole * 4 * 60 / bpm = tempo_whole * 240 / bpm`.
pub fn whole_to_seconds(time_whole: Fraction, tempo_bpm: u16) -> f64 {
    debug_assert!(tempo_bpm > 0, "tempo_bpm deve ser > 0");
    time_whole.to_f64() * 240.0 / tempo_bpm as f64
}

/// Duração total da peça em frações de semibreve (soma das durações dos compassos).
///
/// Compasso sem fórmula explícita herda a anterior; na ausência de qualquer uma,
/// assume 4/4.
pub fn total_duration_whole(score: &Score) -> Fraction {
    let mut total = Fraction::ZERO;
    let mut cur = Fraction::ONE; // 4/4 por padrão
    for m in &score.measures {
        if let Some(ts) = m.time_signature {
            cur = ts.measure_duration();
        }
        total = total + cur;
    }
    total
}

/// Informação intermediária de uma nota, por `NoteId`.
struct NoteRender {
    start: Fraction,
    duration: Fraction,
    key: u8,
    velocity: u8,
    tie_to: Option<u64>,
}

/// Renderiza o `Score` em eventos MIDI ordenados por tempo.
///
/// Notas com grafia inválida (sem número MIDI em 0..=127) são ignoradas.
pub fn render_events(score: &Score) -> Vec<MidiEvent> {
    let (notes, order) = collect_notes(score);

    // Notas que são alvo de alguma ligadura: continuações, não disparam evento próprio.
    let mut is_tie_target: HashSet<u64> = HashSet::new();
    for nr in notes.values() {
        if let Some(t) = nr.tie_to {
            is_tie_target.insert(t);
        }
    }

    let mut events: Vec<MidiEvent> = Vec::new();
    for id in &order {
        if is_tie_target.contains(id) {
            continue; // consumida por uma cadeia de tie
        }
        let head = &notes[id];

        // Soma a duração ao longo da cadeia de ties.
        let mut total = head.duration;
        let mut cursor = head.tie_to;
        let mut guard = 0usize;
        while let Some(next_id) = cursor {
            match notes.get(&next_id) {
                Some(nr) => {
                    total = total + nr.duration;
                    cursor = nr.tie_to;
                }
                None => break, // tie pendurado: trata como fim da cadeia
            }
            guard += 1;
            if guard > 1_000_000 {
                break; // proteção contra ciclo de ties malformado
            }
        }

        events.push(MidiEvent {
            time: head.start,
            message: MidiMessage::NoteOn {
                key: head.key,
                velocity: head.velocity,
            },
        });
        events.push(MidiEvent {
            time: head.start + total,
            message: MidiMessage::NoteOff { key: head.key },
        });
    }

    events.sort_by(|a, b| {
        a.time
            .cmp(&b.time)
            .then(a.message.tie_break().cmp(&b.message.tie_break()))
    });
    events
}

/// Percorre a partitura e coleta cada nota (por id) com tempo absoluto e duração soante.
fn collect_notes(score: &Score) -> (HashMap<u64, NoteRender>, Vec<u64>) {
    let mut notes: HashMap<u64, NoteRender> = HashMap::new();
    let mut order: Vec<u64> = Vec::new();

    let mut measure_start = Fraction::ZERO;
    let mut cur_measure_dur = Fraction::ONE; // 4/4 por padrão

    for m in &score.measures {
        if let Some(ts) = m.time_signature {
            cur_measure_dur = ts.measure_duration();
        }

        // Índice das quiálteras do compasso por id, para escalar durações.
        let tuplets: HashMap<u64, Fraction> = m
            .tuplets
            .iter()
            .map(|t| {
                let ratio = if t.actual == 0 {
                    Fraction::ONE
                } else {
                    Fraction::new(t.normal as i64, t.actual as i64)
                };
                (t.id.value(), ratio)
            })
            .collect();

        for v in &m.voices {
            for cr in &v.elements {
                if cr.is_rest {
                    continue;
                }
                let abs_start = measure_start + cr.start;
                let mut dur = cr.duration.as_fraction();
                if let Some(tid) = cr.tuplet {
                    if let Some(ratio) = tuplets.get(&tid.value()) {
                        dur = dur * *ratio;
                    }
                }

                for n in &cr.notes {
                    let key = match n.pitch.midi_number() {
                        Some(k) => k,
                        None => continue, // grafia inválida: não dá para tocar
                    };
                    let id = n.id.value();
                    notes.insert(
                        id,
                        NoteRender {
                            start: abs_start,
                            duration: dur,
                            key,
                            velocity: n.velocity,
                            tie_to: n.tie_to.map(|t| t.value()),
                        },
                    );
                    order.push(id);
                }
            }
        }

        measure_start = measure_start + cur_measure_dur;
    }

    (notes, order)
}

#[cfg(test)]
mod tests {
    use super::*;
    use klav_core::{
        ChordOrRest, Clef, Duration, DurationBase, ElementId, Measure, Note, NoteId, Pitch, Score,
        StaffType, TimeSignature, Tuplet, TupletId, Voice,
    };

    fn note(id: u64, step: char, octave: i8, tie_to: Option<u64>) -> Note {
        Note {
            id: NoteId(id),
            pitch: Pitch::new(step, octave, None),
            velocity: 80,
            tie_to: tie_to.map(NoteId),
        }
    }

    fn elem(id: u64, start: Fraction, base: DurationBase, notes: Vec<Note>) -> ChordOrRest {
        ChordOrRest {
            id: ElementId(id),
            start,
            duration: Duration::new(base, 0),
            is_rest: notes.is_empty(),
            notes,
            tuplet: None,
        }
    }

    fn one_measure(voices: Vec<Voice>, tuplets: Vec<Tuplet>) -> Score {
        Score {
            title: "t".into(),
            tempo_bpm: 120,
            staff_type: StaffType::Single(Clef::Treble),
            measures: vec![Measure {
                index: 0,
                time_signature: Some(TimeSignature { numerator: 4, denominator: 4 }),
                key_signature: Some(0),
                chord_symbols: vec![],
                voices,
                tuplets,
            }],
        }
    }

    #[test]
    fn nota_simples_gera_on_e_off() {
        let v = Voice {
            voice_id: 0,
            elements: vec![elem(1, Fraction::ZERO, DurationBase::Quarter, vec![note(10, 'C', 4, None)])],
        };
        let ev = render_events(&one_measure(vec![v], vec![]));
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[0], MidiEvent { time: Fraction::ZERO, message: MidiMessage::NoteOn { key: 60, velocity: 80 } });
        assert_eq!(ev[1], MidiEvent { time: Fraction::new(1, 4), message: MidiMessage::NoteOff { key: 60 } });
    }

    #[test]
    fn pausa_nao_gera_evento() {
        let v = Voice {
            voice_id: 0,
            elements: vec![elem(1, Fraction::ZERO, DurationBase::Quarter, vec![])],
        };
        assert!(render_events(&one_measure(vec![v], vec![])).is_empty());
    }

    #[test]
    fn duas_notas_ligadas_viram_uma_sustentada() {
        // C4 (1/4) ligada à C4 seguinte (1/4) -> um NoteOn em 0, NoteOff em 1/2.
        let q = Fraction::new(1, 4);
        let v = Voice {
            voice_id: 0,
            elements: vec![
                elem(1, Fraction::ZERO, DurationBase::Quarter, vec![note(10, 'C', 4, Some(11))]),
                elem(2, q, DurationBase::Quarter, vec![note(11, 'C', 4, None)]),
            ],
        };
        let ev = render_events(&one_measure(vec![v], vec![]));
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[0].message, MidiMessage::NoteOn { key: 60, velocity: 80 });
        assert_eq!(ev[0].time, Fraction::ZERO);
        assert_eq!(ev[1].message, MidiMessage::NoteOff { key: 60 });
        assert_eq!(ev[1].time, Fraction::new(1, 2));
    }

    #[test]
    fn acorde_gera_eventos_para_cada_nota() {
        let v = Voice {
            voice_id: 0,
            elements: vec![elem(
                1,
                Fraction::ZERO,
                DurationBase::Half,
                vec![note(10, 'C', 4, None), note(11, 'E', 4, None), note(12, 'G', 4, None)],
            )],
        };
        let ev = render_events(&one_measure(vec![v], vec![]));
        assert_eq!(ev.len(), 6); // 3 on + 3 off
        let ons = ev.iter().filter(|e| matches!(e.message, MidiMessage::NoteOn { .. })).count();
        assert_eq!(ons, 3);
    }

    #[test]
    fn tuplet_escala_duracao() {
        // Tercina de colcheias: cada colcheia (1/8) * (2/3) = 1/12.
        let mut e = elem(1, Fraction::ZERO, DurationBase::Eighth, vec![note(10, 'C', 4, None)]);
        e.tuplet = Some(TupletId(100));
        let v = Voice { voice_id: 0, elements: vec![e] };
        let tuplet = Tuplet {
            id: TupletId(100),
            actual: 3,
            normal: 2,
            base: DurationBase::Eighth,
            members: vec![ElementId(1)],
        };
        let ev = render_events(&one_measure(vec![v], vec![tuplet]));
        assert_eq!(ev[1].message, MidiMessage::NoteOff { key: 60 });
        assert_eq!(ev[1].time, Fraction::new(1, 12));
    }

    #[test]
    fn notas_em_compassos_diferentes_tem_tempo_absoluto() {
        let v0 = Voice { voice_id: 0, elements: vec![elem(1, Fraction::ZERO, DurationBase::Whole, vec![note(10, 'C', 4, None)])] };
        let v1 = Voice { voice_id: 0, elements: vec![elem(2, Fraction::ZERO, DurationBase::Whole, vec![note(20, 'D', 4, None)])] };
        let score = Score {
            title: "t".into(),
            tempo_bpm: 120,
            staff_type: StaffType::Single(Clef::Treble),
            measures: vec![
                Measure { index: 0, time_signature: Some(TimeSignature { numerator: 4, denominator: 4 }), key_signature: Some(0), chord_symbols: vec![], voices: vec![v0], tuplets: vec![] },
                Measure { index: 1, time_signature: None, key_signature: None, chord_symbols: vec![], voices: vec![v1], tuplets: vec![] },
            ],
        };
        let ev = render_events(&score);
        // 2ª nota (compasso 2) começa em 1 semibreve (fim do compasso 4/4).
        let on_d = ev.iter().find(|e| e.message == MidiMessage::NoteOn { key: 62, velocity: 80 }).unwrap();
        assert_eq!(on_d.time, Fraction::ONE);
    }

    #[test]
    fn conversao_para_segundos() {
        // 120 BPM: semínima = 0.5s; semibreve = 2s.
        assert!((whole_to_seconds(Fraction::new(1, 4), 120) - 0.5).abs() < 1e-9);
        assert!((whole_to_seconds(Fraction::ONE, 120) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn duracao_total_soma_compassos() {
        let v = Voice { voice_id: 0, elements: vec![] };
        let score = Score {
            title: "t".into(),
            tempo_bpm: 120,
            staff_type: StaffType::Single(Clef::Treble),
            measures: vec![
                Measure { index: 0, time_signature: Some(TimeSignature { numerator: 4, denominator: 4 }), key_signature: Some(0), chord_symbols: vec![], voices: vec![v.clone()], tuplets: vec![] },
                Measure { index: 1, time_signature: Some(TimeSignature { numerator: 3, denominator: 4 }), key_signature: None, chord_symbols: vec![], voices: vec![v], tuplets: vec![] },
            ],
        };
        // 4/4 + 3/4 = 1 + 3/4 = 7/4
        assert_eq!(total_duration_whole(&score), Fraction::new(7, 4));
    }
}
