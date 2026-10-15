use std::fmt;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rational {
    num: i64,
    den: i64,
}

fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

fn rational_from_i128(mut num: i128, mut den: i128) -> Result<Rational, RationalError> {
    if den == 0 {
        return Err(RationalError::ZeroDenominator);
    }
    if den < 0 {
        num = -num;
        den = -den;
    }
    if num == 0 {
        return Ok(Rational { num: 0, den: 1 });
    }
    let g = gcd_u128(num.unsigned_abs(), den as u128) as i128;
    num /= g;
    den /= g;
    if num < i64::MIN as i128 || num > i64::MAX as i128 || den < 1 || den > i64::MAX as i128 {
        return Err(RationalError::Overflow);
    }
    Ok(Rational {
        num: num as i64,
        den: den as i64,
    })
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
        let n = self.num as i128 * rhs.den as i128 + rhs.num as i128 * self.den as i128;
        let d = self.den as i128 * rhs.den as i128;
        rational_from_i128(n, d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let n = self.num as i128 * rhs.den as i128 - rhs.num as i128 * self.den as i128;
        let d = self.den as i128 * rhs.den as i128;
        rational_from_i128(n, d)
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
        let bytes = text.as_bytes();
        let fail = |at, kind| RationalError::Parse { at, kind };
        if bytes.is_empty() {
            return Err(fail(0, ParseKind::Empty));
        }

        let mut i = 0;
        if bytes[i] == b'-' {
            i += 1;
            if i == bytes.len() {
                return Err(fail(i, ParseKind::InvalidByte));
            }
        }

        let num_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == num_start {
            return Err(fail(i, ParseKind::InvalidByte));
        }

        let num_end = i;
        let mut den_start = None;
        let mut den_end = None;
        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(fail(i, ParseKind::InvalidByte));
            }
            i += 1;
            den_start = Some(i);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == den_start.unwrap() {
                return Err(fail(i, ParseKind::InvalidByte));
            }
            den_end = Some(i);
            if i < bytes.len() {
                return Err(fail(i, ParseKind::InvalidByte));
            }
        }

        let negative = bytes[0] == b'-';
        let digits_start = if negative { 1 } else { 0 };
        if num_end - digits_start >= 2 && bytes[digits_start] == b'0' {
            return Err(fail(digits_start + 1, ParseKind::NonCanonical));
        }
        if negative && num_end == 2 && bytes[1] == b'0' {
            return Err(fail(0, ParseKind::NonCanonical));
        }
        if let (Some(ds), Some(de)) = (den_start, den_end) {
            if de - ds >= 2 && bytes[ds] == b'0' {
                return Err(fail(ds + 1, ParseKind::NonCanonical));
            }
        }

        let mut num_mag: u128 = 0;
        for &b in &bytes[digits_start..num_end] {
            num_mag = num_mag * 10 + (b - b'0') as u128;
            let limit = if negative { 1u128 << 63 } else { i64::MAX as u128 };
            if num_mag > limit {
                return Err(fail(0, ParseKind::OutOfRange));
            }
        }
        let num: i64 = if negative {
            if num_mag == 1u128 << 63 {
                i64::MIN
            } else {
                -(num_mag as i64)
            }
        } else {
            num_mag as i64
        };

        let mut den = 1i64;
        if let (Some(ds), Some(de)) = (den_start, den_end) {
            let mut mag: u128 = 0;
            for &b in &bytes[ds..de] {
                mag = mag * 10 + (b - b'0') as u128;
                if mag > i64::MAX as u128 {
                    return Err(fail(ds, ParseKind::OutOfRange));
                }
            }
            if mag == 0 {
                return Err(fail(ds, ParseKind::ZeroDenominator));
            }
            den = mag as i64;

            let slash = ds - 1;
            if den == 1 {
                return Err(fail(slash, ParseKind::NonCanonical));
            }
            if num == 0 {
                return Err(fail(slash, ParseKind::NonCanonical));
            }
            if gcd_u128(num.unsigned_abs() as u128, den as u128) > 1 {
                return Err(fail(slash, ParseKind::NonCanonical));
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
pub struct Matrix {
    rows: usize,
    cols: usize,
    data: Vec<Rational>,
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
        let len = self.rows.checked_mul(rhs.cols).ok_or(MatrixError::ShapeOverflow)?;
        let zero = Rational { num: 0, den: 1 };
        let mut data = Vec::with_capacity(len);
        for r in 0..self.rows {
            for c in 0..rhs.cols {
                let mut acc = zero;
                for k in 0..self.cols {
                    let left = self.data[r * self.cols + k];
                    let right = rhs.data[k * rhs.cols + c];
                    let term = left.mul(&right).map_err(|source| MatrixError::Arithmetic { row: r, col: c, step: k, source })?;
                    acc = acc.add(&term).map_err(|source| MatrixError::Arithmetic { row: r, col: c, step: k, source })?;
                }
                data.push(acc);
            }
        }
        Ok(Self { rows: self.rows, cols: rhs.cols, data })
    }

    pub fn rref(&self) -> Result<Reduction, MatrixError> {
        self.eliminate(self.cols, self.cols)
    }

    fn eliminate(&self, pivot_cols: usize, total_cols: usize) -> Result<Reduction, MatrixError> {
        let mut matrix = self.clone();
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let mut p = 0usize;

        for col in 0..pivot_cols {
            if p == matrix.rows {
                break;
            }
            let mut best: Option<(usize, i64, u64, usize)> = None;
            for row in p..matrix.rows {
                let value = matrix.data[row * total_cols + col];
                if value.num == 0 {
                    continue;
                }
                let key = (value.den, value.num.unsigned_abs(), row);
                if best.map_or(true, |(_, den, mag, idx)| key < (den, mag, idx)) {
                    best = Some((row, value.den, value.num.unsigned_abs(), row));
                }
            }
            let Some((q, _, _, _)) = best else {
                continue;
            };
            if q != p {
                for c in 0..total_cols {
                    matrix.data.swap(p * total_cols + c, q * total_cols + c);
                }
                swaps.push((p, q));
            }

            let pivot = matrix.data[p * total_cols + col];
            for c in 0..total_cols {
                let value = matrix.data[p * total_cols + c];
                matrix.data[p * total_cols + c] = value.div(&pivot).map_err(|source| MatrixError::Arithmetic {
                    row: p, col: c, step: p, source
                })?;
            }

            for row in 0..matrix.rows {
                if row == p {
                    continue;
                }
                let factor = matrix.data[row * total_cols + col];
                if factor.num == 0 {
                    continue;
                }
                for c in 0..total_cols {
                    let pivot_value = matrix.data[p * total_cols + c];
                    let product = factor.mul(&pivot_value).map_err(|source| MatrixError::Arithmetic {
                        row, col: c, step: p, source
                    })?;
                    let current = matrix.data[row * total_cols + c];
                    matrix.data[row * total_cols + c] = current.sub(&product).map_err(|source| MatrixError::Arithmetic {
                        row, col: c, step: p, source
                    })?;
                }
            }

            pivots.push(col);
            p += 1;
        }

        Ok(Reduction { matrix, pivots, swaps })
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
        let mut data = Vec::with_capacity(total);
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
        let augmented = Matrix { rows: n, cols: width, data };
        let red = augmented.eliminate(n, width)?;
        if red.pivots.len() < n {
            return Err(MatrixError::Singular { rank: red.pivots.len() });
        }
        let mut result = Vec::with_capacity(n * n);
        for r in 0..n {
            for c in n..width {
                result.push(red.matrix.data[r * width + c]);
            }
        }
        Ok(Matrix { rows: n, cols: n, data: result })
    }
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

fn main() {}