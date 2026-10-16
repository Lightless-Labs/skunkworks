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
    pub fn new(num: i64, den: i64) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        Self::from_i128(num as i128, den as i128)
    }

    fn from_i128(num: i128, den: i128) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        if num == 0 {
            return Ok(Self { num: 0, den: 1 });
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
        Ok(Self {
            num: n as i64,
            den: d as i64,
        })
    }

    pub fn numerator(&self) -> i64 {
        self.num
    }

    pub fn denominator(&self) -> i64 {
        self.den
    }

    pub fn add(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::from_i128(
            (self.num as i128) * (rhs.den as i128) + (rhs.num as i128) * (self.den as i128),
            (self.den as i128) * (rhs.den as i128),
        )
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::from_i128(
            (self.num as i128) * (rhs.den as i128) - (rhs.num as i128) * (self.den as i128),
            (self.den as i128) * (rhs.den as i128),
        )
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::from_i128(
            (self.num as i128) * (rhs.num as i128),
            (self.den as i128) * (rhs.den as i128),
        )
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        Self::from_i128(
            (self.num as i128) * (rhs.den as i128),
            (self.den as i128) * (rhs.num as i128),
        )
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        Self::from_i128(-(self.num as i128), self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(RationalError::Parse {
                at: 0,
                kind: ParseKind::Empty,
            });
        }

        let mut i = 0usize;
        let negative = bytes[0] == b'-';
        if negative {
            i += 1;
        }
        let num_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == num_start {
            return Err(RationalError::Parse {
                at: i,
                kind: ParseKind::InvalidByte,
            });
        }

        let num_end = i;
        let mut den_start = None;
        let mut den_end = None;
        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
            i += 1;
            let start = i;
            den_start = Some(start);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
            den_end = Some(i);
            if i < bytes.len() {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
        }

        if num_end - num_start >= 2 && bytes[num_start] == b'0' {
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
        if let (Some(start), Some(end)) = (den_start, den_end) {
            if end - start >= 2 && bytes[start] == b'0' {
                return Err(RationalError::Parse {
                    at: start + 1,
                    kind: ParseKind::NonCanonical,
                });
            }
        }

        let num_mag = parse_magnitude(&bytes[num_start..num_end]).ok_or(
            RationalError::Parse {
                at: 0,
                kind: ParseKind::OutOfRange,
            },
        )?;
        let num_limit = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        if num_mag > num_limit {
            return Err(RationalError::Parse {
                at: 0,
                kind: ParseKind::OutOfRange,
            });
        }

        let den_mag = if let (Some(start), Some(end)) = (den_start, den_end) {
            let value = parse_magnitude(&bytes[start..end]).ok_or(RationalError::Parse {
                at: start,
                kind: ParseKind::OutOfRange,
            })?;
            if value > i64::MAX as u128 {
                return Err(RationalError::Parse {
                    at: start,
                    kind: ParseKind::OutOfRange,
                });
            }
            value
        } else {
            1
        };

        if den_mag == 0 {
            return Err(RationalError::Parse {
                at: den_start.unwrap(),
                kind: ParseKind::ZeroDenominator,
            });
        }
        if den_start.is_some() && den_mag == 1 {
            return Err(RationalError::Parse {
                at: num_end,
                kind: ParseKind::NonCanonical,
            });
        }
        if den_start.is_some() && num_mag == 0 {
            return Err(RationalError::Parse {
                at: num_end,
                kind: ParseKind::NonCanonical,
            });
        }
        if den_start.is_some() && gcd_u128(num_mag, den_mag) > 1 {
            return Err(RationalError::Parse {
                at: num_end,
                kind: ParseKind::NonCanonical,
            });
        }

        let signed_num = if negative {
            if num_mag == (1u128 << 63) {
                i64::MIN
            } else {
                -(num_mag as i64)
            }
        } else {
            num_mag as i64
        };
        Ok(Self {
            num: signed_num,
            den: den_mag as i64,
        })
    }
}

fn parse_magnitude(bytes: &[u8]) -> Option<u128> {
    let mut value = 0u128;
    for &b in bytes {
        value = value.checked_mul(10)?.checked_add((b - b'0') as u128)?;
    }
    Some(value)
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
        let index = row
            .checked_mul(self.cols)
            .and_then(|v| v.checked_add(col))
            .ok_or(MatrixError::Index { row, col })?;
        self.data
            .get(index)
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
        eliminate(self, self.cols, self.cols)
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
            for c in 0..n {
                data.push(self.data[r * n + c]);
            }
            for c in 0..n {
                data.push(if r == c { one } else { zero });
            }
        }
        let augmented = Matrix {
            rows: n,
            cols: width,
            data,
        };
        let reduction = eliminate(&augmented, n, width)?;
        if reduction.pivots.len() < n {
            return Err(MatrixError::Singular {
                rank: reduction.pivots.len(),
            });
        }
        let mut right = Vec::with_capacity(n * n);
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

fn eliminate(
    matrix: &Matrix,
    pivot_columns: usize,
    update_columns: usize,
) -> Result<Reduction, MatrixError> {
    let mut work = matrix.clone();
    let mut pivots = Vec::new();
    let mut swaps = Vec::new();
    let mut p = 0usize;

    for col in 0..pivot_columns {
        if p == work.rows {
            break;
        }
        let mut chosen: Option<usize> = None;
        for row in p..work.rows {
            let value = work.data[row * work.cols + col];
            if value.num == 0 {
                continue;
            }
            let key = (value.den, value.num.unsigned_abs(), row);
            if chosen.map_or(true, |old| {
                let v = work.data[old * work.cols + col];
                key < (v.den, v.num.unsigned_abs(), old)
            }) {
                chosen = Some(row);
            }
        }
        let Some(q) = chosen else {
            continue;
        };
        if q != p {
            for c in 0..update_columns {
                work.data.swap(p * work.cols + c, q * work.cols + c);
            }
            swaps.push((p, q));
        }

        let pivot = work.data[p * work.cols + col];
        for c in 0..update_columns {
            let current = work.data[p * work.cols + c];
            work.data[p * work.cols + c] =
                current
                    .div(&pivot)
                    .map_err(|source| MatrixError::Arithmetic {
                        row: p,
                        col: c,
                        step: p,
                        source,
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
            for c in 0..update_columns {
                let product = factor
                    .mul(&work.data[p * work.cols + c])
                    .map_err(|source| MatrixError::Arithmetic {
                        row,
                        col: c,
                        step: p,
                        source,
                    })?;
                work.data[row * work.cols + c] = work.data[row * work.cols + c]
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
        matrix: work,
        pivots,
        swaps,
    })
}