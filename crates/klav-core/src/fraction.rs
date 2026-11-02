//! Posição e duração musical como fração de semibreve (whole note).
//!
//! Ex.: semínima = 1/4, colcheia = 1/8, colcheia pontuada = 3/16.
//! Sempre mantida na forma reduzida e com denominador positivo.
//! Tempo musical nunca usa float — isto elimina erro de arredondamento rítmico.

use core::cmp::Ordering;
use core::ops::{Add, Mul, Sub};
use serde::{Deserialize, Serialize};

/// Fração racional `num/den`, sempre reduzida, `den > 0`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Fraction {
    num: i64,
    den: i64,
}

impl Fraction {
    pub const ZERO: Fraction = Fraction { num: 0, den: 1 };
    pub const ONE: Fraction = Fraction { num: 1, den: 1 };

    /// Cria uma fração reduzida. `den == 0` faz pânico (divisão por zero não é musical).
    pub fn new(num: i64, den: i64) -> Fraction {
        assert!(den != 0, "Fraction com denominador zero");
        let mut f = Fraction { num, den };
        f.normalize();
        f
    }

    #[inline]
    pub fn num(&self) -> i64 {
        self.num
    }

    #[inline]
    pub fn den(&self) -> i64 {
        self.den
    }

    #[inline]
    pub fn is_zero(&self) -> bool {
        self.num == 0
    }

    /// Valor aproximado em ponto flutuante — apenas para áudio/render, nunca para lógica rítmica.
    #[inline]
    pub fn to_f64(&self) -> f64 {
        self.num as f64 / self.den as f64
    }

    fn normalize(&mut self) {
        if self.den < 0 {
            self.num = -self.num;
            self.den = -self.den;
        }
        let g = gcd(self.num.unsigned_abs(), self.den.unsigned_abs());
        if g > 1 {
            self.num /= g as i64;
            self.den /= g as i64;
        }
    }
}

/// Máximo divisor comum (Euclides) para normalização.
fn gcd(mut a: u64, mut b: u64) -> u64 {
    if a == 0 {
        return b.max(1);
    }
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

impl PartialEq for Fraction {
    fn eq(&self, other: &Self) -> bool {
        // Ambas já estão reduzidas, então comparação direta basta.
        self.num == other.num && self.den == other.den
    }
}

impl Eq for Fraction {}

impl PartialOrd for Fraction {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Fraction {
    fn cmp(&self, other: &Self) -> Ordering {
        // a/b vs c/d  ->  a*d vs c*b  (den > 0, então o sinal se mantém). i128 evita overflow.
        (self.num as i128 * other.den as i128).cmp(&(other.num as i128 * self.den as i128))
    }
}

impl Add for Fraction {
    type Output = Fraction;
    fn add(self, rhs: Fraction) -> Fraction {
        Fraction::new(
            self.num * rhs.den + rhs.num * self.den,
            self.den * rhs.den,
        )
    }
}

impl Sub for Fraction {
    type Output = Fraction;
    fn sub(self, rhs: Fraction) -> Fraction {
        Fraction::new(
            self.num * rhs.den - rhs.num * self.den,
            self.den * rhs.den,
        )
    }
}

impl Mul for Fraction {
    type Output = Fraction;
    fn mul(self, rhs: Fraction) -> Fraction {
        Fraction::new(self.num * rhs.num, self.den * rhs.den)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduz_na_criacao() {
        let f = Fraction::new(2, 4);
        assert_eq!((f.num(), f.den()), (1, 2));
    }

    #[test]
    fn normaliza_sinal_do_denominador() {
        let f = Fraction::new(1, -2);
        assert_eq!((f.num(), f.den()), (-1, 2));
    }

    #[test]
    fn soma_colcheia_pontuada() {
        // 1/8 + 1/16 = 3/16
        let r = Fraction::new(1, 8) + Fraction::new(1, 16);
        assert_eq!(r, Fraction::new(3, 16));
    }

    #[test]
    fn soma_quatro_seminimas_igual_semibreve() {
        let q = Fraction::new(1, 4);
        let total = q + q + q + q;
        assert_eq!(total, Fraction::ONE);
    }

    #[test]
    fn subtracao_e_multiplicacao() {
        assert_eq!(Fraction::new(1, 2) - Fraction::new(1, 4), Fraction::new(1, 4));
        assert_eq!(Fraction::new(1, 4) * Fraction::new(2, 1), Fraction::new(1, 2));
    }

    #[test]
    fn ordenacao() {
        assert!(Fraction::new(1, 8) < Fraction::new(1, 4));
        assert!(Fraction::new(3, 4) > Fraction::new(1, 2));
        assert_eq!(Fraction::new(2, 4), Fraction::new(1, 2));
    }

    #[test]
    fn zero_e_one() {
        assert!(Fraction::ZERO.is_zero());
        assert_eq!(Fraction::ZERO + Fraction::ONE, Fraction::ONE);
    }
}
