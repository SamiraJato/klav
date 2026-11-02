//! Posição vertical de uma nota na pauta, a partir do pitch e da clave.
//!
//! Unidade: **staff-space (sp)** — a distância entre duas linhas adjacentes da pauta.
//! O eixo Y cresce para baixo (convenção de tela): a **linha superior** da pauta é `y = 0`
//! e a **linha inferior** é `y = 4.0` (5 linhas, 4 espaços). Cada passo diatônico vale 0.5 sp.
//!
//! A posição vertical depende só de `step` + `octave` — o acidente **não** muda a linha
//! (é esse o ponto: Dó e Dó♯ ficam na mesma linha).

use klav_core::{Clef, Pitch};

/// Número diatônico absoluto e monotônico de um pitch (C=0..B=6 dentro da oitava).
///
/// `None` se o `step` não for 'A'..='G'. Independe de acidente.
pub fn diatonic_number(pitch: &Pitch) -> Option<i32> {
    let step_index = match pitch.step.to_ascii_uppercase() {
        'C' => 0,
        'D' => 1,
        'E' => 2,
        'F' => 3,
        'G' => 4,
        'A' => 5,
        'B' => 6,
        _ => return None,
    };
    Some(pitch.octave as i32 * 7 + step_index)
}

/// Número diatônico da nota que fica sobre a **linha superior** da pauta, por clave.
///
/// - Clave de Sol: linha superior = Fá5 (F5).
/// - Clave de Fá: linha superior = Lá3 (A3).
fn top_line_diatonic(clef: Clef) -> i32 {
    match clef {
        Clef::Treble => diatonic_number(&Pitch::new('F', 5, None)).unwrap(),
        Clef::Bass => diatonic_number(&Pitch::new('A', 3, None)).unwrap(),
    }
}

/// Posição vertical da nota em staff-spaces, medida para baixo a partir da linha superior.
///
/// `0.0` = linha superior; `4.0` = linha inferior; valores `< 0` ficam acima da pauta e
/// `> 4` abaixo (linhas suplementares). `None` se o pitch for inválido.
pub fn note_y(pitch: &Pitch, clef: Clef) -> Option<f64> {
    let d = diatonic_number(pitch)?;
    let top = top_line_diatonic(clef);
    // Cada passo diatônico abaixo da linha superior desce 0.5 sp.
    Some((top - d) as f64 * 0.5)
}

/// `true` se a nota cai dentro das 5 linhas da pauta (`0.0..=4.0`).
pub fn within_staff(pitch: &Pitch, clef: Clef) -> bool {
    note_y(pitch, clef)
        .map(|y| (0.0..=4.0).contains(&y))
        .unwrap_or(false)
}

/// Quantidade de linhas suplementares necessárias (0 se estiver dentro da pauta).
///
/// Positivo tanto acima quanto abaixo; o sinal fica implícito na posição Y.
pub fn ledger_line_count(pitch: &Pitch, clef: Clef) -> u32 {
    match note_y(pitch, clef) {
        Some(y) if y < 0.0 => {
            // Linhas suplementares acima: em y = -1, -2, ... (passos de 1 sp).
            (-y).floor() as u32
        }
        Some(y) if y > 4.0 => {
            // Abaixo: em y = 5, 6, ...
            (y - 4.0).floor() as u32
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(step: char, octave: i8) -> Pitch {
        Pitch::new(step, octave, None)
    }

    #[test]
    fn diatonico_monotonico() {
        assert!(diatonic_number(&p('C', 4)) < diatonic_number(&p('D', 4)));
        assert_eq!(
            diatonic_number(&p('C', 5)).unwrap(),
            diatonic_number(&p('C', 4)).unwrap() + 7
        );
    }

    #[test]
    fn acidente_nao_muda_posicao() {
        use klav_core::Accidental;
        let natural = note_y(&p('C', 4), Clef::Treble);
        let sharp = note_y(&Pitch::new('C', 4, Some(Accidental::Sharp)), Clef::Treble);
        assert_eq!(natural, sharp);
    }

    #[test]
    fn linhas_da_clave_de_sol() {
        // Topo->base: F5(0), D5(1), B4(2), G4(3), E4(4)
        assert_eq!(note_y(&p('F', 5), Clef::Treble), Some(0.0));
        assert_eq!(note_y(&p('D', 5), Clef::Treble), Some(1.0));
        assert_eq!(note_y(&p('B', 4), Clef::Treble), Some(2.0));
        assert_eq!(note_y(&p('G', 4), Clef::Treble), Some(3.0));
        assert_eq!(note_y(&p('E', 4), Clef::Treble), Some(4.0));
    }

    #[test]
    fn do_central_na_clave_de_sol_fica_abaixo() {
        // C4 (dó central) fica na 1ª suplementar abaixo da pauta de sol: y = 5.0
        assert_eq!(note_y(&p('C', 4), Clef::Treble), Some(5.0));
        assert_eq!(ledger_line_count(&p('C', 4), Clef::Treble), 1);
        assert!(!within_staff(&p('C', 4), Clef::Treble));
    }

    #[test]
    fn linhas_da_clave_de_fa() {
        // Topo->base: A3(0), F3(1), D3(2), B2(3), G2(4)
        assert_eq!(note_y(&p('A', 3), Clef::Bass), Some(0.0));
        assert_eq!(note_y(&p('G', 2), Clef::Bass), Some(4.0));
    }

    #[test]
    fn do_central_na_clave_de_fa_fica_acima() {
        // C4 fica na 1ª suplementar acima da pauta de fá: y = -1.0
        assert_eq!(note_y(&p('C', 4), Clef::Bass), Some(-1.0));
        assert_eq!(ledger_line_count(&p('C', 4), Clef::Bass), 1);
    }

    #[test]
    fn espaco_entre_linhas() {
        // E5 (1º espaço de cima) fica em 0.5 sp.
        assert_eq!(note_y(&p('E', 5), Clef::Treble), Some(0.5));
    }
}
