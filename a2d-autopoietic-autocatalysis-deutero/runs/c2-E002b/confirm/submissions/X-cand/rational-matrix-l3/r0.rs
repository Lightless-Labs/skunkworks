use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rational {
    num: i64,
    den: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseKind {
    Empty,
    InvalidByte,
    NonCanonical,
    OutOfRange,
    ZeroDenominator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RationalError {
    ZeroDenominator,
    DivideByZero,
    Overflow,
    Parse { at: usize, kind: ParseKind },
}

fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

fn rational_from_i128(num: i128, den: i128) -> Result<Rational, RationalError> {
    if den == 0 {
        return Err(RationalError::ZeroDenominator);
    }
    let mut n = num;
    let mut d = den;
    if d < 0 {
        n = -n;
        d = -d;
    }
    if n == 0 {
        return Ok(Rational { num: 0, den: 1 });
    }
    let g = gcd_u128(n.unsigned_abs(), d as u128) as i128;
    n /= g;
    d /= g;
    if n < i64::MIN as i128 || n > i64::MAX as i128 || d < 1 || d > i64::MAX as i128 {
        return Err(RationalError::Overflow);
    }
    Ok(Rational { num: n as i64, den: d as i64 })
}

impl Rational {
    pub fn new(num: i64, den: i64) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        let mut n = num as i128;
        let mut d = den as i128;
        if d < 0 {
            n = -n;
            d = -d;
        }
        if n == 0 {
            return Ok(Self { num: 0, den: 1 });
        }
        let g = gcd_u128(n.unsigned_abs(), d as u128) as i128;
        n /= g;
        d /= g;
        if n < i64::MIN as i128 || n > i64::MAX as i128 || d > i64::MAX as i128 {
            return Err(RationalError::Overflow);
        }
        Ok(Self { num: n as i64, den: d as i64 })
    }

    pub fn numerator(&self) -> i64 { self.num }
    pub fn denominator(&self) -> i64 { self.den }

    pub fn add(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_from_i128(
            self.num as i128 * rhs.den as i128 + rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_from_i128(
            self.num as i128 * rhs.den as i128 - rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_from_i128(self.num as i128 * rhs.num as i128, self.den as i128 * rhs.den as i128)
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        rational_from_i128(self.num as i128 * rhs.den as i128, self.den as i128 * rhs.num as i128)
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        rational_from_i128(-(self.num as i128), self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let b = text.as_bytes();
        let fail = |at, kind| RationalError::Parse { at, kind };
        if b.is_empty() {
            return Err(fail(0, ParseKind::Empty));
        }

        let mut i = 0;
        if b[i] == b'-' {
            i += 1;
        }
        let num_start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == num_start {
            return Err(fail(i, ParseKind::InvalidByte));
        }
        let num_end = i;
        let mut den_start = None;
        let mut den_end = None;
        if i < b.len() {
            if b[i] != b'/' {
                return Err(fail(i, ParseKind::InvalidByte));
            }
            i += 1;
            let start = i;
            den_start = Some(start);
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return Err(fail(i, ParseKind::InvalidByte));
            }
            den_end = Some(i);
            if i < b.len() {
                return Err(fail(i, ParseKind::InvalidByte));
            }
        }

        let neg = b[0] == b'-';
        let num_digits = num_end - num_start;
        if num_digits >= 2 && b[num_start] == b'0' {
            return Err(fail(num_start + 1, ParseKind::NonCanonical));
        }
        if neg && num_digits == 1 && b[num_start] == b'0' {
            return Err(fail(0, ParseKind::NonCanonical));
        }
        if let (Some(s), Some(e)) = (den_start, den_end) {
            if e - s >= 2 && b[s] == b'0' {
                return Err(fail(s + 1, ParseKind::NonCanonical));
            }
        }

        let mut nmag: u128 = 0;
        for &ch in &b[num_start..num_end] {
            nmag = nmag.saturating_mul(10).saturating_add((ch - b'0') as u128);
        }
        let nlimit = if neg { 1u128 << 63 } else { i64::MAX as u128 };
        if nmag > nlimit {
            return Err(fail(0, ParseKind::OutOfRange));
        }
        let mut dmag = 1u128;
        if let (Some(s), Some(e)) = (den_start, den_end) {
            dmag = 0;
            for &ch in &b[s..e] {
                dmag = dmag.saturating_mul(10).saturating_add((ch - b'0') as u128);
            }
            if dmag > i64::MAX as u128 {
                return Err(fail(s, ParseKind::OutOfRange));
            }
            if dmag == 0 {
                return Err(fail(s, ParseKind::ZeroDenominator));
            }
            if dmag == 1 {
                return Err(fail(s - 1, ParseKind::NonCanonical));
            }
            if nmag == 0 {
                return Err(fail(s - 1, ParseKind::NonCanonical));
            }
            if gcd_u128(nmag, dmag) > 1 {
                return Err(fail(s - 1, ParseKind::NonCanonical));
            }
        }
        let n = if neg {
            if nmag == (1u128 << 63) { i64::MIN } else { -(nmag as i64) }
        } else {
            nmag as i64
        };
        Ok(Self { num: n, den: dmag as i64 })
    }
}

impl fmt::Display for Rational {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 {
            write!(f, "{}", self.num)
        } else {
            write!(f, "{}/{}", self.num, self.den)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Matrix {
    rows: usize,
    cols: usize,
    data: Vec<Rational>,
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
pub struct Reduction {
    pub matrix: Matrix,
    pub pivots: Vec<usize>,
    pub swaps: Vec<(usize, usize)>,
}

impl Reduction {
    pub fn rank(&self) -> usize { self.pivots.len() }
}

impl Matrix {
    pub fn new(rows: usize, cols: usize, data: Vec<Rational>) -> Result<Self, MatrixError> {
        let expected = rows.checked_mul(cols).ok_or(MatrixError::ShapeOverflow)?;
        if data.len() != expected {
            return Err(MatrixError::DataLength { expected, actual: data.len() });
        }
        Ok(Self { rows, cols, data })
    }

    pub fn rows(&self) -> usize { self.rows }
    pub fn cols(&self) -> usize { self.cols }
    pub fn data(&self) -> &[Rational] { &self.data }

    pub fn get(&self, row: usize, col: usize) -> Result<Rational, MatrixError> {
        if row >= self.rows || col >= self.cols {
            return Err(MatrixError::Index { row, col });
        }
        Ok(self.data[row * self.cols + col])
    }

    pub fn transpose(&self) -> Self {
        let mut data = Vec::with_capacity(self.data.len());
        for c in 0..self.cols {
            for r in 0..self.rows {
                data.push(self.data[r * self.cols + c]);
            }
        }
        Self { rows: self.cols, cols: self.rows, data }
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, MatrixError> {
        if self.cols != rhs.rows {
            return Err(MatrixError::DimensionMismatch {
                left: (self.rows, self.cols), right: (rhs.rows, rhs.cols),
            });
        }
        let len = self.rows.checked_mul(rhs.cols).ok_or(MatrixError::ShapeOverflow)?;
        let mut data = Vec::with_capacity(len);
        let zero = Rational { num: 0, den: 1 };
        for r in 0..self.rows {
            for c in 0..rhs.cols {
                let mut acc = zero;
                for k in 0..self.cols {
                    let lhs = self.data[r * self.cols + k];
                    let rhsv = rhs.data[k * rhs.cols + c];
                    let term = lhs.mul(&rhsv).map_err(|source| MatrixError::Arithmetic { row: r, col: c, step: k, source })?;
                    acc = acc.add(&term).map_err(|source| MatrixError::Arithmetic { row: r, col: c, step: k, source })?;
                }
                data.push(acc);
            }
        }
        Ok(Self { rows: self.rows, cols: rhs.cols, data })
    }

    fn eliminate(&self, pivot_cols: usize) -> Result<Reduction, MatrixError> {
        let mut m = self.clone();
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let mut p = 0usize;

        for col in 0..pivot_cols {
            if p == m.rows { break; }
            let mut best: Option<usize> = None;
            for r in p..m.rows {
                let v = m.data[r * m.cols + col];
                if v.num == 0 { continue; }
                let key = (v.den, v.num.unsigned_abs(), r);
                if best.map_or(true, |br| {
                    let bv = m.data[br * m.cols + col];
                    key < (bv.den, bv.num.unsigned_abs(), br)
                }) {
                    best = Some(r);
                }
            }
            let q = match best { Some(q) => q, None => continue };
            if q != p {
                for c in 0..m.cols {
                    m.data.swap(p * m.cols + c, q * m.cols + c);
                }
                swaps.push((p, q));
            }
            let pivot = m.data[p * m.cols + col];
            for c in 0..m.cols {
                let idx = p * m.cols + c;
                m.data[idx] = m.data[idx].div(&pivot).map_err(|source| MatrixError::Arithmetic { row: p, col: c, step: p, source })?;
            }
            for r in 0..m.rows {
                if r == p { continue; }
                let factor = m.data[r * m.cols + col];
                if factor.num == 0 { continue; }
                for c in 0..m.cols {
                    let product = factor.mul(&m.data[p * m.cols + c])
                        .map_err(|source| MatrixError::Arithmetic { row: r, col: c, step: p, source })?;
                    let idx = r * m.cols + c;
                    m.data[idx] = m.data[idx].sub(&product)
                        .map_err(|source| MatrixError::Arithmetic { row: r, col: c, step: p, source })?;
                }
            }
            pivots.push(col);
            p += 1;
        }
        Ok(Reduction { matrix: m, pivots, swaps })
    }

    pub fn rref(&self) -> Result<Reduction, MatrixError> {
        self.eliminate(self.cols)
    }

    pub fn rank(&self) -> Result<usize, MatrixError> {
        Ok(self.rref()?.rank())
    }

    pub fn inverse(&self) -> Result<Self, MatrixError> {
        if self.rows != self.cols {
            return Err(MatrixError::NotSquare { rows: self.rows, cols: self.cols });
        }
        let n = self.rows;
        let width = n.checked_mul(2).ok_or(MatrixError::ShapeOverflow)?;
        let len = n.checked_mul(width).ok_or(MatrixError::ShapeOverflow)?;
        let mut data = Vec::with_capacity(len);
        let zero = Rational { num: 0, den: 1 };
        let one = Rational { num: 1, den: 1 };
        for r in 0..n {
            for c in 0..n {
                data.push(self.data[r * n + c]);
            }
            for c in 0..n {
                data.push(if r == c { one } else { zero });
            }
        }
        let aug = Self { rows: n, cols: width, data };
        let red = aug.eliminate(n)?;
        let rank = red.pivots.len();
        if rank < n {
            return Err(MatrixError::Singular { rank });
        }
        let mut out = Vec::with_capacity(n.checked_mul(n).ok_or(MatrixError::ShapeOverflow)?);
        for r in 0..n {
            for c in 0..n {
                out.push(red.matrix.data[r * width + n + c]);
            }
        }
        Ok(Self { rows: n, cols: n, data: out })
    }
}