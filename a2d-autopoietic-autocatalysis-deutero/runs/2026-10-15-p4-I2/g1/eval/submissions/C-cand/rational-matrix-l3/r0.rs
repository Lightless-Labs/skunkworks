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
    Ok(Rational {
        num: n as i64,
        den: d as i64,
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
            return Err(RationalError::Parse {
                at: 0,
                kind: ParseKind::Empty,
            });
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
            return Err(RationalError::Parse {
                at: i,
                kind: if i < bytes.len() {
                    ParseKind::InvalidByte
                } else {
                    ParseKind::InvalidByte
                },
            });
        }

        let num_end = i;
        let mut den_start = None;
        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
            i += 1;
            den_start = Some(i);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == den_start.unwrap() {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
            if i < bytes.len() {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
        }

        if bytes[num_start] == b'0' && num_end - num_start >= 2 {
            return Err(RationalError::Parse {
                at: num_start + 1,
                kind: ParseKind::NonCanonical,
            });
        }
        if negative && num_end - num_start == 1 && bytes[num_start] == b'0' {
            return Err(RationalError::Parse {
                at: 0,
                kind: ParseKind::NonCanonical,
            });
        }
        if let Some(ds) = den_start {
            if bytes[ds] == b'0' && bytes.len() - ds >= 2 {
                return Err(RationalError::Parse {
                    at: ds + 1,
                    kind: ParseKind::NonCanonical,
                });
            }
        }

        let num_digits = &bytes[num_start..num_end];
        let mut magnitude: u128 = 0;
        for &b in num_digits {
            magnitude = magnitude * 10 + (b - b'0') as u128;
            let limit = if negative {
                1u128 << 63
            } else {
                i64::MAX as u128
            };
            if magnitude > limit {
                return Err(RationalError::Parse {
                    at: 0,
                    kind: ParseKind::OutOfRange,
                });
            }
        }

        let num = if negative {
            if magnitude == (1u128 << 63) {
                i64::MIN
            } else {
                -(magnitude as i64)
            }
        } else {
            magnitude as i64
        };

        let Some(ds) = den_start else {
            return Ok(Rational { num, den: 1 });
        };

        let mut den_mag: u128 = 0;
        for &b in &bytes[ds..] {
            den_mag = den_mag * 10 + (b - b'0') as u128;
            if den_mag > i64::MAX as u128 {
                return Err(RationalError::Parse {
                    at: ds,
                    kind: ParseKind::OutOfRange,
                });
            }
        }
        if den_mag == 0 {
            return Err(RationalError::Parse {
                at: ds,
                kind: ParseKind::ZeroDenominator,
            });
        }
        if den_mag == 1 {
            return Err(RationalError::Parse {
                at: ds - 1,
                kind: ParseKind::NonCanonical,
            });
        }
        if num == 0 {
            return Err(RationalError::Parse {
                at: ds - 1,
                kind: ParseKind::NonCanonical,
            });
        }
        if gcd_u128(num.unsigned_abs() as u128, den_mag) > 1 {
            return Err(RationalError::Parse {
                at: ds - 1,
                kind: ParseKind::NonCanonical,
            });
        }
        Ok(Rational {
            num,
            den: den_mag as i64,
        })
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
        Ok(self.data[row * self.cols + col])
    }

    pub fn transpose(&self) -> Self {
        let mut data = Vec::with_capacity(self.data.len());
        for c in 0..self.cols {
            for r in 0..self.rows {
                data.push(self.data[r * self.cols + c]);
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
        let len = self
            .rows
            .checked_mul(rhs.cols)
            .ok_or(MatrixError::ShapeOverflow)?;
        let mut data = Vec::with_capacity(len);
        let zero = Rational { num: 0, den: 1 };
        for r in 0..self.rows {
            for c in 0..rhs.cols {
                let mut acc = zero;
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

    pub fn rref(&self) -> Result<Reduction, MatrixError> {
        reduce(self.clone(), self.cols)
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
        let zero = Rational { num: 0, den: 1 };
        let one = Rational { num: 1, den: 1 };
        for r in 0..n {
            for c in 0..width {
                if c < n {
                    data.push(self.data[r * n + c]);
                } else {
                    data.push(if c - n == r { one } else { zero });
                }
            }
        }
        let augmented = Matrix {
            rows: n,
            cols: width,
            data,
        };
        let red = reduce(augmented, n)?;
        let rank = red.pivots.len();
        if rank < n {
            return Err(MatrixError::Singular { rank });
        }
        let mut inverse_data = Vec::with_capacity(n * n);
        for r in 0..n {
            for c in 0..n {
                inverse_data.push(red.matrix.data[r * width + n + c]);
            }
        }
        Ok(Self {
            rows: n,
            cols: n,
            data: inverse_data,
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

fn reduce(mut matrix: Matrix, pivot_cols: usize) -> Result<Reduction, MatrixError> {
    let mut pivots = Vec::new();
    let mut swaps = Vec::new();
    let mut p = 0usize;

    for col in 0..pivot_cols {
        if p == matrix.rows {
            break;
        }
        let mut chosen: Option<usize> = None;
        for row in p..matrix.rows {
            let value = matrix.data[row * matrix.cols + col];
            if value.num == 0 {
                continue;
            }
            let key = (value.den, value.num.unsigned_abs(), row);
            if chosen.map_or(true, |best| {
                let v = matrix.data[best * matrix.cols + col];
                key < (v.den, v.num.unsigned_abs(), best)
            }) {
                chosen = Some(row);
            }
        }
        let Some(q) = chosen else {
            continue;
        };

        if q != p {
            for c in 0..matrix.cols {
                matrix.data.swap(p * matrix.cols + c, q * matrix.cols + c);
            }
            swaps.push((p, q));
        }

        let pivot = matrix.data[p * matrix.cols + col];
        let mut normalized = Vec::with_capacity(matrix.cols);
        for c in 0..matrix.cols {
            normalized.push(
                matrix.data[p * matrix.cols + c]
                    .div(&pivot)
                    .map_err(|source| MatrixError::Arithmetic {
                        row: p,
                        col: c,
                        step: p,
                        source,
                    })?,
            );
        }
        for c in 0..matrix.cols {
            matrix.data[p * matrix.cols + c] = normalized[c];
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
                let product = factor.mul(&normalized[c]).map_err(|source| {
                    MatrixError::Arithmetic {
                        row,
                        col: c,
                        step: p,
                        source,
                    }
                })?;
                let updated = matrix.data[row * matrix.cols + c]
                    .sub(&product)
                    .map_err(|source| MatrixError::Arithmetic {
                        row,
                        col: c,
                        step: p,
                        source,
                    })?;
                matrix.data[row * matrix.cols + c] = updated;
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

fn main() {}