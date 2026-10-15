// Fault: output shape overflow takes precedence over mismatch
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseKind { Empty, InvalidByte, NonCanonical, OutOfRange, ZeroDenominator }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RationalError {
    ZeroDenominator,
    DivideByZero,
    Overflow,
    Parse { at: usize, kind: ParseKind },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rational { num: i64, den: i64 }

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 { let r = a % b; a = b; b = r; }
    a
}
impl Rational {
    fn reduced(mut n: i128, mut d: i128) -> Result<Self, RationalError> {
        if d == 0 { return Err(RationalError::ZeroDenominator); }
        if n == 0 { return Ok(Self { num: 0, den: 1 }); }
        if d < 0 { n = -n; d = -d; }
        let g = gcd(n.unsigned_abs(), d as u128) as i128;
        n /= g;
        d /= g;
        if n < i64::MIN as i128 || n > i64::MAX as i128 || d > i64::MAX as i128 {
            return Err(RationalError::Overflow);
        }
        Ok(Self { num: n as i64, den: d as i64 })
    }
    pub fn new(num: i64, den: i64) -> Result<Self, RationalError> {
        Self::reduced(num as i128, den as i128)
    }
    pub fn numerator(&self) -> i64 { self.num }
    pub fn denominator(&self) -> i64 { self.den }
    pub fn add(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::reduced(self.num as i128 * rhs.den as i128 + rhs.num as i128 * self.den as i128,
                      self.den as i128 * rhs.den as i128)
    }
    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::reduced(self.num as i128 * rhs.den as i128 - rhs.num as i128 * self.den as i128,
                      self.den as i128 * rhs.den as i128)
    }
    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::reduced(self.num as i128 * rhs.num as i128, self.den as i128 * rhs.den as i128)
    }
    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 { return Err(RationalError::DivideByZero); }
        Self::reduced(self.num as i128 * rhs.den as i128, self.den as i128 * rhs.num as i128)
    }
    pub fn neg(&self) -> Result<Self, RationalError> {
        Self::reduced(-(self.num as i128), self.den as i128)
    }
    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let err = |at, kind| RationalError::Parse { at, kind };
        let b = text.as_bytes();
        if b.is_empty() { return Err(err(0, ParseKind::Empty)); }
        let neg = b[0] == b'-';
        let start = usize::from(neg);
        let mut i = start;
        if i == b.len() || !b[i].is_ascii_digit() { return Err(err(i, ParseKind::InvalidByte)); }
        while i < b.len() && b[i].is_ascii_digit() { i += 1; }
        let nend = i;
        let slash = if i == b.len() { None } else {
            if b[i] != b'/' { return Err(err(i, ParseKind::InvalidByte)); }
            let s = i;
            i += 1;
            if i == b.len() || !b[i].is_ascii_digit() { return Err(err(i, ParseKind::InvalidByte)); }
            while i < b.len() && b[i].is_ascii_digit() { i += 1; }
            if i != b.len() { return Err(err(i, ParseKind::InvalidByte)); }
            Some(s)
        };
        // Syntax is checked in full before any canonicality or magnitude check.
        if nend - start > 1 && b[start] == b'0' { return Err(err(start + 1, ParseKind::NonCanonical)); }
        if neg && nend - start == 1 && b[start] == b'0' { return Err(err(0, ParseKind::NonCanonical)); }
        if let Some(s) = slash {
            if b.len() - s - 1 > 1 && b[s + 1] == b'0' { return Err(err(s + 2, ParseKind::NonCanonical)); }
        }
        let magnitude = |digits: &[u8], limit: u128| -> Option<u128> {
            let mut v = 0u128;
            for &c in digits {
                v = v.checked_mul(10)?.checked_add((c - b'0') as u128)?;
                if v > limit { return None; }
            }
            Some(v)
        };
        let lim = if neg { 1u128 << 63 } else { i64::MAX as u128 };
        let mag = magnitude(&b[start..nend], lim).ok_or(err(0, ParseKind::OutOfRange))?;
        let n = if neg { -(mag as i128) } else { mag as i128 };
        let d = if let Some(s) = slash {
            magnitude(&b[s + 1..], i64::MAX as u128).ok_or(err(s + 1, ParseKind::OutOfRange))?
        } else { 1 };
        if let Some(s) = slash {
            if d == 0 { return Err(err(s + 1, ParseKind::ZeroDenominator)); }
            if d == 1 { return Err(err(s, ParseKind::NonCanonical)); }
            if n == 0 { return Err(err(s, ParseKind::NonCanonical)); }
            if gcd(n.unsigned_abs(), d) != 1 { return Err(err(s, ParseKind::NonCanonical)); }
        }
        Ok(Self { num: n as i64, den: d as i64 })
    }
}
impl fmt::Display for Rational {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 { write!(f, "{}", self.num) } else { write!(f, "{}/{}", self.num, self.den) }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatrixError {
    ShapeOverflow,
    DataLength { expected: usize, actual: usize },
    Index { row: usize, col: usize },
    DimensionMismatch { left: (usize, usize), right: (usize, usize) },
    Arithmetic { row: usize, col: usize, step: usize, source: RationalError },
    NotSquare { rows: usize, cols: usize },
    Singular { rank: usize },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Matrix { rows: usize, cols: usize, data: Vec<Rational> }
impl Matrix {
    pub fn new(rows: usize, cols: usize, data: Vec<Rational>) -> Result<Self, MatrixError> {
        let expected = rows.checked_mul(cols).ok_or(MatrixError::ShapeOverflow)?;
        if data.len() != expected { return Err(MatrixError::DataLength { expected, actual: data.len() }); }
        Ok(Self { rows, cols, data })
    }
    pub fn rows(&self) -> usize { self.rows }
    pub fn cols(&self) -> usize { self.cols }
    pub fn data(&self) -> &[Rational] { &self.data }
    pub fn get(&self, row: usize, col: usize) -> Result<Rational, MatrixError> {
        if row >= self.rows || col >= self.cols { return Err(MatrixError::Index { row, col }); }
        Ok(self.data[row * self.cols + col])
    }
    pub fn transpose(&self) -> Self {
        let mut data = vec![Rational { num: 0, den: 1 }; self.data.len()];
        for (i, &v) in self.data.iter().enumerate() {
            data[(i % self.cols) * self.rows + i / self.cols] = v;
        }
        Self { rows: self.cols, cols: self.rows, data }
    }
    pub fn mul(&self, rhs: &Self) -> Result<Self, MatrixError> {
        self.rows.checked_mul(rhs.cols).ok_or(MatrixError::ShapeOverflow)?;
        if self.cols != rhs.rows {
            return Err(MatrixError::DimensionMismatch { left: (self.rows, self.cols), right: (rhs.rows, rhs.cols) });
        }
        let len = self.rows.checked_mul(rhs.cols).ok_or(MatrixError::ShapeOverflow)?;
        let mut out = vec![Rational { num: 0, den: 1 }; len];
        for (index, slot) in out.iter_mut().enumerate() {
            let r = index / rhs.cols;
            let c = index % rhs.cols;
            for k in 0..self.cols {
                let map = |source| MatrixError::Arithmetic { row: r, col: c, step: k, source };
                let term = self.data[r * self.cols + k].mul(&rhs.data[k * rhs.cols + c]).map_err(map)?;
                *slot = slot.add(&term).map_err(map)?;
            }
        }
        Self::new(self.rows, rhs.cols, out)
    }
}
