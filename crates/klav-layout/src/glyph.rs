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
    Flag8thUp,
    Flag8thDown,
    Flag16thUp,
    Flag16thDown,
    Flag32ndUp,
    Flag32ndDown,
    Flag64thUp,
    Flag64thDown,
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
            Glyph::Flag8thUp => 0xE240,
            Glyph::Flag8thDown => 0xE241,
            Glyph::Flag16thUp => 0xE242,
            Glyph::Flag16thDown => 0xE243,
            Glyph::Flag32ndUp => 0xE244,
            Glyph::Flag32ndDown => 0xE245,
            Glyph::Flag64thUp => 0xE246,
            Glyph::Flag64thDown => 0xE247,
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

    /// `true` se a duração leva haste (tudo menos a semibreve).
    pub fn has_stem(base: DurationBase) -> bool {
        base != DurationBase::Whole
    }

    /// Bandeirola para a duração e direção da haste (`up`), ou `None` se não tem bandeirola
    /// (semibreve, mínima, semínima).
    pub fn flag_for(base: DurationBase, up: bool) -> Option<Glyph> {
        Some(match (base, up) {
            (DurationBase::Eighth, true) => Glyph::Flag8thUp,
            (DurationBase::Eighth, false) => Glyph::Flag8thDown,
            (DurationBase::Sixteenth, true) => Glyph::Flag16thUp,
            (DurationBase::Sixteenth, false) => Glyph::Flag16thDown,
            (DurationBase::ThirtySecond, true) => Glyph::Flag32ndUp,
            (DurationBase::ThirtySecond, false) => Glyph::Flag32ndDown,
            (DurationBase::SixtyFourth, true) => Glyph::Flag64thUp,
            (DurationBase::SixtyFourth, false) => Glyph::Flag64thDown,
            _ => return None,
        })
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
        assert_eq!(
            Glyph::notehead_for(DurationBase::Whole),
            Glyph::NoteheadWhole
        );
        assert_eq!(Glyph::notehead_for(DurationBase::Half), Glyph::NoteheadHalf);
        assert_eq!(
            Glyph::notehead_for(DurationBase::Quarter),
            Glyph::NoteheadBlack
        );
        assert_eq!(
            Glyph::notehead_for(DurationBase::Eighth),
            Glyph::NoteheadBlack
        );
    }

    #[test]
    fn pausa_por_duracao() {
        assert_eq!(Glyph::rest_for(DurationBase::Quarter), Glyph::RestQuarter);
        assert_eq!(Glyph::rest_for(DurationBase::Eighth), Glyph::Rest8th);
    }

    #[test]
    fn haste_so_nao_na_semibreve() {
        assert!(!Glyph::has_stem(DurationBase::Whole));
        assert!(Glyph::has_stem(DurationBase::Half));
        assert!(Glyph::has_stem(DurationBase::Quarter));
        assert!(Glyph::has_stem(DurationBase::Eighth));
    }

    #[test]
    fn bandeirola_so_de_colcheia_pra_baixo() {
        assert_eq!(Glyph::flag_for(DurationBase::Quarter, true), None);
        assert_eq!(Glyph::flag_for(DurationBase::Half, false), None);
        assert_eq!(
            Glyph::flag_for(DurationBase::Eighth, true),
            Some(Glyph::Flag8thUp)
        );
        assert_eq!(
            Glyph::flag_for(DurationBase::Eighth, false),
            Some(Glyph::Flag8thDown)
        );
        assert_eq!(
            Glyph::flag_for(DurationBase::Sixteenth, true),
            Some(Glyph::Flag16thUp)
        );
    }
}
