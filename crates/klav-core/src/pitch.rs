//! Altura (Pitch) — a grafia (step + octave + accidental) é a FONTE DA VERDADE.
//!
//! `midi_number` é DERIVADO, não armazenado, evitando dessincronização entre o número
//! MIDI e a grafia (ex.: Dó# e Réb compartilham o MIDI 61 mas têm grafias distintas).

use serde::{Deserialize, Serialize};

/// Acidente (alteração) aplicado a uma nota.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Accidental {
    DoubleFlat,
    Flat,
    Natural,
    Sharp,
    DoubleSharp,
}

impl Accidental {
    /// Deslocamento em semitons.
    #[inline]
    pub fn semitone_offset(&self) -> i32 {
        match self {
            Accidental::DoubleFlat => -2,
            Accidental::Flat => -1,
            Accidental::Natural => 0,
            Accidental::Sharp => 1,
            Accidental::DoubleSharp => 2,
        }
    }
}

/// Altura de uma nota pela sua grafia.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pitch {
    /// Nome da nota: 'A'..='G' (maiúsculo).
    pub step: char,
    /// Oitava na convenção científica (C4 = Dó central = MIDI 60).
    pub octave: i8,
    /// Acidente; `None` significa a nota natural conforme a armadura.
    pub accidental: Option<Accidental>,
}

impl Pitch {
    pub fn new(step: char, octave: i8, accidental: Option<Accidental>) -> Pitch {
        Pitch {
            step,
            octave,
            accidental,
        }
    }

    /// Semitom do step dentro da oitava (C=0, D=2, E=4, F=5, G=7, A=9, B=11).
    /// `None` se `step` não for 'A'..='G'.
    fn step_semitone(step: char) -> Option<i32> {
        match step.to_ascii_uppercase() {
            'C' => Some(0),
            'D' => Some(2),
            'E' => Some(4),
            'F' => Some(5),
            'G' => Some(7),
            'A' => Some(9),
            'B' => Some(11),
            _ => None,
        }
    }

    /// `true` se a grafia é válida (step em A–G e MIDI resultante em 0..=127).
    pub fn is_valid(&self) -> bool {
        self.midi_number().is_some()
    }

    /// Número MIDI derivado da grafia. `None` se a grafia for inválida ou cair fora de 0..=127.
    ///
    /// Nunca é armazenado: é sempre recalculado a partir de step/octave/accidental.
    pub fn midi_number(&self) -> Option<u8> {
        let semitone = Self::step_semitone(self.step)?;
        let acc = self.accidental.map(|a| a.semitone_offset()).unwrap_or(0);
        // MIDI: C-1 = 0  =>  (octave + 1) * 12 + semitom + acidente
        let midi = (self.octave as i32 + 1) * 12 + semitone + acc;
        if (0..=127).contains(&midi) {
            Some(midi as u8)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn do_central_e_midi_60() {
        assert_eq!(Pitch::new('C', 4, None).midi_number(), Some(60));
    }

    #[test]
    fn la_de_referencia_e_midi_69() {
        assert_eq!(Pitch::new('A', 4, None).midi_number(), Some(69));
    }

    #[test]
    fn enarmonia_do_sustenido_igual_re_bemol() {
        let do_sharp = Pitch::new('C', 4, Some(Accidental::Sharp)).midi_number();
        let re_flat = Pitch::new('D', 4, Some(Accidental::Flat)).midi_number();
        assert_eq!(do_sharp, Some(61));
        assert_eq!(re_flat, Some(61));
    }

    #[test]
    fn limites_midi() {
        // C-1 = 0 (mínimo), G9 = 127 (máximo)
        assert_eq!(Pitch::new('C', -1, None).midi_number(), Some(0));
        assert_eq!(Pitch::new('G', 9, None).midi_number(), Some(127));
        // Fora do range -> None
        assert_eq!(Pitch::new('C', -2, None).midi_number(), None);
        assert_eq!(Pitch::new('A', 9, None).midi_number(), None); // 141
    }

    #[test]
    fn step_invalido_e_none() {
        assert_eq!(Pitch::new('H', 4, None).midi_number(), None);
        assert!(!Pitch::new('H', 4, None).is_valid());
    }
}
