//! Validação dos invariantes do modelo (ver `docs/arquitetura.md` §3.8).

use std::collections::{HashMap, HashSet};

use crate::fraction::Fraction;
use crate::id::{ElementId, NoteId, TupletId};
use crate::model::{Measure, Score, VOICES};

/// Erro de validação, com contexto suficiente para apontar a origem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// `voice_id` >= VOICES.
    VoiceIdOutOfRange { measure: u32, voice_id: u8 },
    /// Duas vozes no mesmo compasso com o mesmo `voice_id`.
    DuplicateVoiceId { measure: u32, voice_id: u8 },
    /// Pitch com grafia inválida ou fora de 0..=127.
    InvalidPitch { measure: u32, note: NoteId },
    /// Elementos de uma voz fora de ordem ou sobrepostos no tempo.
    OverlappingElements {
        measure: u32,
        voice_id: u8,
        at: ElementId,
    },
    /// Soma das durações de uma voz (com tuplets aplicados) excede a duração do compasso.
    VoiceExceedsMeasure { measure: u32, voice_id: u8 },
    /// `tie_to` aponta para uma nota inexistente no Score.
    DanglingTie {
        measure: u32,
        from: NoteId,
        to: NoteId,
    },
    /// `tie_to` aponta para uma nota de altura diferente.
    TiePitchMismatch {
        measure: u32,
        from: NoteId,
        to: NoteId,
    },
    /// `tie_to` aponta para uma nota de outra voz.
    TieVoiceMismatch {
        measure: u32,
        from: NoteId,
        to: NoteId,
    },
    /// `tie_to` não aponta para o elemento imediatamente seguinte da mesma voz.
    TieNotImmediate {
        measure: u32,
        from: NoteId,
        to: NoteId,
    },
    /// Membro de tuplet referencia um `ElementId` inexistente no compasso.
    TupletMemberMissing { measure: u32, member: ElementId },
    /// Nenhum compasso tem fórmula de compasso e nenhuma foi herdada.
    MissingInitialTimeSignature,
    /// Fórmula de compasso com numerador ou denominador zero (duração indefinida).
    InvalidTimeSignature {
        measure: u32,
        numerator: u8,
        denominator: u8,
    },
    /// Dois `Note` no documento com o mesmo `NoteId`.
    DuplicateNoteId { note: NoteId },
    /// Dois `ChordOrRest` no documento com o mesmo `ElementId`.
    DuplicateElementId { element: ElementId },
    /// Dois `Tuplet` no mesmo compasso com o mesmo `TupletId`.
    DuplicateTupletId { measure: u32, tuplet: TupletId },
    /// `is_rest == true` mas o elemento tem notas (ou o contrário).
    RestNotesMismatch { measure: u32, element: ElementId },
    /// `start` de `ChordSymbol` fora do intervalo `[0, duração do compasso)`.
    ChordSymbolOutOfMeasure { measure: u32 },
    /// Tuplet com `actual` ou `normal` igual a zero.
    InvalidTuplet { measure: u32, tuplet: TupletId },
    /// Elemento referencia um `TupletId` que não existe no seu compasso.
    ElementTupletUnknown {
        measure: u32,
        element: ElementId,
        tuplet: TupletId,
    },
    /// Associação elemento↔tuplet incoerente (membro não aponta de volta, ou vice-versa).
    TupletMembershipInconsistent { measure: u32, tuplet: TupletId },
}

/// Contexto de cada nota no documento: voz, ordem temporal na voz e altura soante.
struct NoteCtx {
    voice_id: u8,
    /// Posição sequencial do elemento dentro da voz (global, em ordem de tempo).
    seq: u64,
    /// Número MIDI derivado; `None` se a grafia for inválida.
    midi: Option<u8>,
}

impl Score {
    /// Valida os invariantes do modelo. `Ok(())` se tudo estiver consistente.
    pub fn validate(&self) -> Result<(), ValidationError> {
        // --- Passo A: índice global de notas + unicidade de NoteId/ElementId. ---
        let note_ctx = self.build_note_index()?;

        // --- Passo B: fórmula de compasso herdada + checagens por compasso. ---
        let mut current_ts: Option<Fraction> = None;

        for m in &self.measures {
            if let Some(ts) = m.time_signature {
                // Rejeita numerador/denominador zero ANTES de calcular a duração
                // (Fraction::new faria panic com denominador zero).
                if ts.denominator == 0 || ts.numerator == 0 {
                    return Err(ValidationError::InvalidTimeSignature {
                        measure: m.index,
                        numerator: ts.numerator,
                        denominator: ts.denominator,
                    });
                }
                current_ts = Some(ts.measure_duration());
            }
            let measure_dur = match current_ts {
                Some(d) => d,
                None => return Err(ValidationError::MissingInitialTimeSignature),
            };

            validate_measure(m, measure_dur, &note_ctx)?;
        }

        Ok(())
    }

    /// Percorre o documento atribuindo a cada nota sua voz e ordem temporal, e detecta
    /// `NoteId`/`ElementId` duplicados. A ordem sequencial por voz segue a ordem dos
    /// compassos no `Vec` e dos elementos dentro de cada voz.
    fn build_note_index(&self) -> Result<HashMap<u64, NoteCtx>, ValidationError> {
        let mut note_ctx: HashMap<u64, NoteCtx> = HashMap::new();
        let mut seen_elements: HashSet<u64> = HashSet::new();
        // Próximo índice sequencial por voz (acumulado entre compassos).
        let mut next_seq: HashMap<u8, u64> = HashMap::new();

        for m in &self.measures {
            for v in &m.voices {
                let seq_slot = next_seq.entry(v.voice_id).or_insert(0);
                for cr in &v.elements {
                    if !seen_elements.insert(cr.id.value()) {
                        return Err(ValidationError::DuplicateElementId { element: cr.id });
                    }
                    let seq = *seq_slot;
                    for n in &cr.notes {
                        if note_ctx
                            .insert(
                                n.id.value(),
                                NoteCtx {
                                    voice_id: v.voice_id,
                                    seq,
                                    midi: n.pitch.midi_number(),
                                },
                            )
                            .is_some()
                        {
                            return Err(ValidationError::DuplicateNoteId { note: n.id });
                        }
                    }
                    *seq_slot += 1;
                }
            }
        }

        Ok(note_ctx)
    }
}

fn validate_measure(
    m: &Measure,
    measure_dur: Fraction,
    note_ctx: &HashMap<u64, NoteCtx>,
) -> Result<(), ValidationError> {
    // Índice de tuplets do compasso (por id), detectando TupletId duplicado,
    // e razão normal/actual para escalar as durações.
    let mut tuplet_ratio: HashMap<u64, Fraction> = HashMap::new();
    let mut tuplet_members: HashMap<u64, HashSet<u64>> = HashMap::new();
    for t in &m.tuplets {
        if t.actual == 0 || t.normal == 0 {
            return Err(ValidationError::InvalidTuplet {
                measure: m.index,
                tuplet: t.id,
            });
        }
        if tuplet_ratio
            .insert(
                t.id.value(),
                Fraction::new(t.normal as i64, t.actual as i64),
            )
            .is_some()
        {
            return Err(ValidationError::DuplicateTupletId {
                measure: m.index,
                tuplet: t.id,
            });
        }
        tuplet_members.insert(t.id.value(), t.members.iter().map(|e| e.value()).collect());
    }

    // IDs de elementos existentes neste compasso (para checar membros de tuplets)
    // e o tuplet declarado por cada elemento (para checar coerência bidirecional).
    let mut element_ids: HashSet<u64> = HashSet::new();
    let mut element_tuplet: HashMap<u64, u64> = HashMap::new();
    for v in &m.voices {
        for cr in &v.elements {
            element_ids.insert(cr.id.value());
            if let Some(tid) = cr.tuplet {
                element_tuplet.insert(cr.id.value(), tid.value());
            }
        }
    }

    // Símbolos de acorde dentro do compasso.
    for cs in &m.chord_symbols {
        if cs.start < Fraction::ZERO || cs.start >= measure_dur {
            return Err(ValidationError::ChordSymbolOutOfMeasure { measure: m.index });
        }
    }

    let mut seen_voice_ids: HashSet<u8> = HashSet::new();

    for v in &m.voices {
        if v.voice_id >= VOICES {
            return Err(ValidationError::VoiceIdOutOfRange {
                measure: m.index,
                voice_id: v.voice_id,
            });
        }
        if !seen_voice_ids.insert(v.voice_id) {
            return Err(ValidationError::DuplicateVoiceId {
                measure: m.index,
                voice_id: v.voice_id,
            });
        }

        // Elementos monotônicos e não sobrepostos; soma (com tuplet) não excede o compasso.
        let mut cursor = Fraction::ZERO;
        for cr in &v.elements {
            if cr.start < cursor {
                return Err(ValidationError::OverlappingElements {
                    measure: m.index,
                    voice_id: v.voice_id,
                    at: cr.id,
                });
            }

            // Coerência is_rest ↔ notes.
            if cr.is_rest != cr.notes.is_empty() {
                return Err(ValidationError::RestNotesMismatch {
                    measure: m.index,
                    element: cr.id,
                });
            }

            // Pitch e ties de cada nota.
            for n in &cr.notes {
                if !n.pitch.is_valid() {
                    return Err(ValidationError::InvalidPitch {
                        measure: m.index,
                        note: n.id,
                    });
                }
                if let Some(target) = n.tie_to {
                    validate_tie(m.index, n.id, target, note_ctx)?;
                }
            }

            // Duração efetiva, escalada pelo fator do tuplet quando houver.
            let mut dur = cr.duration.as_fraction();
            if let Some(tid) = cr.tuplet {
                match tuplet_ratio.get(&tid.value()) {
                    Some(ratio) => dur = dur * *ratio,
                    None => {
                        return Err(ValidationError::ElementTupletUnknown {
                            measure: m.index,
                            element: cr.id,
                            tuplet: tid,
                        })
                    }
                }
            }
            cursor = cr.start + dur;
            if cursor > measure_dur {
                return Err(ValidationError::VoiceExceedsMeasure {
                    measure: m.index,
                    voice_id: v.voice_id,
                });
            }
        }
    }

    // Membros de tuplets devem existir no compasso e a associação ser coerente:
    // todo membro declarado aponta de volta (element.tuplet == tuplet.id).
    for t in &m.tuplets {
        for member in &t.members {
            if !element_ids.contains(&member.value()) {
                return Err(ValidationError::TupletMemberMissing {
                    measure: m.index,
                    member: *member,
                });
            }
            if element_tuplet.get(&member.value()) != Some(&t.id.value()) {
                return Err(ValidationError::TupletMembershipInconsistent {
                    measure: m.index,
                    tuplet: t.id,
                });
            }
        }
    }
    // E todo elemento que aponta para um tuplet deve constar como membro dele.
    for (elem, tid) in &element_tuplet {
        match tuplet_members.get(tid) {
            Some(members) if members.contains(elem) => {}
            _ => {
                return Err(ValidationError::TupletMembershipInconsistent {
                    measure: m.index,
                    tuplet: TupletId(*tid),
                });
            }
        }
    }

    Ok(())
}

/// Valida uma ligadura de valor conforme §3.8: destino existente, de mesma altura,
/// no futuro imediato da mesma voz.
fn validate_tie(
    measure: u32,
    from: NoteId,
    to: NoteId,
    note_ctx: &HashMap<u64, NoteCtx>,
) -> Result<(), ValidationError> {
    let src = match note_ctx.get(&from.value()) {
        Some(c) => c,
        None => return Ok(()), // a própria nota de origem sempre está no índice; defensivo.
    };
    let dst = match note_ctx.get(&to.value()) {
        Some(c) => c,
        None => return Err(ValidationError::DanglingTie { measure, from, to }),
    };
    if dst.voice_id != src.voice_id {
        return Err(ValidationError::TieVoiceMismatch { measure, from, to });
    }
    if src.midi != dst.midi {
        return Err(ValidationError::TiePitchMismatch { measure, from, to });
    }
    if dst.seq != src.seq + 1 {
        return Err(ValidationError::TieNotImmediate { measure, from, to });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{ElementId, NoteId, TupletId};
    use crate::model::*;
    use crate::pitch::Pitch;

    fn note(id: u64, step: char, octave: i8) -> Note {
        Note {
            id: NoteId(id),
            pitch: Pitch::new(step, octave, None),
            velocity: 80,
            tie_to: None,
        }
    }

    fn quarter(id: u64, start: Fraction, notes: Vec<Note>) -> ChordOrRest {
        ChordOrRest {
            id: ElementId(id),
            start,
            duration: Duration::new(DurationBase::Quarter, 0),
            is_rest: notes.is_empty(),
            notes,
            tuplet: None,
        }
    }

    fn score_44(voices: Vec<Voice>) -> Score {
        score_44_full(voices, vec![], vec![])
    }

    fn score_44_full(
        voices: Vec<Voice>,
        tuplets: Vec<Tuplet>,
        chord_symbols: Vec<ChordSymbol>,
    ) -> Score {
        Score {
            title: "t".into(),
            tempo_bpm: 120,
            staff_type: StaffType::GrandStaff,
            measures: vec![Measure {
                index: 0,
                time_signature: Some(TimeSignature {
                    numerator: 4,
                    denominator: 4,
                }),
                key_signature: Some(0),
                chord_symbols,
                voices,
                tuplets,
            }],
        }
    }

    #[test]
    fn compasso_valido_quatro_seminimas() {
        let q = Fraction::new(1, 4);
        let v = Voice {
            voice_id: 0,
            elements: vec![
                quarter(1, Fraction::ZERO, vec![note(10, 'C', 4)]),
                quarter(2, q, vec![note(11, 'D', 4)]),
                quarter(3, q + q, vec![note(12, 'E', 4)]),
                quarter(4, q + q + q, vec![note(13, 'F', 4)]),
            ],
        };
        assert_eq!(score_44(vec![v]).validate(), Ok(()));
    }

    #[test]
    fn detecta_voz_que_excede_o_compasso() {
        let q = Fraction::new(1, 4);
        let v = Voice {
            voice_id: 0,
            elements: vec![
                quarter(1, Fraction::ZERO, vec![note(10, 'C', 4)]),
                quarter(2, q, vec![note(11, 'D', 4)]),
                quarter(3, q + q, vec![note(12, 'E', 4)]),
                quarter(4, q + q + q, vec![note(13, 'F', 4)]),
                quarter(5, Fraction::ONE, vec![note(14, 'G', 4)]), // 5ª semínima -> estoura 4/4
            ],
        };
        assert_eq!(
            score_44(vec![v]).validate(),
            Err(ValidationError::VoiceExceedsMeasure {
                measure: 0,
                voice_id: 0
            })
        );
    }

    #[test]
    fn detecta_tie_pendurado() {
        let mut n = note(10, 'C', 4);
        n.tie_to = Some(NoteId(999)); // não existe
        let v = Voice {
            voice_id: 0,
            elements: vec![quarter(1, Fraction::ZERO, vec![n])],
        };
        assert_eq!(
            score_44(vec![v]).validate(),
            Err(ValidationError::DanglingTie {
                measure: 0,
                from: NoteId(10),
                to: NoteId(999)
            })
        );
    }

    #[test]
    fn tie_valido_para_proximo_elemento_mesma_altura() {
        let q = Fraction::new(1, 4);
        let mut n0 = note(10, 'C', 4);
        n0.tie_to = Some(NoteId(11));
        let v = Voice {
            voice_id: 0,
            elements: vec![
                quarter(1, Fraction::ZERO, vec![n0]),
                quarter(2, q, vec![note(11, 'C', 4)]),
            ],
        };
        assert_eq!(score_44(vec![v]).validate(), Ok(()));
    }

    #[test]
    fn detecta_tie_para_altura_diferente() {
        let q = Fraction::new(1, 4);
        let mut n0 = note(10, 'C', 4);
        n0.tie_to = Some(NoteId(11));
        let v = Voice {
            voice_id: 0,
            elements: vec![
                quarter(1, Fraction::ZERO, vec![n0]),
                quarter(2, q, vec![note(11, 'D', 4)]), // altura diferente
            ],
        };
        assert_eq!(
            score_44(vec![v]).validate(),
            Err(ValidationError::TiePitchMismatch {
                measure: 0,
                from: NoteId(10),
                to: NoteId(11)
            })
        );
    }

    #[test]
    fn detecta_tie_entre_vozes() {
        let mut n0 = note(10, 'C', 4);
        n0.tie_to = Some(NoteId(20));
        let v0 = Voice {
            voice_id: 0,
            elements: vec![quarter(1, Fraction::ZERO, vec![n0])],
        };
        let v1 = Voice {
            voice_id: 1,
            elements: vec![quarter(2, Fraction::ZERO, vec![note(20, 'C', 4)])],
        };
        assert_eq!(
            score_44(vec![v0, v1]).validate(),
            Err(ValidationError::TieVoiceMismatch {
                measure: 0,
                from: NoteId(10),
                to: NoteId(20)
            })
        );
    }

    #[test]
    fn detecta_tie_nao_imediato() {
        let q = Fraction::new(1, 4);
        let mut n0 = note(10, 'C', 4);
        n0.tie_to = Some(NoteId(12)); // pula o elemento do meio
        let v = Voice {
            voice_id: 0,
            elements: vec![
                quarter(1, Fraction::ZERO, vec![n0]),
                quarter(2, q, vec![note(11, 'C', 4)]),
                quarter(3, q + q, vec![note(12, 'C', 4)]),
            ],
        };
        assert_eq!(
            score_44(vec![v]).validate(),
            Err(ValidationError::TieNotImmediate {
                measure: 0,
                from: NoteId(10),
                to: NoteId(12)
            })
        );
    }

    #[test]
    fn tie_atravessa_compasso() {
        // Última nota do c.0 liga à primeira do c.1 (mesma voz, mesma altura).
        let mut last = note(13, 'F', 4);
        last.tie_to = Some(NoteId(20));
        let q = Fraction::new(1, 4);
        let v0 = Voice {
            voice_id: 0,
            elements: vec![
                quarter(1, Fraction::ZERO, vec![note(10, 'C', 4)]),
                quarter(2, q, vec![note(11, 'D', 4)]),
                quarter(3, q + q, vec![note(12, 'E', 4)]),
                quarter(4, q + q + q, vec![last]),
            ],
        };
        let v1 = Voice {
            voice_id: 0,
            elements: vec![quarter(5, Fraction::ZERO, vec![note(20, 'F', 4)])],
        };
        let score = Score {
            title: "t".into(),
            tempo_bpm: 120,
            staff_type: StaffType::GrandStaff,
            measures: vec![
                Measure {
                    index: 0,
                    time_signature: Some(TimeSignature {
                        numerator: 4,
                        denominator: 4,
                    }),
                    key_signature: Some(0),
                    chord_symbols: vec![],
                    voices: vec![v0],
                    tuplets: vec![],
                },
                Measure {
                    index: 1,
                    time_signature: None,
                    key_signature: None,
                    chord_symbols: vec![],
                    voices: vec![v1],
                    tuplets: vec![],
                },
            ],
        };
        assert_eq!(score.validate(), Ok(()));
    }

    #[test]
    fn detecta_voice_id_duplicado() {
        let v1 = Voice {
            voice_id: 0,
            elements: vec![],
        };
        let v2 = Voice {
            voice_id: 0,
            elements: vec![],
        };
        assert_eq!(
            score_44(vec![v1, v2]).validate(),
            Err(ValidationError::DuplicateVoiceId {
                measure: 0,
                voice_id: 0
            })
        );
    }

    #[test]
    fn detecta_sobreposicao() {
        let v = Voice {
            voice_id: 0,
            elements: vec![
                quarter(1, Fraction::new(1, 2), vec![note(10, 'C', 4)]),
                quarter(2, Fraction::ZERO, vec![note(11, 'D', 4)]), // start volta no tempo
            ],
        };
        assert_eq!(
            score_44(vec![v]).validate(),
            Err(ValidationError::OverlappingElements {
                measure: 0,
                voice_id: 0,
                at: ElementId(2)
            })
        );
    }

    #[test]
    fn detecta_note_id_duplicado() {
        let q = Fraction::new(1, 4);
        let v = Voice {
            voice_id: 0,
            elements: vec![
                quarter(1, Fraction::ZERO, vec![note(10, 'C', 4)]),
                quarter(2, q, vec![note(10, 'D', 4)]), // id 10 repetido
            ],
        };
        assert_eq!(
            score_44(vec![v]).validate(),
            Err(ValidationError::DuplicateNoteId { note: NoteId(10) })
        );
    }

    #[test]
    fn detecta_element_id_duplicado() {
        let q = Fraction::new(1, 4);
        let v = Voice {
            voice_id: 0,
            elements: vec![
                quarter(1, Fraction::ZERO, vec![note(10, 'C', 4)]),
                quarter(1, q, vec![note(11, 'D', 4)]), // ElementId 1 repetido
            ],
        };
        assert_eq!(
            score_44(vec![v]).validate(),
            Err(ValidationError::DuplicateElementId {
                element: ElementId(1)
            })
        );
    }

    #[test]
    fn detecta_is_rest_inconsistente() {
        let mut cr = quarter(1, Fraction::ZERO, vec![note(10, 'C', 4)]);
        cr.is_rest = true; // marca como pausa mas tem notas
        let v = Voice {
            voice_id: 0,
            elements: vec![cr],
        };
        assert_eq!(
            score_44(vec![v]).validate(),
            Err(ValidationError::RestNotesMismatch {
                measure: 0,
                element: ElementId(1)
            })
        );
    }

    #[test]
    fn detecta_chord_symbol_fora_do_compasso() {
        let v = Voice {
            voice_id: 0,
            elements: vec![quarter(1, Fraction::ZERO, vec![note(10, 'C', 4)])],
        };
        let cs = ChordSymbol {
            start: Fraction::ONE,
            text: "C".into(),
        }; // start == dur do compasso
        assert_eq!(
            score_44_full(vec![v], vec![], vec![cs]).validate(),
            Err(ValidationError::ChordSymbolOutOfMeasure { measure: 0 })
        );
    }

    #[test]
    fn tuplet_valido_passa() {
        // Tercina de colcheias (3 no tempo de 2): 3 * (1/8 * 2/3) = 1/4 <= 4/4.
        let eighth = |id: u64, start: Fraction, nid: u64| ChordOrRest {
            id: ElementId(id),
            start,
            duration: Duration::new(DurationBase::Eighth, 0),
            is_rest: false,
            notes: vec![note(nid, 'C', 4)],
            tuplet: Some(TupletId(100)),
        };
        let twelfth = Fraction::new(1, 12);
        let v = Voice {
            voice_id: 0,
            elements: vec![
                eighth(1, Fraction::ZERO, 10),
                eighth(2, twelfth, 11),
                eighth(3, twelfth + twelfth, 12),
            ],
        };
        let t = Tuplet {
            id: TupletId(100),
            actual: 3,
            normal: 2,
            base: DurationBase::Eighth,
            members: vec![ElementId(1), ElementId(2), ElementId(3)],
        };
        assert_eq!(score_44_full(vec![v], vec![t], vec![]).validate(), Ok(()));
    }

    #[test]
    fn detecta_tuplet_invalido() {
        let v = Voice {
            voice_id: 0,
            elements: vec![quarter(1, Fraction::ZERO, vec![note(10, 'C', 4)])],
        };
        let t = Tuplet {
            id: TupletId(100),
            actual: 0,
            normal: 2,
            base: DurationBase::Eighth,
            members: vec![],
        };
        assert_eq!(
            score_44_full(vec![v], vec![t], vec![]).validate(),
            Err(ValidationError::InvalidTuplet {
                measure: 0,
                tuplet: TupletId(100)
            })
        );
    }

    #[test]
    fn detecta_membro_de_tuplet_inexistente() {
        let v = Voice {
            voice_id: 0,
            elements: vec![quarter(1, Fraction::ZERO, vec![note(10, 'C', 4)])],
        };
        let t = Tuplet {
            id: TupletId(100),
            actual: 3,
            normal: 2,
            base: DurationBase::Eighth,
            members: vec![ElementId(999)],
        };
        assert_eq!(
            score_44_full(vec![v], vec![t], vec![]).validate(),
            Err(ValidationError::TupletMemberMissing {
                measure: 0,
                member: ElementId(999)
            })
        );
    }

    #[test]
    fn detecta_elemento_apontando_tuplet_inexistente() {
        let mut cr = quarter(1, Fraction::ZERO, vec![note(10, 'C', 4)]);
        cr.tuplet = Some(TupletId(777)); // nenhum tuplet 777 no compasso
        let v = Voice {
            voice_id: 0,
            elements: vec![cr],
        };
        assert_eq!(
            score_44_full(vec![v], vec![], vec![]).validate(),
            Err(ValidationError::ElementTupletUnknown {
                measure: 0,
                element: ElementId(1),
                tuplet: TupletId(777)
            })
        );
    }

    #[test]
    fn detecta_membership_incoerente() {
        // Tuplet lista um membro que não aponta de volta para ele.
        let v = Voice {
            voice_id: 0,
            elements: vec![quarter(1, Fraction::ZERO, vec![note(10, 'C', 4)])], // tuplet: None
        };
        let t = Tuplet {
            id: TupletId(100),
            actual: 3,
            normal: 2,
            base: DurationBase::Eighth,
            members: vec![ElementId(1)],
        };
        assert_eq!(
            score_44_full(vec![v], vec![t], vec![]).validate(),
            Err(ValidationError::TupletMembershipInconsistent {
                measure: 0,
                tuplet: TupletId(100)
            })
        );
    }

    #[test]
    fn detecta_time_signature_invalida() {
        let v = Voice {
            voice_id: 0,
            elements: vec![],
        };
        let score = Score {
            title: "t".into(),
            tempo_bpm: 120,
            staff_type: StaffType::GrandStaff,
            measures: vec![Measure {
                index: 0,
                time_signature: Some(TimeSignature {
                    numerator: 4,
                    denominator: 0,
                }),
                key_signature: Some(0),
                chord_symbols: vec![],
                voices: vec![v],
                tuplets: vec![],
            }],
        };
        assert_eq!(
            score.validate(),
            Err(ValidationError::InvalidTimeSignature {
                measure: 0,
                numerator: 4,
                denominator: 0
            })
        );
    }
}
