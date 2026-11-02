//! Validação dos invariantes do modelo (ver `docs/arquitetura.md` §3.8).

use std::collections::HashSet;

use crate::fraction::Fraction;
use crate::id::{ElementId, NoteId};
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
    /// Soma das durações de uma voz excede a duração do compasso.
    VoiceExceedsMeasure { measure: u32, voice_id: u8 },
    /// `tie_to` aponta para uma nota inexistente no Score.
    DanglingTie { measure: u32, from: NoteId, to: NoteId },
    /// Membro de tuplet referencia um `ElementId` inexistente no compasso.
    TupletMemberMissing { measure: u32, member: ElementId },
    /// Nenhum compasso tem fórmula de compasso e nenhuma foi herdada.
    MissingInitialTimeSignature,
}

impl Score {
    /// Valida os invariantes do modelo. `Ok(())` se tudo estiver consistente.
    pub fn validate(&self) -> Result<(), ValidationError> {
        // Coleta todos os NoteId do score para checar ties.
        let mut all_notes: HashSet<u64> = HashSet::new();
        for m in &self.measures {
            for v in &m.voices {
                for cr in &v.elements {
                    for n in &cr.notes {
                        all_notes.insert(n.id.value());
                    }
                }
            }
        }

        // Precisa de uma fórmula de compasso inicial (explícita no 1º compasso que a define).
        let mut current_ts: Option<Fraction> = None;

        for m in &self.measures {
            if let Some(ts) = m.time_signature {
                current_ts = Some(ts.measure_duration());
            }
            let measure_dur = match current_ts {
                Some(d) => d,
                None => return Err(ValidationError::MissingInitialTimeSignature),
            };

            validate_measure(m, measure_dur, &all_notes)?;
        }

        Ok(())
    }
}

fn validate_measure(
    m: &Measure,
    measure_dur: Fraction,
    all_notes: &HashSet<u64>,
) -> Result<(), ValidationError> {
    // IDs de elementos existentes neste compasso (para checar membros de tuplets).
    let mut element_ids: HashSet<u64> = HashSet::new();
    for v in &m.voices {
        for cr in &v.elements {
            element_ids.insert(cr.id.value());
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

        // Elementos monotônicos e não sobrepostos; soma não excede o compasso.
        let mut cursor = Fraction::ZERO;
        for cr in &v.elements {
            if cr.start < cursor {
                return Err(ValidationError::OverlappingElements {
                    measure: m.index,
                    voice_id: v.voice_id,
                    at: cr.id,
                });
            }
            // Valida pitch de cada nota.
            for n in &cr.notes {
                if !n.pitch.is_valid() {
                    return Err(ValidationError::InvalidPitch {
                        measure: m.index,
                        note: n.id,
                    });
                }
                if let Some(target) = n.tie_to {
                    if !all_notes.contains(&target.value()) {
                        return Err(ValidationError::DanglingTie {
                            measure: m.index,
                            from: n.id,
                            to: target,
                        });
                    }
                }
            }
            // Nota: a quiáltera altera a duração efetiva; aqui usamos a duração nominal
            // (refinamento previsto quando o cálculo de tuplet entrar no engine).
            cursor = cr.start + cr.duration.as_fraction();
            if cursor > measure_dur {
                return Err(ValidationError::VoiceExceedsMeasure {
                    measure: m.index,
                    voice_id: v.voice_id,
                });
            }
        }
    }

    // Membros de tuplets devem existir no compasso.
    for t in &m.tuplets {
        for member in &t.members {
            if !element_ids.contains(&member.value()) {
                return Err(ValidationError::TupletMemberMissing {
                    measure: m.index,
                    member: *member,
                });
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{ElementId, NoteId};
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
        Score {
            title: "t".into(),
            tempo_bpm: 120,
            staff_type: StaffType::GrandStaff,
            measures: vec![Measure {
                index: 0,
                time_signature: Some(TimeSignature { numerator: 4, denominator: 4 }),
                key_signature: Some(0),
                chord_symbols: vec![],
                voices,
                tuplets: vec![],
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
            Err(ValidationError::VoiceExceedsMeasure { measure: 0, voice_id: 0 })
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
            Err(ValidationError::DanglingTie { measure: 0, from: NoteId(10), to: NoteId(999) })
        );
    }

    #[test]
    fn detecta_voice_id_duplicado() {
        let v1 = Voice { voice_id: 0, elements: vec![] };
        let v2 = Voice { voice_id: 0, elements: vec![] };
        assert_eq!(
            score_44(vec![v1, v2]).validate(),
            Err(ValidationError::DuplicateVoiceId { measure: 0, voice_id: 0 })
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
            Err(ValidationError::OverlappingElements { measure: 0, voice_id: 0, at: ElementId(2) })
        );
    }
}
