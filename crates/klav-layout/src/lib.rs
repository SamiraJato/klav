//! # klav-layout
//!
//! Motor de layout do klav — converte um [`Score`] em **geometria agnóstica de backend**:
//! coordenadas em *staff-spaces* (sp) e glifos [`Glyph`] (SMuFL). Nada aqui sabe o que é
//! SVG, Canvas ou pixels; o backend de render (no `klav-app`) consome esta cena e desenha.
//!
//! Este corte inclui: posição vertical por pitch/clave, glifos SMuFL, espaçamento horizontal
//! proporcional à duração, **barras de compasso** e **quebra em sistemas** (linhas) quando a
//! largura passa de [`MAX_SYSTEM_WIDTH`] — a pauta não cresce infinitamente para a direita.
//! **Ainda não:** hastes/bandeirolas, linhas suplementares desenhadas, acidentes posicionados,
//! quebra em páginas e Grand Staff (usa a pauta de cima por ora).

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
/// Espaço vertical (sp) entre a última linha de um sistema e a primeira do seguinte.
pub const SYSTEM_GAP: Sp = 8.0;
/// Largura máxima (sp) de um sistema antes de quebrar para a próxima linha.
/// A ~10px/sp isto equivale a ~680px, cabendo na área de partitura da janela padrão;
/// acima disso os compassos vão para o sistema de baixo em vez de crescer para a direita.
pub const MAX_SYSTEM_WIDTH: Sp = 68.0;

/// Altura total de um sistema (pauta + espaço até o próximo), em sp.
fn system_height() -> Sp {
    (STAFF_LINE_COUNT as Sp - 1.0) + SYSTEM_GAP
}

/// X onde começa o conteúdo musical (após margem + clave).
fn content_start() -> Sp {
    MARGIN + CLEF_WIDTH
}

/// Ponto no sistema de coordenadas do layout (sp), Y crescendo para baixo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: Sp,
    pub y: Sp,
}

/// Um glifo já posicionado na cena (Y já inclui o deslocamento do sistema).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedGlyph {
    pub pos: Point,
    pub glyph: Glyph,
    /// `NoteId` de origem (para hit-testing/edição), quando o glifo é uma cabeça de nota.
    pub note_id: Option<u64>,
}

/// Barra de compasso vertical: vai de `y_top` até `y_top + (STAFF_LINE_COUNT-1)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Barline {
    pub x: Sp,
    pub y_top: Sp,
}

/// Uma linha de pauta (sistema): uma fileira horizontal de compassos.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct System {
    /// Y da linha superior desta pauta (sp).
    pub y_top: Sp,
    /// X do extremo direito ocupado (sp) — onde termina a última barra.
    pub width: Sp,
    /// Instante absoluto (semibreves) no início do sistema.
    pub start_whole: f64,
    /// Instante absoluto (semibreves) no fim do sistema.
    pub end_whole: f64,
}

/// Geometria de uma pauta já diagramada, possivelmente em vários sistemas (linhas).
#[derive(Debug, Clone, PartialEq)]
pub struct StaffLayout {
    pub clef: Clef,
    /// Número de linhas por pauta (5).
    pub line_count: u32,
    /// Maior largura entre os sistemas (sp) — para dimensionar a cena.
    pub width: Sp,
    /// Altura total ocupada por todos os sistemas (sp).
    pub height: Sp,
    pub systems: Vec<System>,
    pub glyphs: Vec<PlacedGlyph>,
    pub barlines: Vec<Barline>,
}

impl StaffLayout {
    /// Posição `(x, y_top)` do cursor para um instante absoluto (semibreves), localizando
    /// o sistema que o contém. `None` só se não houver sistema algum.
    pub fn cursor_at(&self, abs_whole: f64) -> Option<(Sp, Sp)> {
        let last = self.systems.len().saturating_sub(1);
        for (i, s) in self.systems.iter().enumerate() {
            let within = abs_whole >= s.start_whole
                && (abs_whole < s.end_whole || (i == last && abs_whole <= s.end_whole));
            if within {
                let x = content_start() + (abs_whole - s.start_whole) * SPACE_PER_WHOLE;
                return Some((x, s.y_top));
            }
        }
        // Fora do alcance: ancora no fim do último sistema.
        self.systems.last().map(|s| {
            (
                content_start() + (s.end_whole - s.start_whole) * SPACE_PER_WHOLE,
                s.y_top,
            )
        })
    }
}

/// Y de uma linha da pauta (0 = superior .. 4 = inferior), em sp.
pub fn staff_line_y(line_from_top: u32) -> Sp {
    line_from_top as Sp
}

fn push_clef(glyphs: &mut Vec<PlacedGlyph>, clef: Clef, y_top: Sp) {
    let (glyph, y) = match clef {
        Clef::Treble => (Glyph::GClef, 3.0),
        Clef::Bass => (Glyph::FClef, 1.0),
    };
    glyphs.push(PlacedGlyph {
        pos: Point {
            x: MARGIN,
            y: y + y_top,
        },
        glyph,
        note_id: None,
    });
}

/// Diagrama uma pauta simples a partir do `Score`, quebrando em sistemas quando um compasso
/// faria o sistema atual passar de `max_width` (sp). Use [`MAX_SYSTEM_WIDTH`] como padrão.
///
/// Para `StaffType::GrandStaff` usa, por enquanto, só a pauta de cima (clave de sol).
pub fn layout_single_staff(score: &Score, max_width: Sp) -> StaffLayout {
    let clef = match score.staff_type {
        StaffType::Single(c) => c,
        StaffType::GrandStaff => Clef::Treble, // TODO: diagramar as duas pautas
    };

    let cs = content_start();
    let mut glyphs: Vec<PlacedGlyph> = Vec::new();
    let mut barlines: Vec<Barline> = Vec::new();
    let mut systems: Vec<System> = Vec::new();

    let mut sys_index: usize = 0;
    let mut sys_local = 0.0_f64; // whole acumulado no sistema atual
    let mut sys_start_abs = 0.0_f64; // abs whole no início do sistema atual
    let mut abs = 0.0_f64; // abs whole no início do compasso atual
    let mut cur_dur = 1.0_f64; // 4/4 por padrão

    let y_top = |i: usize| i as Sp * system_height();

    push_clef(&mut glyphs, clef, y_top(0));

    for m in &score.measures {
        if let Some(ts) = m.time_signature {
            cur_dur = ts.measure_duration().to_f64();
        }

        // Quebra de sistema: se já há conteúdo e somar este compasso passa da largura máxima.
        if sys_local > 0.0 && cs + (sys_local + cur_dur) * SPACE_PER_WHOLE > max_width {
            systems.push(System {
                y_top: y_top(sys_index),
                width: cs + sys_local * SPACE_PER_WHOLE,
                start_whole: sys_start_abs,
                end_whole: abs,
            });
            sys_index += 1;
            sys_local = 0.0;
            sys_start_abs = abs;
            push_clef(&mut glyphs, clef, y_top(sys_index));
        }

        for v in &m.voices {
            for cr in &v.elements {
                let x = cs + (sys_local + cr.start.to_f64()) * SPACE_PER_WHOLE;

                if cr.is_rest {
                    glyphs.push(PlacedGlyph {
                        pos: Point {
                            x,
                            y: 2.0 + y_top(sys_index),
                        },
                        glyph: Glyph::rest_for(cr.duration.base),
                        note_id: None,
                    });
                    continue;
                }

                for n in &cr.notes {
                    if let Some(y) = vertical::note_y(&n.pitch, clef) {
                        glyphs.push(PlacedGlyph {
                            pos: Point {
                                x,
                                y: y + y_top(sys_index),
                            },
                            glyph: Glyph::notehead_for(cr.duration.base),
                            note_id: Some(n.id.value()),
                        });
                    }
                    // pitch inválido: sem glifo (não há como posicionar)
                }
            }
        }

        sys_local += cur_dur;
        abs += cur_dur;

        // Barra ao fim do compasso.
        barlines.push(Barline {
            x: cs + sys_local * SPACE_PER_WHOLE,
            y_top: y_top(sys_index),
        });
    }

    // Fecha o último sistema (ou um sistema vazio, se o score não tem compassos).
    let last_width = if score.measures.is_empty() {
        cs + SPACE_PER_WHOLE
    } else {
        cs + sys_local * SPACE_PER_WHOLE
    };
    systems.push(System {
        y_top: y_top(sys_index),
        width: last_width,
        start_whole: sys_start_abs,
        end_whole: abs,
    });

    let width = systems.iter().map(|s| s.width).fold(0.0_f64, f64::max);
    let height = y_top(sys_index) + (STAFF_LINE_COUNT as Sp - 1.0) + 2.0;

    StaffLayout {
        clef,
        line_count: STAFF_LINE_COUNT,
        width,
        height,
        systems,
        glyphs,
        barlines,
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
        Note {
            id: NoteId(id),
            pitch: Pitch::new(step, octave, None),
            velocity: 80,
            tie_to: None,
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

    /// Compasso 4/4 com uma semibreve (preenche o compasso), id configurável.
    fn whole_measure(index: u32, ts: Option<TimeSignature>, id: u64) -> Measure {
        Measure {
            index,
            time_signature: ts,
            key_signature: Some(0),
            chord_symbols: vec![],
            voices: vec![Voice {
                voice_id: 0,
                elements: vec![elem(
                    id,
                    Fraction::ZERO,
                    DurationBase::Whole,
                    vec![note(id, 'G', 4)],
                )],
            }],
            tuplets: vec![],
        }
    }

    fn score_measures(measures: Vec<Measure>) -> Score {
        Score {
            title: "t".into(),
            tempo_bpm: 120,
            staff_type: StaffType::Single(Clef::Treble),
            measures,
        }
    }

    fn score_1m(staff: StaffType, voices: Vec<Voice>) -> Score {
        Score {
            title: "t".into(),
            tempo_bpm: 120,
            staff_type: staff,
            measures: vec![Measure {
                index: 0,
                time_signature: Some(TimeSignature {
                    numerator: 4,
                    denominator: 4,
                }),
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
        let v = Voice {
            voice_id: 0,
            elements: vec![elem(
                1,
                Fraction::ZERO,
                DurationBase::Quarter,
                vec![note(10, 'C', 4)],
            )],
        };
        let l = layout_single_staff(
            &score_1m(StaffType::Single(Clef::Treble), vec![v]),
            MAX_SYSTEM_WIDTH,
        );
        assert_eq!(l.clef, Clef::Treble);
        assert_eq!(l.glyphs[0].glyph, Glyph::GClef);
        let heads = noteheads(&l);
        assert_eq!(heads.len(), 1);
        assert_eq!(heads[0].glyph, Glyph::NoteheadBlack);
        assert_eq!(heads[0].pos.y, 5.0); // C4 na 1ª suplementar abaixo
        assert_eq!(heads[0].pos.x, MARGIN + CLEF_WIDTH);
        assert_eq!(heads[0].note_id, Some(10));
    }

    #[test]
    fn espacamento_horizontal_proporcional_ao_inicio() {
        let q = Fraction::new(1, 4);
        let v = Voice {
            voice_id: 0,
            elements: vec![
                elem(
                    1,
                    Fraction::ZERO,
                    DurationBase::Quarter,
                    vec![note(10, 'E', 4)],
                ),
                elem(2, q, DurationBase::Quarter, vec![note(11, 'G', 4)]),
            ],
        };
        let l = layout_single_staff(
            &score_1m(StaffType::Single(Clef::Treble), vec![v]),
            MAX_SYSTEM_WIDTH,
        );
        let heads = noteheads(&l);
        assert_eq!(heads[0].pos.x, MARGIN + CLEF_WIDTH);
        assert_eq!(heads[1].pos.x, MARGIN + CLEF_WIDTH + SPACE_PER_WHOLE * 0.25);
    }

    #[test]
    fn pausa_vira_glifo_de_pausa_na_linha_central() {
        let v = Voice {
            voice_id: 0,
            elements: vec![elem(1, Fraction::ZERO, DurationBase::Quarter, vec![])],
        };
        let l = layout_single_staff(
            &score_1m(StaffType::Single(Clef::Treble), vec![v]),
            MAX_SYSTEM_WIDTH,
        );
        assert!(noteheads(&l).is_empty());
        let rest = l
            .glyphs
            .iter()
            .find(|g| g.glyph == Glyph::RestQuarter)
            .unwrap();
        assert_eq!(rest.pos.y, 2.0);
    }

    #[test]
    fn grand_staff_usa_pauta_de_cima_por_enquanto() {
        let v = Voice {
            voice_id: 0,
            elements: vec![elem(
                1,
                Fraction::ZERO,
                DurationBase::Whole,
                vec![note(10, 'G', 4)],
            )],
        };
        let l = layout_single_staff(&score_1m(StaffType::GrandStaff, vec![v]), MAX_SYSTEM_WIDTH);
        assert_eq!(l.clef, Clef::Treble);
        assert_eq!(l.glyphs[0].glyph, Glyph::GClef);
    }

    #[test]
    fn largura_cresce_com_o_conteudo() {
        let vazio = score_1m(
            StaffType::Single(Clef::Treble),
            vec![Voice {
                voice_id: 0,
                elements: vec![],
            }],
        );
        let l = layout_single_staff(&vazio, MAX_SYSTEM_WIDTH);
        assert!(l.width > MARGIN + CLEF_WIDTH);
    }

    #[test]
    fn uma_barra_por_compasso() {
        let s = score_measures(vec![
            whole_measure(
                0,
                Some(TimeSignature {
                    numerator: 4,
                    denominator: 4,
                }),
                1,
            ),
            whole_measure(1, None, 2),
        ]);
        let l = layout_single_staff(&s, MAX_SYSTEM_WIDTH);
        assert_eq!(l.barlines.len(), 2);
        // 1ª barra no fim do compasso 0 (1 semibreve), 2ª no fim do 1 (2 semibreves),
        // ambos no mesmo sistema (2*24=48 sp cabe em MAX_SYSTEM_WIDTH).
        assert_eq!(l.barlines[0].x, MARGIN + CLEF_WIDTH + SPACE_PER_WHOLE);
        assert_eq!(l.barlines[1].x, MARGIN + CLEF_WIDTH + 2.0 * SPACE_PER_WHOLE);
        assert_eq!(l.systems.len(), 1);
    }

    #[test]
    fn quebra_em_sistemas_ao_passar_da_largura() {
        // Largura de teste explícita (independe da constante de produção).
        // 6 compassos de semibreve: cada um ocupa 24 sp; conteúdo começa em 6 sp.
        // Limite 80 sp => cabem 3 compassos por sistema (6+72=78 <= 80; +24 passaria).
        const W: Sp = 80.0;
        let measures: Vec<Measure> = (0..6)
            .map(|i| {
                let ts = if i == 0 {
                    Some(TimeSignature {
                        numerator: 4,
                        denominator: 4,
                    })
                } else {
                    None
                };
                whole_measure(i, ts, (i + 1) as u64)
            })
            .collect();
        let l = layout_single_staff(&score_measures(measures), W);
        assert_eq!(l.systems.len(), 2, "esperava 2 sistemas");
        // O 2º sistema fica abaixo do 1º.
        assert!(l.systems[1].y_top > l.systems[0].y_top);
        // Cada cabeça de nota do 2º sistema tem y deslocado para baixo.
        let heads = noteheads(&l);
        assert_eq!(heads.len(), 6);
        // A largura total não excede o limite.
        assert!(l.width <= W);
        // Há uma clave por sistema (2 claves no total).
        let clefs = l.glyphs.iter().filter(|g| g.glyph == Glyph::GClef).count();
        assert_eq!(clefs, 2);
    }

    #[test]
    fn cursor_localiza_o_sistema_certo() {
        const W: Sp = 80.0; // 3 compassos por sistema
        let measures: Vec<Measure> = (0..6)
            .map(|i| {
                let ts = if i == 0 {
                    Some(TimeSignature {
                        numerator: 4,
                        denominator: 4,
                    })
                } else {
                    None
                };
                whole_measure(i, ts, (i + 1) as u64)
            })
            .collect();
        let l = layout_single_staff(&score_measures(measures), W);
        // Instante 0 -> 1º sistema, no início do conteúdo.
        let (x0, y0) = l.cursor_at(0.0).unwrap();
        assert_eq!(y0, l.systems[0].y_top);
        assert_eq!(x0, MARGIN + CLEF_WIDTH);
        // Instante 4 (4º compasso) -> 2º sistema (que começa no compasso 3).
        let (_, y4) = l.cursor_at(4.0).unwrap();
        assert_eq!(y4, l.systems[1].y_top);
    }
}
