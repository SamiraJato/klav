//! Modelo musical do klav. Dados puros: sem UI, sem áudio, sem geometria.
//!
//! Decisões principais (ver `docs/arquitetura.md`):
//! - Tempo/posição sempre em [`Fraction`] (fração de semibreve), nunca float.
//! - Cada [`ChordOrRest`] carrega `start` explícito no compasso (alinhamento entre vozes verificável).
//! - Ties são direcionais via [`NoteId`]; quiálteras referenciadas por [`TupletId`].

use serde::{Deserialize, Serialize};

use crate::fraction::Fraction;
use crate::id::{ElementId, NoteId, TupletId};
use crate::pitch::Pitch;

/// Número máximo de vozes por pauta (padrão da notação ocidental, igual ao MuseScore).
pub const VOICES: u8 = 4;

/// Clave.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Clef {
    Treble,
    Bass,
}

/// Tipo de pauta do documento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StaffType {
    /// Pauta simples (ex.: lead sheet em clave de Sol).
    Single(Clef),
    /// Pauta dupla Sol/Fá (piano).
    GrandStaff,
}

/// Fórmula de compasso (ex.: 4/4 -> numerator 4, denominator 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeSignature {
    pub numerator: u8,
    pub denominator: u8,
}

impl TimeSignature {
    /// Duração total do compasso como fração de semibreve (ex.: 4/4 -> 1, 6/8 -> 3/4).
    pub fn measure_duration(&self) -> Fraction {
        Fraction::new(self.numerator as i64, self.denominator as i64)
    }
}

/// Armadura de clave: nº de sustenidos (+) ou bemóis (-). 0 = Dó maior / Lá menor.
pub type KeySignature = i8;

/// Duração básica (potência de dois).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DurationBase {
    Whole,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
    SixtyFourth,
}

impl DurationBase {
    /// Fração de semibreve sem considerar pontos.
    pub fn base_fraction(&self) -> Fraction {
        let den = match self {
            DurationBase::Whole => 1,
            DurationBase::Half => 2,
            DurationBase::Quarter => 4,
            DurationBase::Eighth => 8,
            DurationBase::Sixteenth => 16,
            DurationBase::ThirtySecond => 32,
            DurationBase::SixtyFourth => 64,
        };
        Fraction::new(1, den)
    }
}

/// Duração de um evento: valor base + pontos de aumento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Duration {
    pub base: DurationBase,
    pub dots: u8,
}

impl Duration {
    pub fn new(base: DurationBase, dots: u8) -> Duration {
        Duration { base, dots }
    }

    /// Duração efetiva como fração, já aplicando os pontos.
    ///
    /// Cada ponto adiciona metade do valor anterior: `base * (2 - 1/2^dots)`.
    /// A quiáltera (tuplet), se houver, é aplicada fora, no contexto do compasso.
    pub fn as_fraction(&self) -> Fraction {
        let base = self.base.base_fraction();
        // Soma geométrica: base + base/2 + base/4 + ... (dots termos extras)
        let mut total = base;
        let mut add = base;
        for _ in 0..self.dots {
            add = add * Fraction::new(1, 2);
            total = total + add;
        }
        total
    }
}

/// Uma nota individual.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: NoteId,
    pub pitch: Pitch,
    /// Velocidade MIDI (0–127).
    pub velocity: u8,
    /// Ligadura de VALOR: aponta para a próxima nota ligada (de mesma altura).
    /// `None` = sem tie. Direcional, substitui o antigo `is_tied: bool`.
    pub tie_to: Option<NoteId>,
}

/// Um acorde (uma ou mais notas) ou uma pausa, com posição explícita no compasso.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChordOrRest {
    pub id: ElementId,
    /// Início DENTRO do compasso (0 = começo). Torna o alinhamento entre vozes explícito.
    pub start: Fraction,
    pub duration: Duration,
    pub is_rest: bool,
    /// Notas do acorde; vazio se `is_rest`.
    pub notes: Vec<Note>,
    /// Pertence a uma quiáltera? Resolvido via [`Measure::tuplets`].
    pub tuplet: Option<TupletId>,
}

/// Quiáltera (ex.: tercina de colcheias = 3 no tempo de 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tuplet {
    pub id: TupletId,
    /// Quantas notas são tocadas.
    pub actual: u8,
    /// No tempo de quantas.
    pub normal: u8,
    /// Unidade de duração da quiáltera.
    pub base: DurationBase,
    /// Elementos que a compõem.
    pub members: Vec<ElementId>,
}

/// Uma voz dentro de um compasso.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Voice {
    /// 0..VOICES.
    pub voice_id: u8,
    pub elements: Vec<ChordOrRest>,
}

/// Cifra / símbolo de acorde (lead sheet).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChordSymbol {
    /// Posição no compasso.
    pub start: Fraction,
    /// Texto da cifra (ex.: "Cm7", "G/B"). O parsing harmônico fica em camada separada.
    pub text: String,
}

/// Um compasso.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Measure {
    pub index: u32,
    /// `None` herda do compasso anterior.
    pub time_signature: Option<TimeSignature>,
    pub key_signature: Option<KeySignature>,
    pub chord_symbols: Vec<ChordSymbol>,
    pub voices: Vec<Voice>,
    pub tuplets: Vec<Tuplet>,
}

/// A partitura inteira (modelo de dados; layout e áudio ficam em outras crates).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Score {
    pub title: String,
    /// Andamento em BPM. Fixo por Score no MVP.
    pub tempo_bpm: u16,
    pub staff_type: StaffType,
    pub measures: Vec<Measure>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duracao_sem_pontos() {
        assert_eq!(
            Duration::new(DurationBase::Quarter, 0).as_fraction(),
            Fraction::new(1, 4)
        );
    }

    #[test]
    fn duracao_com_um_ponto() {
        // semínima pontuada = 1/4 + 1/8 = 3/8
        assert_eq!(
            Duration::new(DurationBase::Quarter, 1).as_fraction(),
            Fraction::new(3, 8)
        );
    }

    #[test]
    fn duracao_com_dois_pontos() {
        // semínima com duplo ponto = 1/4 + 1/8 + 1/16 = 7/16
        assert_eq!(
            Duration::new(DurationBase::Quarter, 2).as_fraction(),
            Fraction::new(7, 16)
        );
    }

    #[test]
    fn duracao_compasso_por_formula() {
        assert_eq!(
            TimeSignature {
                numerator: 4,
                denominator: 4
            }
            .measure_duration(),
            Fraction::ONE
        );
        assert_eq!(
            TimeSignature {
                numerator: 6,
                denominator: 8
            }
            .measure_duration(),
            Fraction::new(3, 4)
        );
    }
}
