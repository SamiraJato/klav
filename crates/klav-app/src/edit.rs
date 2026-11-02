//! Edição da partitura: transposição diatônica da nota selecionada.
//!
//! A lógica musical é pura (testável sem UI); só `transpose_active` toca nos `Signal`.

use dioxus::prelude::*;

use klav_core::{Pitch, Score};

use crate::document::Tab;

/// Move a nota selecionada da aba ativa um passo diatônico.
pub fn transpose_active(mut tabs: Signal<Vec<Tab>>, active: Signal<usize>, up: bool) {
    let a = *active.peek();
    let mut w = tabs.write();
    if let Some(tab) = w.get_mut(a) {
        if let Some(id) = tab.selected {
            transpose_in_score(&mut tab.score, id, up);
        }
    }
}

/// Aplica a transposição diatônica à nota de id `id` em todo o Score.
pub fn transpose_in_score(score: &mut Score, id: u64, up: bool) {
    for m in score.measures.iter_mut() {
        for v in m.voices.iter_mut() {
            for cr in v.elements.iter_mut() {
                for n in cr.notes.iter_mut() {
                    if n.id.value() == id {
                        n.pitch = step_pitch(n.pitch, up);
                    }
                }
            }
        }
    }
}

/// Sobe/desce uma nota um passo diatônico (C→D→…→B→C+8ª), preservando o acidente.
pub fn step_pitch(p: Pitch, up: bool) -> Pitch {
    const ORDER: [char; 7] = ['C', 'D', 'E', 'F', 'G', 'A', 'B'];
    let Some(idx) = ORDER.iter().position(|&c| c == p.step.to_ascii_uppercase()) else {
        return p;
    };
    let (new_idx, new_oct) = if up {
        if idx == 6 {
            (0, p.octave + 1)
        } else {
            (idx + 1, p.octave)
        }
    } else if idx == 0 {
        (6, p.octave - 1)
    } else {
        (idx - 1, p.octave)
    };
    Pitch::new(ORDER[new_idx], new_oct, p.accidental)
}

#[cfg(test)]
mod tests {
    use super::*;
    use klav_core::Accidental;

    #[test]
    fn sobe_dentro_da_oitava() {
        let p = step_pitch(Pitch::new('C', 4, None), true);
        assert_eq!((p.step, p.octave), ('D', 4));
    }

    #[test]
    fn sobe_de_b_para_c_da_proxima_oitava() {
        let p = step_pitch(Pitch::new('B', 4, None), true);
        assert_eq!((p.step, p.octave), ('C', 5));
    }

    #[test]
    fn desce_de_c_para_b_da_oitava_anterior() {
        let p = step_pitch(Pitch::new('C', 4, None), false);
        assert_eq!((p.step, p.octave), ('B', 3));
    }

    #[test]
    fn preserva_o_acidente() {
        let p = step_pitch(Pitch::new('C', 4, Some(Accidental::Sharp)), true);
        assert_eq!((p.step, p.octave, p.accidental), ('D', 4, Some(Accidental::Sharp)));
    }

    #[test]
    fn step_invalido_nao_muda() {
        let orig = Pitch::new('H', 4, None);
        let p = step_pitch(orig, true);
        assert_eq!((p.step, p.octave), ('H', 4));
    }
}
