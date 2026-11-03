//! # klav-layout
//!
//! Motor de layout do klav — converte um [`Score`] em **geometria agnóstica de backend**:
//! coordenadas em *staff-spaces* (sp) e glifos [`Glyph`] (SMuFL). Nada aqui sabe o que é
//! SVG, Canvas ou pixels; o backend de render (no `klav-app`) consome esta cena e desenha.
//!
//! Este corte inclui: posição vertical por pitch/clave, glifos SMuFL, espaçamento horizontal
//! proporcional à duração, **hastes e bandeirolas** (que distinguem a duração), **barras de
//! compasso**, **quebra em sistemas** (linhas) quando a largura passa de [`MAX_SYSTEM_WIDTH`], e
//! um **respiro** ([`MEASURE_PAD`]) nas duas pontas de cada compasso.
//! **Ainda não:** vigas (beams) ligando colcheias, acidentes posicionados, quebra em páginas
//! e Grand Staff (usa a pauta de cima por ora).

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
/// Respiro (sp) em cada ponta do compasso: o conteúdo começa `MEASURE_PAD` depois da barra
/// esquerda e termina `MEASURE_PAD` antes da direita, centralizando-o no compasso.
pub const MEASURE_PAD: Sp = 2.5;
/// Número de linhas da pauta.
pub const STAFF_LINE_COUNT: u32 = 5;
/// Comprimento padrão da haste (sp) — ~uma oitava, convenção de gravação.
pub const STEM_LEN: Sp = 3.5;
/// Largura aproximada da cabeça de nota (sp), usada para ancorar a haste à direita.
pub const NOTEHEAD_W: Sp = 1.18;
/// Quanto a linha suplementar se estende além da cabeça, de cada lado (sp).
pub const LEDGER_EXT: Sp = 0.4;
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

/// X onde começa o primeiro compasso de um sistema (após margem + clave).
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
    /// `ElementId` de origem (para hit-testing/seleção), quando o glifo pertence a um
    /// `ChordOrRest` — cabeça de nota OU pausa. `None` para clave e bandeirola.
    pub element_id: Option<u64>,
}

/// Barra de compasso vertical: vai de `y_top` até `y_top + (STAFF_LINE_COUNT-1)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Barline {
    pub x: Sp,
    pub y_top: Sp,
}

/// Haste de uma nota/acorde: segmento vertical em `x`, de `y_top` a `y_bottom`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stem {
    pub x: Sp,
    pub y_top: Sp,
    pub y_bottom: Sp,
}

/// Linha suplementar (ledger): segmento horizontal em `y`, de `x1` a `x2`, para notas fora
/// das 5 linhas da pauta.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LedgerLine {
    pub x1: Sp,
    pub x2: Sp,
    pub y: Sp,
}

/// Uma linha de pauta (sistema): uma fileira horizontal de compassos.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct System {
    /// Y da linha superior desta pauta (sp).
    pub y_top: Sp,
    /// X do extremo direito ocupado (sp) — onde termina a última barra.
    pub width: Sp,
}

/// A caixa de um compasso já diagramado, usada para mapear tempo → posição (ex.: cursor).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeasureBox {
    /// Instante absoluto (semibreves) do início do compasso.
    pub start_whole: f64,
    /// Instante absoluto (semibreves) do fim do compasso.
    pub end_whole: f64,
    /// X (sp) onde o conteúdo começa (após o respiro esquerdo).
    pub x_notes: Sp,
    /// Y da linha superior do sistema em que o compasso está.
    pub y_top: Sp,
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
    pub measures: Vec<MeasureBox>,
    pub glyphs: Vec<PlacedGlyph>,
    pub barlines: Vec<Barline>,
    pub stems: Vec<Stem>,
    pub ledgers: Vec<LedgerLine>,
}

impl StaffLayout {
    /// Posição `(x, y_top)` do cursor para um instante absoluto (semibreves), localizando
    /// o compasso que o contém. `None` se o score não tem compassos.
    pub fn cursor_at(&self, abs_whole: f64) -> Option<(Sp, Sp)> {
        let last = self.measures.len().saturating_sub(1);
        for (i, mb) in self.measures.iter().enumerate() {
            let within = abs_whole >= mb.start_whole
                && (abs_whole < mb.end_whole || (i == last && abs_whole <= mb.end_whole));
            if within {
                let x = mb.x_notes + (abs_whole - mb.start_whole) * SPACE_PER_WHOLE;
                return Some((x, mb.y_top));
            }
        }
        // Fora do alcance: ancora no fim do último compasso.
        self.measures.last().map(|mb| {
            (
                mb.x_notes + (mb.end_whole - mb.start_whole) * SPACE_PER_WHOLE,
                mb.y_top,
            )
        })
    }
}

/// Y de uma linha da pauta (0 = superior .. 4 = inferior), em sp.
pub fn staff_line_y(line_from_top: u32) -> Sp {
    line_from_top as Sp
}

/// Acrescenta as linhas suplementares para uma cabeça em `y` (relativo à pauta: 0 = linha de
/// cima, 4 = de baixo), deslocadas pelo `yof` do sistema. Linhas acima (y <= -1) e abaixo
/// (y >= 5), uma por posição inteira entre a pauta e a nota.
fn push_ledgers(ledgers: &mut Vec<LedgerLine>, x: Sp, y: Sp, yof: Sp) {
    let (x1, x2) = (x - LEDGER_EXT, x + NOTEHEAD_W + LEDGER_EXT);
    if y >= 5.0 {
        let mut p = 5;
        while (p as Sp) <= y {
            ledgers.push(LedgerLine {
                x1,
                x2,
                y: p as Sp + yof,
            });
            p += 1;
        }
    } else if y <= -1.0 {
        let mut p = -1;
        while (p as Sp) >= y {
            ledgers.push(LedgerLine {
                x1,
                x2,
                y: p as Sp + yof,
            });
            p -= 1;
        }
    }
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
        element_id: None,
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

    let mut glyphs: Vec<PlacedGlyph> = Vec::new();
    let mut barlines: Vec<Barline> = Vec::new();
    let mut stems: Vec<Stem> = Vec::new();
    let mut ledgers: Vec<LedgerLine> = Vec::new();
    let mut systems: Vec<System> = Vec::new();
    let mut measures: Vec<MeasureBox> = Vec::new();

    let mut sys_index: usize = 0;
    let mut sys_x = content_start(); // x (sp) onde começa o próximo compasso no sistema atual
    let mut abs = 0.0_f64; // instante absoluto (semibreves) no início do compasso atual
    let mut cur_dur = 1.0_f64; // 4/4 por padrão

    let y_top = |i: usize| i as Sp * system_height();

    push_clef(&mut glyphs, clef, y_top(0));

    for m in &score.measures {
        if let Some(ts) = m.time_signature {
            cur_dur = ts.measure_duration().to_f64();
        }
        let box_w = cur_dur * SPACE_PER_WHOLE + 2.0 * MEASURE_PAD;

        // Quebra: se o sistema já tem conteúdo e somar este compasso passaria da largura.
        if sys_x > content_start() && sys_x + box_w > max_width {
            systems.push(System {
                y_top: y_top(sys_index),
                width: sys_x,
            });
            sys_index += 1;
            sys_x = content_start();
            push_clef(&mut glyphs, clef, y_top(sys_index));
        }

        let x_notes = sys_x + MEASURE_PAD;

        for v in &m.voices {
            for cr in &v.elements {
                let x = x_notes + cr.start.to_f64() * SPACE_PER_WHOLE;

                if cr.is_rest {
                    glyphs.push(PlacedGlyph {
                        pos: Point {
                            x,
                            y: 2.0 + y_top(sys_index),
                        },
                        glyph: Glyph::rest_for(cr.duration.base),
                        element_id: Some(cr.id.value()),
                    });
                    continue;
                }

                let yof = y_top(sys_index);
                let mut ys: Vec<Sp> = Vec::new();
                for n in &cr.notes {
                    if let Some(y) = vertical::note_y(&n.pitch, clef) {
                        let yy = y + yof;
                        glyphs.push(PlacedGlyph {
                            pos: Point { x, y: yy },
                            glyph: Glyph::notehead_for(cr.duration.base),
                            element_id: Some(cr.id.value()),
                        });
                        ys.push(yy);
                        push_ledgers(&mut ledgers, x, y, yof);
                    }
                    // pitch inválido: sem glifo (não há como posicionar)
                }

                // Haste + bandeirola (indicam a duração). Semibreve não tem haste.
                if !ys.is_empty() && Glyph::has_stem(cr.duration.base) {
                    let min_y = ys.iter().cloned().fold(Sp::INFINITY, Sp::min);
                    let max_y = ys.iter().cloned().fold(Sp::NEG_INFINITY, Sp::max);
                    let mid = 2.0 + y_top(sys_index); // linha do meio
                                                      // Direção pela nota mais distante do meio: abaixo/na linha => haste p/ cima.
                    let farthest = ys
                        .iter()
                        .cloned()
                        .max_by(|a, b| (a - mid).abs().total_cmp(&(b - mid).abs()))
                        .unwrap();
                    let up = farthest >= mid;

                    let (stem_x, y_t, y_b, flag_y) = if up {
                        (x + NOTEHEAD_W, min_y - STEM_LEN, max_y, min_y - STEM_LEN)
                    } else {
                        (x, min_y, max_y + STEM_LEN, max_y + STEM_LEN)
                    };
                    stems.push(Stem {
                        x: stem_x,
                        y_top: y_t,
                        y_bottom: y_b,
                    });

                    if let Some(flag) = Glyph::flag_for(cr.duration.base, up) {
                        glyphs.push(PlacedGlyph {
                            pos: Point {
                                x: stem_x,
                                y: flag_y,
                            },
                            glyph: flag,
                            element_id: None,
                        });
                    }
                }
            }
        }

        measures.push(MeasureBox {
            start_whole: abs,
            end_whole: abs + cur_dur,
            x_notes,
            y_top: y_top(sys_index),
        });

        // Barra ao fim do compasso (fim da caixa).
        let barline_x = sys_x + box_w;
        barlines.push(Barline {
            x: barline_x,
            y_top: y_top(sys_index),
        });

        sys_x += box_w;
        abs += cur_dur;
    }

    // Fecha o último sistema (ou um sistema vazio, se o score não tem compassos).
    let last_width = if score.measures.is_empty() {
        content_start() + SPACE_PER_WHOLE
    } else {
        sys_x
    };
    systems.push(System {
        y_top: y_top(sys_index),
        width: last_width,
    });

    let width = systems.iter().map(|s| s.width).fold(0.0_f64, f64::max);
    let height = y_top(sys_index) + (STAFF_LINE_COUNT as Sp - 1.0) + 2.0;

    StaffLayout {
        clef,
        line_count: STAFF_LINE_COUNT,
        width,
        height,
        systems,
        measures,
        glyphs,
        barlines,
        stems,
        ledgers,
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

    fn c_sig() -> TimeSignature {
        TimeSignature {
            numerator: 4,
            denominator: 4,
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
                time_signature: Some(c_sig()),
                key_signature: Some(0),
                chord_symbols: vec![],
                voices,
                tuplets: vec![],
            }],
        }
    }

    fn noteheads(l: &StaffLayout) -> Vec<&PlacedGlyph> {
        l.glyphs
            .iter()
            .filter(|g| {
                matches!(
                    g.glyph,
                    Glyph::NoteheadBlack | Glyph::NoteheadHalf | Glyph::NoteheadWhole
                )
            })
            .collect()
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
                                         // 1ª nota não fica colada na barra: começa após o respiro.
        let x_esperado = MARGIN + CLEF_WIDTH + MEASURE_PAD;
        assert_eq!(heads[0].pos.x, x_esperado);
        assert_eq!(heads[0].element_id, Some(1)); // id do ChordOrRest
    }

    #[test]
    fn primeira_nota_tem_respiro_apos_a_barra() {
        // Compasso 2 começa logo após a barra do compasso 1; sua 1ª nota também tem respiro.
        let s = score_measures(vec![
            whole_measure(0, Some(c_sig()), 1),
            whole_measure(1, None, 2),
        ]);
        let l = layout_single_staff(&s, MAX_SYSTEM_WIDTH);
        let heads = noteheads(&l);
        // nota do 2º compasso = barra do 1º + respiro
        assert_eq!(heads[1].pos.x, l.barlines[0].x + MEASURE_PAD);
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
        let base = MARGIN + CLEF_WIDTH + MEASURE_PAD;
        assert_eq!(heads[0].pos.x, base);
        assert_eq!(heads[1].pos.x, base + SPACE_PER_WHOLE * 0.25);
    }

    #[test]
    fn semibreve_nao_tem_haste_mas_seminima_tem() {
        let whole = score_1m(
            StaffType::Single(Clef::Treble),
            vec![Voice {
                voice_id: 0,
                elements: vec![elem(
                    1,
                    Fraction::ZERO,
                    DurationBase::Whole,
                    vec![note(10, 'C', 4)],
                )],
            }],
        );
        assert!(layout_single_staff(&whole, MAX_SYSTEM_WIDTH)
            .stems
            .is_empty());

        let quarter = score_1m(
            StaffType::Single(Clef::Treble),
            vec![Voice {
                voice_id: 0,
                elements: vec![elem(
                    1,
                    Fraction::ZERO,
                    DurationBase::Quarter,
                    vec![note(10, 'C', 4)],
                )],
            }],
        );
        let l = layout_single_staff(&quarter, MAX_SYSTEM_WIDTH);
        assert_eq!(l.stems.len(), 1);
        // C4 (abaixo do meio) => haste para cima: fica à direita e sobe acima da cabeça.
        let s = l.stems[0];
        assert!(s.y_top < s.y_bottom);
        assert!(l
            .glyphs
            .iter()
            .all(|g| !matches!(g.glyph, Glyph::Flag8thUp | Glyph::Flag8thDown)));
        // semínima não tem bandeirola
    }

    #[test]
    fn colcheia_ganha_bandeirola() {
        let s = score_1m(
            StaffType::Single(Clef::Treble),
            vec![Voice {
                voice_id: 0,
                elements: vec![elem(
                    1,
                    Fraction::ZERO,
                    DurationBase::Eighth,
                    vec![note(10, 'C', 4)],
                )],
            }],
        );
        let l = layout_single_staff(&s, MAX_SYSTEM_WIDTH);
        assert_eq!(l.stems.len(), 1);
        assert!(l.glyphs.iter().any(|g| g.glyph == Glyph::Flag8thUp));
    }

    #[test]
    fn direcao_da_haste_segue_a_altura() {
        // Nota aguda (acima do meio) => haste para baixo (ancorada à esquerda, x da cabeça).
        let aguda = score_1m(
            StaffType::Single(Clef::Treble),
            vec![Voice {
                voice_id: 0,
                elements: vec![elem(
                    1,
                    Fraction::ZERO,
                    DurationBase::Quarter,
                    vec![note(10, 'G', 5)],
                )],
            }],
        );
        let l = layout_single_staff(&aguda, MAX_SYSTEM_WIDTH);
        let head_x = l
            .glyphs
            .iter()
            .find(|g| g.glyph == Glyph::NoteheadBlack)
            .unwrap()
            .pos
            .x;
        // haste para baixo fica no x da cabeça; para cima ficaria em head_x + NOTEHEAD_W.
        assert_eq!(l.stems[0].x, head_x);
    }

    #[test]
    fn nota_fora_da_pauta_ganha_linha_suplementar() {
        // C4 em clave de sol fica em y=5 (1ª suplementar abaixo).
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
        assert_eq!(l.ledgers.len(), 1);
        assert_eq!(l.ledgers[0].y, 5.0);
    }

    #[test]
    fn nota_dentro_da_pauta_nao_tem_suplementar() {
        // G4 fica na linha central da clave de sol (y=3), dentro da pauta.
        let v = Voice {
            voice_id: 0,
            elements: vec![elem(
                1,
                Fraction::ZERO,
                DurationBase::Quarter,
                vec![note(10, 'G', 4)],
            )],
        };
        let l = layout_single_staff(
            &score_1m(StaffType::Single(Clef::Treble), vec![v]),
            MAX_SYSTEM_WIDTH,
        );
        assert!(l.ledgers.is_empty());
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
            whole_measure(0, Some(c_sig()), 1),
            whole_measure(1, None, 2),
        ]);
        let l = layout_single_staff(&s, MAX_SYSTEM_WIDTH);
        assert_eq!(l.barlines.len(), 2);
        let box_w = SPACE_PER_WHOLE + 2.0 * MEASURE_PAD;
        assert_eq!(l.barlines[0].x, MARGIN + CLEF_WIDTH + box_w);
        assert_eq!(l.barlines[1].x, MARGIN + CLEF_WIDTH + 2.0 * box_w);
        assert_eq!(l.systems.len(), 1);
    }

    #[test]
    fn quebra_em_sistemas_ao_passar_da_largura() {
        // Largura de teste explícita (independe da constante de produção).
        // 6 compassos de semibreve: caixa de cada = 24 + 2*2.5 = 29 sp; conteúdo começa em 6.
        // Limite 90 sp => cabem 3 por sistema (6 + 3*29 = 93? não: 6+29=35,64,93>90 => 2? ver abaixo).
        const W: Sp = 100.0;
        let measures: Vec<Measure> = (0..6)
            .map(|i| {
                let ts = if i == 0 { Some(c_sig()) } else { None };
                whole_measure(i, ts, (i + 1) as u64)
            })
            .collect();
        let l = layout_single_staff(&score_measures(measures), W);
        // box_w = 29; 6+29=35,64,93 <=100; +29=122>100 => 3 por sistema => 2 sistemas.
        assert_eq!(l.systems.len(), 2, "esperava 2 sistemas");
        assert!(l.systems[1].y_top > l.systems[0].y_top);
        assert_eq!(noteheads(&l).len(), 6);
        assert!(l.width <= W);
        let clefs = l.glyphs.iter().filter(|g| g.glyph == Glyph::GClef).count();
        assert_eq!(clefs, 2);
    }

    #[test]
    fn cursor_localiza_o_sistema_certo() {
        const W: Sp = 100.0; // 3 compassos por sistema
        let measures: Vec<Measure> = (0..6)
            .map(|i| {
                let ts = if i == 0 { Some(c_sig()) } else { None };
                whole_measure(i, ts, (i + 1) as u64)
            })
            .collect();
        let l = layout_single_staff(&score_measures(measures), W);
        // Instante 0 -> 1º compasso, início do conteúdo (após o respiro).
        let (x0, y0) = l.cursor_at(0.0).unwrap();
        assert_eq!(y0, l.systems[0].y_top);
        assert_eq!(x0, MARGIN + CLEF_WIDTH + MEASURE_PAD);
        // Instante 4 (4º compasso) -> 2º sistema (começa no compasso 3).
        let (_, y4) = l.cursor_at(4.0).unwrap();
        assert_eq!(y4, l.systems[1].y_top);
    }
}
