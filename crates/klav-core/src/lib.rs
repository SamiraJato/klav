//! # klav-core
//!
//! Modelo de dados musical do klav — puro, sem UI, sem áudio, sem geometria.
//!
//! Ver `docs/arquitetura.md` para o plano completo. Pontos-chave:
//! - Tempo/duração sempre em [`Fraction`] (fração de semibreve), nunca float.
//! - Grafia é a fonte da verdade para [`Pitch`]; o número MIDI é derivado.
//! - Ties direcionais via [`id::NoteId`]; posição explícita em cada [`model::ChordOrRest`].

pub mod fraction;
pub mod id;
pub mod model;
pub mod pitch;
pub mod validate;

pub use fraction::Fraction;
pub use id::{ElementId, NoteId, TupletId};
pub use model::{
    ChordOrRest, ChordSymbol, Clef, Duration, DurationBase, KeySignature, Measure, Note, Score,
    StaffType, TimeSignature, Tuplet, Voice, VOICES,
};
pub use pitch::{Accidental, Pitch};
pub use validate::ValidationError;
