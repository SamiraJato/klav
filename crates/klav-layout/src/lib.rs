//! # klav-layout
//!
//! Motor de layout do klav — converte um [`Score`] em **geometria agnóstica de backend**:
//! coordenadas em *staff-spaces* (sp) e glifos [`Glyph`] (SMuFL). Nada aqui sabe o que é
//! SVG, Canvas ou pixels; o backend de render (no `klav-app`) consome esta cena e desenha.
//!
//! Este é o primeiro corte. **Inclui:** posição vertical por pitch/clave, glifos SMuFL e
//! espaçamento horizontal proporcional à duração (uma pauta simples, em linha contínua).
//! **Ainda não:** hastes/bandeirolas/barras, linhas suplementares desenhadas, acidentes
//! posicionados, quebra em sistemas/páginas e Grand Staff (usa a pauta de cima por ora).

pub mod glyph;
pub mod vertical;

pub use glyph::Glyph;

use klav_core::{Clef, Score, StaffType};

/// Alias semântico: medidas em *staff-spaces*.
pub type Sp = f64;

/// Margem à esquerda antes da clave (sp).
pub const MARGIN: Sp = 2.0;
/// Largura reservada para a clave (sp).
pub const CLEF_WIDTH: Sp = 4.0;
/// Largura horizontal por semibreve de tempo musical (sp) — base do espaçamento proporcional.
pub const SPACE_PER_WHOLE: Sp = 24.0;
/// Número de linhas da pauta.
pub const STAFF_LINE_COUNT: u32 = 5;

/// Ponto no sistema de coordenadas do layout (sp), Y crescendo para baixo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: Sp,
    pub y: Sp,
}

/// Um glifo já posicionado na cena.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedGlyph {
    pub pos: Point,
    pub glyph: Glyph,
    /// `NoteId` de origem (para hit-testing/edição), quando o glifo é uma cabeça de nota.
    pub note_id: Option<u64>,
}

/// Geometria de uma pauta já diagramada.
#[derive(Debug, Clone, PartialEq)]
pub struct StaffLayout {
    pub clef: Clef,
    /// Y da linha superior (sempre 0.0 neste corte).
    pub y_top: Sp,
    /// Número de linhas (5).
    pub line_count: u32,
    /// Largura total ocupada (sp).
    pub width: Sp,
    pub glyphs: Vec<PlacedGlyph>,
}

/// Y de uma linha da pauta (0 = superior .. 4 = inferior), em sp.
pub fn staff_line_y(line_from_top: u32) -> Sp {
    line_from_top as Sp
}

/// X de um instante musical absoluto (em semibreves) na linha contínua.
fn x_at(abs_whole: f64) -> Sp {
    MARGIN + CLEF_WIDTH + SPACE_PER_WHOLE * abs_whole
}

/// Diagrama uma pauta simples a partir do `Score` (linha contínua).
///
/// Para `StaffType::GrandStaff` usa, por enquanto, só a pauta de cima (clave de sol).
pub fn layout_single_staff(score: &Score) -> StaffLayout {
    let clef = match score.staff_type {
        StaffType::Single(c) => c,
        StaffType::GrandStaff => Clef::Treble, // TODO: diagramar as duas pautas
    };

    let mut glyphs: Vec<PlacedGlyph> = Vec::new();

    // Clave no início. Ancoragem aproximada: sol curva na linha de G4 (y=3), fá nos pontos de F3 (y=1).
    let clef_glyph = match clef {
        Clef::Treble => Glyph::GClef,
        Clef::Bass => Glyph::FClef,
    };
    let clef_y = match clef {
        Clef::Treble => 3.0,
        Clef::Bass => 1.0,
    };
    glyphs.push(PlacedGlyph {
        pos: Point { x: MARGIN, y: clef_y },
        glyph: clef_glyph,
        note_id: None,
    });

    // Percorre os compassos acumulando o início absoluto (em semibreves).
    let mut measure_start = 0.0_f64;
    let mut cur_measure_dur = 1.0_f64; // 4/4 por padrão

    for m in &score.measures {
        if let Some(ts) = m.time_signature {
            cur_measure_dur = ts.measure_duration().to_f64();
        }

        for v in &m.voices {
            for cr in &v.elements {
                let abs = measure_start + cr.start.to_f64();
                let x = x_at(abs);

                if cr.is_rest {
                    glyphs.push(PlacedGlyph {
                        pos: Point { x, y: 2.0 }, // pausa na linha central
                        glyph: Glyph::rest_for(cr.duration.base),
                        note_id: None,
                    });
                    continue;
                }

                for n in &cr.notes {
                    if let Some(y) = vertical::note_y(&n.pitch, clef) {
                        glyphs.push(PlacedGlyph {
                            pos: Point { x, y },
                            glyph: Glyph::notehead_for(cr.duration.base),
                            note_id: Some(n.id.value()),
                        });
                    }
                    // pitch inválido: sem glifo (não há como posicionar)
                }
            }
        }

        measure_start += cur_measure_dur;
    }

    StaffLayout {
        clef,
        y_top: 0.0,
        line_count: STAFF_LINE_COUNT,
        width: x_at(measure_start) + MARGIN,
        glyphs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use klav_core::{
        ChordOrRest, Duration, DurationBase, ElementId, Fraction, Measure, Note, NoteId, Pitch,
        Score, StaffType, TimeSignature, Voice,
    };

    fn note(id: u64, step: char, octave: i8) -> Note {
        Note { id: NoteId(id), pitch: Pitch::new(step, octave, None), velocity: 80, tie_to: None }
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

    fn score(staff: StaffType, voices: Vec<Voice>) -> Score {
        Score {
            title: "t".into(),
            tempo_bpm: 120,
            staff_type: staff,
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

    fn noteheads(l: &StaffLayout) -> Vec<&PlacedGlyph> {
        l.glyphs.iter().filter(|g| g.note_id.is_some()).collect()
    }

    #[test]
    fn coloca_clave_e_uma_cabeca() {
        let v = Voice { voice_id: 0, elements: vec![elem(1, Fraction::ZERO, DurationBase::Quarter, vec![note(10, 'C', 4)])] };
        let l = layout_single_staff(&score(StaffType::Single(Clef::Treble), vec![v]));
        assert_eq!(l.clef, Clef::Treble);
        // primeiro glifo é a clave
        assert_eq!(l.glyphs[0].glyph, Glyph::GClef);
        // uma cabeça de nota, C4 em y=5.0 (1ª suplementar abaixo), x = MARGIN+CLEF_WIDTH
        let heads = noteheads(&l);
        assert_eq!(heads.len(), 1);
        assert_eq!(heads[0].glyph, Glyph::NoteheadBlack);
        assert_eq!(heads[0].pos.y, 5.0);
        assert_eq!(heads[0].pos.x, MARGIN + CLEF_WIDTH);
        assert_eq!(heads[0].note_id, Some(10));
    }

    #[test]
    fn espacamento_horizontal_proporcional_ao_inicio() {
        let q = Fraction::new(1, 4);
        let v = Voice {
            voice_id: 0,
            elements: vec![
                elem(1, Fraction::ZERO, DurationBase::Quarter, vec![note(10, 'E', 4)]),
                elem(2, q, DurationBase::Quarter, vec![note(11, 'G', 4)]),
            ],
        };
        let l = layout_single_staff(&score(StaffType::Single(Clef::Treble), vec![v]));
        let heads = noteheads(&l);
        assert_eq!(heads[0].pos.x, MARGIN + CLEF_WIDTH); // start 0
        assert_eq!(heads[1].pos.x, MARGIN + CLEF_WIDTH + SPACE_PER_WHOLE * 0.25); // start 1/4
        assert!(heads[1].pos.x > heads[0].pos.x);
    }

    #[test]
    fn pausa_vira_glifo_de_pausa_na_linha_central() {
        let v = Voice { voice_id: 0, elements: vec![elem(1, Fraction::ZERO, DurationBase::Quarter, vec![])] };
        let l = layout_single_staff(&score(StaffType::Single(Clef::Treble), vec![v]));
        assert!(noteheads(&l).is_empty());
        let rest = l.glyphs.iter().find(|g| g.glyph == Glyph::RestQuarter).unwrap();
        assert_eq!(rest.pos.y, 2.0);
    }

    #[test]
    fn grand_staff_usa_pauta_de_cima_por_enquanto() {
        let v = Voice { voice_id: 0, elements: vec![elem(1, Fraction::ZERO, DurationBase::Whole, vec![note(10, 'G', 4)])] };
        let l = layout_single_staff(&score(StaffType::GrandStaff, vec![v]));
        assert_eq!(l.clef, Clef::Treble);
        assert_eq!(l.glyphs[0].glyph, Glyph::GClef);
    }

    #[test]
    fn largura_cresce_com_o_conteudo() {
        let vazio = score(StaffType::Single(Clef::Treble), vec![Voice { voice_id: 0, elements: vec![] }]);
        let l = layout_single_staff(&vazio);
        // mesmo vazio, ocupa pelo menos o compasso 4/4 (1 semibreve de largura) + margens/clave
        assert!(l.width > MARGIN + CLEF_WIDTH);
    }
}
