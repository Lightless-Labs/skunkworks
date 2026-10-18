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

impl Rational {
    fn from_i128(num: i128, den: i128) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        let (mut n, mut d) = (num, den);
        if d < 0 {
            n = n.checked_neg().ok_or(RationalError::Overflow)?;
            d = -d;
        }
        if n == 0 {
            return Ok(Self { num: 0, den: 1 });
        }
        let g = gcd_u128(n.unsigned_abs(), d as u128) as i128;
        n /= g;
        d /= g;
        if n < i64::MIN as i128 || n > i64::MAX as i128 || d < 1 || d > i64::MAX as i128 {
            return Err(RationalError::Overflow);
        }
        Ok(Self { num: n as i64, den: d as i64 })
    }

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
        Self::from_i128(n, d)
    }

    pub fn numerator(&self) -> i64 { self.num }
    pub fn denominator(&self) -> i64 { self.den }

    pub fn add(&self, rhs: &Self) -> Result<Self, RationalError> {
        let n = self.num as i128 * rhs.den as i128 + rhs.num as i128 * self.den as i128;
        let d = self.den as i128 * rhs.den as i128;
        Self::from_i128(n, d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let n = self.num as i128 * rhs.den as i128 - rhs.num as i128 * self.den as i128;
        let d = self.den as i128 * rhs.den as i128;
        Self::from_i128(n, d)
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::from_i128(self.num as i128 * rhs.num as i128, self.den as i128 * rhs.den as i128)
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 { return Err(RationalError::DivideByZero); }
        Self::from_i128(self.num as i128 * rhs.den as i128, self.den as i128 * rhs.num as i128)
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        let n = -(self.num as i128);
        Self::from_i128(n, self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let b = text.as_bytes();
        let err = |at, kind| RationalError::Parse { at, kind };
        if b.is_empty() { return Err(err(0, ParseKind::Empty)); }

        let slash = b.iter().position(|&x| x == b'/');
        let n_end = slash.unwrap_or(b.len());
        let mut i = 0;
        if b[0] == b'-' { i = 1; }
        if i == n_end {
            return Err(err(n_end, ParseKind::InvalidByte));
        }
        while i < n_end {
            if !b[i].is_ascii_digit() { return Err(err(i, ParseKind::InvalidByte)); }
            i += 1;
        }
        if let Some(s) = slash {
            if s + 1 == b.len() { return Err(err(b.len(), ParseKind::InvalidByte)); }
            for j in s + 1..b.len() {
                if !b[j].is_ascii_digit() { return Err(err(j, ParseKind::InvalidByte)); }
            }
        }

        let neg = b[0] == b'-';
        let digit_start = usize::from(neg);
        if n_end - digit_start >= 2 && b[digit_start] == b'0' {
            return Err(err(digit_start + 1, ParseKind::NonCanonical));
        }
        if neg && n_end == 2 && b[1] == b'0' {
            return Err(err(0, ParseKind::NonCanonical));
        }

        let den_start = slash.map(|s| s + 1);
        if let (Some(s), Some(ds)) = (slash, den_start) {
            if b.len() - ds >= 2 && b[ds] == b'0' {
                return Err(err(ds + 1, ParseKind::NonCanonical));
            }
        }

        let parse_mag = |start: usize, end: usize, limit: u128| -> Option<u128> {
            let mut v = 0u128;
            for &c in &b[start..end] {
                v = v.checked_mul(10)?.checked_add((c - b'0') as u128)?;
                if v > limit { return None; }
            }
            Some(v)
        };

        let nmag = parse_mag(digit_start, n_end, if neg { 1u128 << 63 } else { i64::MAX as u128 })
            .ok_or_else(|| err(0, ParseKind::OutOfRange))?;
        let n = if neg {
            if nmag == 1u128 << 63 { i64::MIN } else { -(nmag as i64) }
        } else { nmag as i64 };

        if let (Some(s), Some(ds)) = (slash, den_start) {
            let dmag = parse_mag(ds, b.len(), i64::MAX as u128)
                .ok_or_else(|| err(ds, ParseKind::OutOfRange))?;
            if dmag == 0 { return Err(err(ds, ParseKind::ZeroDenominator)); }
            if dmag == 1 { return Err(err(s, ParseKind::NonCanonical)); }
            if n == 0 { return Err(err(s, ParseKind::NonCanonical)); }
            if gcd_u128(n.unsigned_abs() as u128, dmag) > 1 {
                return Err(err(s, ParseKind::NonCanonical));
            }
            Ok(Self { num: n, den: dmag as i64 })
        } else {
            Ok(Self { num: n, den: 1 })
        }
    }
}

impl fmt::Display for Rational {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 { write!(f, "{}", self.num) }
        else { write!(f, "{}/{}", self.num, self.den) }
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

    fn offset(&self, row: usize, col: usize) -> Option<usize> {
        if row >= self.rows || col >= self.cols { return None; }
        row.checked_mul(self.cols)?.checked_add(col)
    }

    pub fn get(&self, row: usize, col: usize) -> Result<Rational, MatrixError> {
        self.offset(row, col).and_then(|i| self.data.get(i).copied())
            .ok_or(MatrixError::Index { row, col })
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

    fn eliminate(mut matrix: Self, pivot_cols: usize) -> Result<Reduction, MatrixError> {
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let mut p = 0usize;

        for col in 0..pivot_cols {
            if p == matrix.rows { break; }
            let mut chosen: Option<usize> = None;
            for row in p..matrix.rows {
                let v = matrix.data[row * matrix.cols + col];
                if v.num == 0 { continue; }
                let key = (v.den, v.num.unsigned_abs(), row);
                if chosen.map_or(true, |q| {
                    let w = matrix.data[q * matrix.cols + col];
                    key < (w.den, w.num.unsigned_abs(), q)
                }) {
                    chosen = Some(row);
                }
            }
            let Some(q) = chosen else { continue; };
            if q != p {
                for c in 0..matrix.cols {
                    matrix.data.swap(p * matrix.cols + c, q * matrix.cols + c);
                }
                swaps.push((p, q));
            }

            let pivot = matrix.data[p * matrix.cols + col];
            for c in 0..matrix.cols {
                let idx = p * matrix.cols + c;
                let value = matrix.data[idx].div(&pivot)
                    .map_err(|source| MatrixError::Arithmetic { row: p, col: c, step: p, source })?;
                matrix.data[idx] = value;
            }

            for row in 0..matrix.rows {
                if row == p { continue; }
                let factor = matrix.data[row * matrix.cols + col];
                if factor.num == 0 { continue; }
                for c in 0..matrix.cols {
                    let pivot_entry = matrix.data[p * matrix.cols + c];
                    let product = factor.mul(&pivot_entry)
                        .map_err(|source| MatrixError::Arithmetic { row, col: c, step: p, source })?;
                    let idx = row * matrix.cols + c;
                    let value = matrix.data[idx].sub(&product)
                        .map_err(|source| MatrixError::Arithmetic { row, col: c, step: p, source })?;
                    matrix.data[idx] = value;
                }
            }
            pivots.push(col);
            p += 1;
        }
        Ok(Reduction { matrix, pivots, swaps })
    }

    pub fn rref(&self) -> Result<Reduction, MatrixError> {
        Self::eliminate(self.clone(), self.cols)
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
        for r in 0..n {
            for c in 0..n {
                data.push(self.data[r * n + c]);
            }
            for c in 0..n {
                data.push(if r == c { Rational { num: 1, den: 1 } } else { Rational { num: 0, den: 1 } });
            }
        }
        let augmented = Self { rows: n, cols: width, data };
        let red = Self::eliminate(augmented, n)?;
        if red.pivots.len() < n {
            return Err(MatrixError::Singular { rank: red.pivots.len() });
        }
        let mut result = Vec::with_capacity(n * n);
        for r in 0..n {
            result.extend_from_slice(&red.matrix.data[r * width + n..r * width + width]);
        }
        Ok(Self { rows: n, cols: n, data: result })
    }
}