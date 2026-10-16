use std::cmp::Ordering;
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
    fn from_parts(num: i128, den: i128) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        if num == 0 {
            return Ok(Self { num: 0, den: 1 });
        }

        let (mut n, mut d) = (num, den);
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
        Ok(Self {
            num: n as i64,
            den: d as i64,
        })
    }

    pub fn new(num: i64, den: i64) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        Self::from_parts(num as i128, den as i128)
    }

    pub fn numerator(&self) -> i64 {
        self.num
    }

    pub fn denominator(&self) -> i64 {
        self.den
    }

    pub fn add(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::from_parts(
            self.num as i128 * rhs.den as i128 + rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::from_parts(
            self.num as i128 * rhs.den as i128 - rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::from_parts(
            self.num as i128 * rhs.num as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        Self::from_parts(
            self.num as i128 * rhs.den as i128,
            self.den as i128 * rhs.num as i128,
        )
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        Self::from_parts(-(self.num as i128), self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        let fail = |at, kind| Err(RationalError::Parse { at, kind });

        if bytes.is_empty() {
            return fail(0, ParseKind::Empty);
        }

        let mut i = 0;
        let negative = bytes[0] == b'-';
        if negative {
            i = 1;
        }

        let num_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == num_start {
            if i == bytes.len() {
                return fail(i, ParseKind::InvalidByte);
            }
            return fail(i, ParseKind::InvalidByte);
        }
        let num_end = i;

        let mut den_start = None;
        let mut den_end = None;
        if i < bytes.len() {
            if bytes[i] != b'/' {
                return fail(i, ParseKind::InvalidByte);
            }
            i += 1;
            let start = i;
            den_start = Some(start);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return fail(i, ParseKind::InvalidByte);
            }
            den_end = Some(i);
            if i < bytes.len() {
                return fail(i, ParseKind::InvalidByte);
            }
        }

        let num_digits = &bytes[num_start..num_end];
        if num_digits.len() >= 2 && num_digits[0] == b'0' {
            return fail(num_start + 1, ParseKind::NonCanonical);
        }
        if negative && num_digits == b"0" {
            return fail(0, ParseKind::NonCanonical);
        }

        if let (Some(ds), Some(de)) = (den_start, den_end) {
            let den_digits = &bytes[ds..de];
            if den_digits.len() >= 2 && den_digits[0] == b'0' {
                return fail(ds + 1, ParseKind::NonCanonical);
            }
        }

        let mut num_mag: u128 = 0;
        for &b in num_digits {
            num_mag = num_mag * 10 + (b - b'0') as u128;
            let max = if negative { 1u128 << 63 } else { i64::MAX as u128 };
            if num_mag > max {
                return fail(0, ParseKind::OutOfRange);
            }
        }

        let mut den_mag: u128 = 1;
        if let (Some(ds), Some(de)) = (den_start, den_end) {
            den_mag = 0;
            for &b in &bytes[ds..de] {
                den_mag = den_mag * 10 + (b - b'0') as u128;
                if den_mag > i64::MAX as u128 {
                    return fail(ds, ParseKind::OutOfRange);
                }
            }
            if den_mag == 0 {
                return fail(ds, ParseKind::ZeroDenominator);
            }
            if den_mag == 1 {
                return fail(ds - 1, ParseKind::NonCanonical);
            }
            if num_mag == 0 {
                return fail(ds - 1, ParseKind::NonCanonical);
            }
            if gcd_u128(num_mag, den_mag) > 1 {
                return fail(ds - 1, ParseKind::NonCanonical);
            }
        }

        let signed_num = if negative {
            -(num_mag as i128)
        } else {
            num_mag as i128
        };
        Self::from_parts(signed_num, den_mag as i128)
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
    DataLength {
        expected: usize,
        actual: usize,
    },
    Index {
        row: usize,
        col: usize,
    },
    DimensionMismatch {
        left: (usize, usize),
        right: (usize, usize),
    },
    Arithmetic {
        row: usize,
        col: usize,
        step: usize,
        source: RationalError,
    },
    NotSquare {
        rows: usize,
        cols: usize,
    },
    Singular {
        rank: usize,
    },
}

impl Matrix {
    pub fn new(rows: usize, cols: usize, data: Vec<Rational>) -> Result<Self, MatrixError> {
        let expected = rows.checked_mul(cols).ok_or(MatrixError::ShapeOverflow)?;
        if data.len() != expected {
            return Err(MatrixError::DataLength {
                expected,
                actual: data.len(),
            });
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
        let idx = row
            .checked_mul(self.cols)
            .and_then(|x| x.checked_add(col))
            .ok_or(MatrixError::Index { row, col })?;
        self.data
            .get(idx)
            .copied()
            .ok_or(MatrixError::Index { row, col })
    }

    pub fn transpose(&self) -> Self {
        let mut data = Vec::with_capacity(self.data.len());
        for r in 0..self.cols {
            for c in 0..self.rows {
                data.push(self.data[c * self.cols + r]);
            }
        }
        Self {
            rows: self.cols,
            cols: self.rows,
            data,
        }
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, MatrixError> {
        if self.cols != rhs.rows {
            return Err(MatrixError::DimensionMismatch {
                left: (self.rows, self.cols),
                right: (rhs.rows, rhs.cols),
            });
        }
        let out_len = self
            .rows
            .checked_mul(rhs.cols)
            .ok_or(MatrixError::ShapeOverflow)?;
        let mut data = Vec::with_capacity(out_len);
        for r in 0..self.rows {
            for c in 0..rhs.cols {
                let mut acc = Rational { num: 0, den: 1 };
                for k in 0..self.cols {
                    let left = self.data[r * self.cols + k];
                    let right = rhs.data[k * rhs.cols + c];
                    let term = left.mul(&right).map_err(|source| MatrixError::Arithmetic {
                        row: r,
                        col: c,
                        step: k,
                        source,
                    })?;
                    acc = acc.add(&term).map_err(|source| MatrixError::Arithmetic {
                        row: r,
                        col: c,
                        step: k,
                        source,
                    })?;
                }
                data.push(acc);
            }
        }
        Ok(Self {
            rows: self.rows,
            cols: rhs.cols,
            data,
        })
    }

    fn eliminate(&self, pivot_cols: usize) -> Result<Reduction, MatrixError> {
        let mut matrix = self.clone();
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let mut p = 0usize;

        for col in 0..pivot_cols {
            if p == matrix.rows {
                break;
            }

            let mut best: Option<usize> = None;
            for row in p..matrix.rows {
                let value = matrix.data[row * matrix.cols + col];
                if value.num == 0 {
                    continue;
                }
                let key = (value.den as u64, value.num.unsigned_abs(), row);
                if best.is_none_or(|b| {
                    let current = matrix.data[b * matrix.cols + col];
                    key < (current.den as u64, current.num.unsigned_abs(), b)
                }) {
                    best = Some(row);
                }
            }
            let Some(q) = best else {
                continue;
            };

            if q != p {
                for c in 0..matrix.cols {
                    matrix.data.swap(p * matrix.cols + c, q * matrix.cols + c);
                }
                swaps.push((p, q));
            }

            let pivot = matrix.data[p * matrix.cols + col];
            for c in 0..matrix.cols {
                let original = matrix.data[p * matrix.cols + c];
                matrix.data[p * matrix.cols + c] =
                    original
                        .div(&pivot)
                        .map_err(|source| MatrixError::Arithmetic {
                            row: p,
                            col: c,
                            step: p,
                            source,
                        })?;
            }

            for row in 0..matrix.rows {
                if row == p {
                    continue;
                }
                let factor = matrix.data[row * matrix.cols + col];
                if factor.num == 0 {
                    continue;
                }
                for c in 0..matrix.cols {
                    let product = factor.mul(&matrix.data[p * matrix.cols + c]).map_err(
                        |source| MatrixError::Arithmetic {
                            row,
                            col: c,
                            step: p,
                            source,
                        },
                    )?;
                    matrix.data[row * matrix.cols + c] = matrix.data[row * matrix.cols + c]
                        .sub(&product)
                        .map_err(|source| MatrixError::Arithmetic {
                            row,
                            col: c,
                            step: p,
                            source,
                        })?;
                }
            }

            pivots.push(col);
            p += 1;
        }

        Ok(Reduction {
            matrix,
            pivots,
            swaps,
        })
    }

    pub fn rref(&self) -> Result<Reduction, MatrixError> {
        self.eliminate(self.cols)
    }

    pub fn rank(&self) -> Result<usize, MatrixError> {
        Ok(self.rref()?.rank())
    }

    pub fn inverse(&self) -> Result<Self, MatrixError> {
        if self.rows != self.cols {
            return Err(MatrixError::NotSquare {
                rows: self.rows,
                cols: self.cols,
            });
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
                data.push(Rational {
                    num: if r == c { 1 } else { 0 },
                    den: 1,
                });
            }
        }

        let augmented = Self {
            rows: n,
            cols: width,
            data,
        };
        let reduction = augmented.eliminate(n)?;
        let rank = reduction.rank();
        if rank < n {
            return Err(MatrixError::Singular { rank });
        }

        let mut right = Vec::with_capacity(total / 2);
        for r in 0..n {
            for c in 0..n {
                right.push(reduction.matrix.data[r * width + n + c]);
            }
        }
        Ok(Self {
            rows: n,
            cols: n,
            data: right,
        })
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