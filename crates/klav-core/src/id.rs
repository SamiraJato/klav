//! IDs estáveis e tipados para os elementos musicais.
//!
//! Newtypes sobre `u64`: compactos e performáticos para WASM, e o sistema de tipos
//! impede trocar um `NoteId` por um `ElementId` por engano.

use serde::{Deserialize, Serialize};

macro_rules! define_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(pub u64);

        impl $name {
            #[inline]
            pub fn value(&self) -> u64 {
                self.0
            }
        }

        impl From<u64> for $name {
            #[inline]
            fn from(v: u64) -> Self {
                $name(v)
            }
        }
    };
}

define_id!(
    /// Identifica uma `Note` individual (usado por ties).
    NoteId
);
define_id!(
    /// Identifica um `ChordOrRest`.
    ElementId
);
define_id!(
    /// Identifica um `Tuplet` (quiáltera).
    TupletId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newtypes_sao_distintos_mas_comparaveis() {
        assert_eq!(NoteId(1), NoteId(1));
        assert_ne!(NoteId(1), NoteId(2));
        assert_eq!(NoteId::from(7).value(), 7);
    }
}
