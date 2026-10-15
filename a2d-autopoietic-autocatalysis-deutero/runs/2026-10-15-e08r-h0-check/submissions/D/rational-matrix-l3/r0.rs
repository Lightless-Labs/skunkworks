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
    if num == 0 {
        return Ok(Rational { num: 0, den: 1 });
    }
    let mut n = num;
    let mut d = den;
    if d < 0 {
        n = -n;
        d = -d;
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
        rational_from_i128(num as i128, den as i128)
    }

    pub fn numerator(&self) -> i64 {
        self.num
    }

    pub fn denominator(&self) -> i64 {
        self.den
    }

    pub fn add(&self, rhs: &Self) -> Result<Self, RationalError> {
        let a = self.num as i128 * rhs.den as i128;
        let b = rhs.num as i128 * self.den as i128;
        rational_from_i128(a + b, self.den as i128 * rhs.den as i128)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let a = self.num as i128 * rhs.den as i128;
        let b = rhs.num as i128 * self.den as i128;
        rational_from_i128(a - b, self.den as i128 * rhs.den as i128)
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_from_i128(
            self.num as i128 * rhs.num as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        rational_from_i128(
            self.num as i128 * rhs.den as i128,
            self.den as i128 * rhs.num as i128,
        )
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        rational_from_i128(-(self.num as i128), self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(RationalError::Parse { at: 0, kind: ParseKind::Empty });
        }

        let mut i = 0;
        if bytes[0] == b'-' {
            i = 1;
        }

        let num_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == num_start {
            return Err(RationalError::Parse { at: i, kind: ParseKind::InvalidByte });
        }

        let mut den_start = None;
        let mut slash_at = None;
        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(RationalError::Parse { at: i, kind: ParseKind::InvalidByte });
            }
            slash_at = Some(i);
            i += 1;
            den_start = Some(i);
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return Err(RationalError::Parse { at: i, kind: ParseKind::InvalidByte });
            }
        }
        if i < bytes.len() {
            return Err(RationalError::Parse { at: i, kind: ParseKind::InvalidByte });
        }

        let fail = |at, kind| RationalError::Parse { at, kind };

        if num_start < bytes.len() && bytes[num_start] == b'0' && (num_start + 1 < (slash_at.unwrap_or(bytes.len()))) {
            return Err(fail(num_start + 1, ParseKind::NonCanonical));
        }
        if bytes[0] == b'-' && &text[..slash_at.unwrap_or(bytes.len())] == "-0" {
            return Err(fail(0, ParseKind::NonCanonical));
        }

        if let Some(ds) = den_start {
            let end = bytes.len();
            if end - ds > 1 && bytes[ds] == b'0' {
                return Err(fail(ds + 1, ParseKind::NonCanonical));
            }
        }

        let num_end = slash_at.unwrap_or(bytes.len());
        let num_digits = &text[num_start..num_end];
        let mut num_mag: u128 = 0;
        for b in num_digits.bytes() {
            num_mag = num_mag * 10 + (b - b'0') as u128;
            let limit = if bytes[0] == b'-' { 1u128 << 63 } else { i64::MAX as u128 };
            if num_mag > limit {
                return Err(fail(0, ParseKind::OutOfRange));
            }
        }
        let num = if bytes[0] == b'-' {
            if num_mag == 1u128 << 63 { i64::MIN } else { -(num_mag as i64) }
        } else {
            num_mag as i64
        };

        let mut den = 1i64;
        if let Some(ds) = den_start {
            let mut mag: u128 = 0;
            for b in bytes[ds..].iter().copied() {
                mag = mag * 10 + (b - b'0') as u128;
                if mag > i64::MAX as u128 {
                    return Err(fail(ds, ParseKind::OutOfRange));
                }
            }
            if mag == 0 {
                return Err(fail(ds, ParseKind::ZeroDenominator));
            }
            den = mag as i64;
            if den == 1 {
                return Err(fail(slash_at.unwrap(), ParseKind::NonCanonical));
            }
            if num == 0 {
                return Err(fail(slash_at.unwrap(), ParseKind::NonCanonical));
            }
            if gcd_u128(num.unsigned_abs() as u128, den as u128) > 1 {
                return Err(fail(slash_at.unwrap(), ParseKind::NonCanonical));
            }
        }
        Ok(Rational { num, den })
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
    pub fn rank(&self) -> usize {
        self.pivots.len()
    }
}

impl Matrix {
    pub fn new(rows: usize, cols: usize, data: Vec<Rational>) -> Result<Self, MatrixError> {
        let expected = rows.checked_mul(cols).ok_or(MatrixError::ShapeOverflow)?;
        if data.len() != expected {
            return Err(MatrixError::DataLength { expected, actual: data.len() });
        }
        Ok(Self { rows, cols, data })
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn data(&self) -> &[Rational] {
        &self.data
    }

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
                left: (self.rows, self.cols),
                right: (rhs.rows, rhs.cols),
            });
        }
        let count = self.rows.checked_mul(rhs.cols).ok_or(MatrixError::ShapeOverflow)?;
        let mut data = Vec::with_capacity(count);
        for r in 0..self.rows {
            for c in 0..rhs.cols {
                let mut acc = Rational { num: 0, den: 1 };
                for k in 0..self.cols {
                    let a = self.data[r * self.cols + k];
                    let b = rhs.data[k * rhs.cols + c];
                    let term = a.mul(&b).map_err(|source| MatrixError::Arithmetic { row: r, col: c, step: k, source })?;
                    acc = acc.add(&term).map_err(|source| MatrixError::Arithmetic { row: r, col: c, step: k, source })?;
                }
                data.push(acc);
            }
        }
        Ok(Self { rows: self.rows, cols: rhs.cols, data })
    }

    fn eliminate(&self, pivot_cols: usize) -> Result<Reduction, MatrixError> {
        let mut work = self.clone();
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let mut p = 0usize;

        for col in 0..pivot_cols {
            if p == work.rows {
                break;
            }
            let mut best: Option<usize> = None;
            for row in p..work.rows {
                let v = work.data[row * work.cols + col];
                if v.num == 0 {
                    continue;
                }
                let key = (v.den, v.num.unsigned_abs(), row);
                if best.map_or(true, |br| {
                    let bv = work.data[br * work.cols + col];
                    key < (bv.den, bv.num.unsigned_abs(), br)
                }) {
                    best = Some(row);
                }
            }
            let Some(q) = best else { continue };

            if q != p {
                for c in 0..work.cols {
                    work.data.swap(p * work.cols + c, q * work.cols + c);
                }
                swaps.push((p, q));
            }

            let pivot = work.data[p * work.cols + col];
            for c in 0..work.cols {
                let current = work.data[p * work.cols + c];
                work.data[p * work.cols + c] = current.div(&pivot).map_err(|source| {
                    MatrixError::Arithmetic { row: p, col: c, step: p, source }
                })?;
            }

            for row in 0..work.rows {
                if row == p {
                    continue;
                }
                let factor = work.data[row * work.cols + col];
                if factor.num == 0 {
                    continue;
                }
                for c in 0..work.cols {
                    let product = factor.mul(&work.data[p * work.cols + c]).map_err(|source| {
                        MatrixError::Arithmetic { row, col: c, step: p, source }
                    })?;
                    work.data[row * work.cols + c] = work.data[row * work.cols + c].sub(&product).map_err(|source| {
                        MatrixError::Arithmetic { row, col: c, step: p, source }
                    })?;
                }
            }
            pivots.push(col);
            p += 1;
        }
        Ok(Reduction { matrix: work, pivots, swaps })
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
        let total = n.checked_mul(width).ok_or(MatrixError::ShapeOverflow)?;
        let mut data = vec![Rational { num: 0, den: 1 }; total];
        for r in 0..n {
            for c in 0..n {
                data[r * width + c] = self.data[r * n + c];
            }
            data[r * width + n + r] = Rational { num: 1, den: 1 };
        }
        let augmented = Matrix { rows: n, cols: width, data };
        let reduction = augmented.eliminate(n)?;
        let rank = reduction.pivots.len();
        if rank < n {
            return Err(MatrixError::Singular { rank });
        }
        let mut result = Vec::with_capacity(n * n);
        for r in 0..n {
            for c in 0..n {
                result.push(reduction.matrix.data[r * width + n + c]);
            }
        }
        Ok(Matrix { rows: n, cols: n, data: result })
    }
}