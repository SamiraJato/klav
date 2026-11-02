//! Geometria do teclado de 88 teclas (A0=21 .. C8=108).
//!
//! Função pura, sem UI: facilita teste e mantém o render em `main.rs` declarativo.

/// Largura de uma tecla branca (em unidades do view-box do teclado).
pub const WHITE_W: f64 = 15.0;
/// Largura de uma tecla preta.
pub const BLACK_W: f64 = 9.0;

/// Fila de teclas: cada item é `(nota_midi, x)`.
pub type KeyRow = Vec<(u8, f64)>;

/// Posições das 88 teclas: (brancas, pretas, largura total).
///
/// As pretas ficam centradas na junção entre brancas.
pub fn keyboard_geometry() -> (KeyRow, KeyRow, f64) {
    let mut whites = Vec::new();
    let mut blacks = Vec::new();
    let mut wx = 0.0;
    for midi in 21u8..=108 {
        let pc = midi % 12;
        if matches!(pc, 1 | 3 | 6 | 8 | 10) {
            blacks.push((midi, wx - BLACK_W / 2.0));
        } else {
            whites.push((midi, wx));
            wx += WHITE_W;
        }
    }
    (whites, blacks, wx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contagem_de_teclas() {
        let (whites, blacks, width) = keyboard_geometry();
        // 88 teclas: 52 brancas + 36 pretas.
        assert_eq!(whites.len(), 52);
        assert_eq!(blacks.len(), 36);
        assert_eq!(whites.len() + blacks.len(), 88);
        assert_eq!(width, 52.0 * WHITE_W);
    }

    #[test]
    fn primeira_e_ultima_tecla() {
        let (whites, _, _) = keyboard_geometry();
        assert_eq!(whites.first().unwrap().0, 21); // A0
        assert_eq!(whites.last().unwrap().0, 108); // C8
    }
}
