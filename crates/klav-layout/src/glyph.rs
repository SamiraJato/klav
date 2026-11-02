//! Glifos musicais no padrão **SMuFL** (Standard Music Font Layout).
//!
//! Cada variante mapeia para um code point Unicode da área de uso privado definida pelo
//! SMuFL (implementada pela fonte Bravura, entre outras). O backend de render apenas
//! desenha o caractere na posição que o layout calculou — ele não precisa saber o que é.

use klav_core::DurationBase;

/// Um glifo SMuFL que o layout pode posicionar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    GClef,
    FClef,
    NoteheadWhole,
    NoteheadHalf,
    NoteheadBlack,
    RestWhole,
    RestHalf,
    RestQuarter,
    Rest8th,
    Rest16th,
    Rest32nd,
    Rest64th,
    AccidentalFlat,
    AccidentalNatural,
    AccidentalSharp,
    AccidentalDoubleSharp,
    AccidentalDoubleFlat,
}

impl Glyph {
    /// Code point SMuFL correspondente.
    pub fn codepoint(&self) -> char {
        let cp: u32 = match self {
            Glyph::GClef => 0xE050,
            Glyph::FClef => 0xE062,
            Glyph::NoteheadWhole => 0xE0A2,
            Glyph::NoteheadHalf => 0xE0A3,
            Glyph::NoteheadBlack => 0xE0A4,
            Glyph::RestWhole => 0xE4E3,
            Glyph::RestHalf => 0xE4E4,
            Glyph::RestQuarter => 0xE4E5,
            Glyph::Rest8th => 0xE4E6,
            Glyph::Rest16th => 0xE4E7,
            Glyph::Rest32nd => 0xE4E8,
            Glyph::Rest64th => 0xE4E9,
            Glyph::AccidentalFlat => 0xE260,
            Glyph::AccidentalNatural => 0xE261,
            Glyph::AccidentalSharp => 0xE262,
            Glyph::AccidentalDoubleSharp => 0xE263,
            Glyph::AccidentalDoubleFlat => 0xE264,
        };
        // Todos os code points acima são válidos; o unwrap nunca falha.
        char::from_u32(cp).expect("code point SMuFL inválido")
    }

    /// Cabeça de nota adequada para a duração.
    pub fn notehead_for(base: DurationBase) -> Glyph {
        match base {
            DurationBase::Whole => Glyph::NoteheadWhole,
            DurationBase::Half => Glyph::NoteheadHalf,
            _ => Glyph::NoteheadBlack,
        }
    }

    /// Pausa adequada para a duração.
    pub fn rest_for(base: DurationBase) -> Glyph {
        match base {
            DurationBase::Whole => Glyph::RestWhole,
            DurationBase::Half => Glyph::RestHalf,
            DurationBase::Quarter => Glyph::RestQuarter,
            DurationBase::Eighth => Glyph::Rest8th,
            DurationBase::Sixteenth => Glyph::Rest16th,
            DurationBase::ThirtySecond => Glyph::Rest32nd,
            DurationBase::SixtyFourth => Glyph::Rest64th,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codepoints_conhecidos() {
        assert_eq!(Glyph::GClef.codepoint(), '\u{E050}');
        assert_eq!(Glyph::NoteheadBlack.codepoint(), '\u{E0A4}');
    }

    #[test]
    fn cabeca_por_duracao() {
        assert_eq!(Glyph::notehead_for(DurationBase::Whole), Glyph::NoteheadWhole);
        assert_eq!(Glyph::notehead_for(DurationBase::Half), Glyph::NoteheadHalf);
        assert_eq!(Glyph::notehead_for(DurationBase::Quarter), Glyph::NoteheadBlack);
        assert_eq!(Glyph::notehead_for(DurationBase::Eighth), Glyph::NoteheadBlack);
    }

    #[test]
    fn pausa_por_duracao() {
        assert_eq!(Glyph::rest_for(DurationBase::Quarter), Glyph::RestQuarter);
        assert_eq!(Glyph::rest_for(DurationBase::Eighth), Glyph::Rest8th);
    }
}
