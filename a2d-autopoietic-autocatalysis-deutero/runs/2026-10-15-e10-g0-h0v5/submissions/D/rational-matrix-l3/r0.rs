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

fn rational_from_parts(num: i128, den: i128) -> Result<Rational, RationalError> {
    if den == 0 {
        return Err(RationalError::ZeroDenominator);
    }
    if num == 0 {
        return Ok(Rational { num: 0, den: 1 });
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
        rational_from_parts(num as i128, den as i128)
    }

    pub fn numerator(&self) -> i64 {
        self.num
    }

    pub fn denominator(&self) -> i64 {
        self.den
    }

    pub fn add(&self, rhs: &Self) -> Result<Self, RationalError> {
        let g = gcd_u128(self.den as u128, rhs.den as u128) as i128;
        let a = rhs.den as i128 / g;
        let b = self.den as i128 / g;
        rational_from_parts(
            self.num as i128 * a + rhs.num as i128 * b,
            self.den as i128 * a,
        )
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let g = gcd_u128(self.den as u128, rhs.den as u128) as i128;
        let a = rhs.den as i128 / g;
        let b = self.den as i128 / g;
        rational_from_parts(
            self.num as i128 * a - rhs.num as i128 * b,
            self.den as i128 * a,
        )
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        let g1 = gcd_u128(self.num.unsigned_abs() as u128, rhs.den as u128) as i128;
        let g2 = gcd_u128(rhs.num.unsigned_abs() as u128, self.den as u128) as i128;
        rational_from_parts(
            (self.num as i128 / g1) * (rhs.num as i128 / g2),
            (self.den as i128 / g2) * (rhs.den as i128 / g1),
        )
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        rational_from_parts(
            self.num as i128 * rhs.den as i128,
            self.den as i128 * rhs.num as i128,
        )
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        rational_from_parts(-(self.num as i128), self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        let parse_err = |at, kind| RationalError::Parse { at, kind };

        if bytes.is_empty() {
            return Err(parse_err(0, ParseKind::Empty));
        }

        let negative = bytes[0] == b'-';
        let num_start = usize::from(negative);
        let mut i = num_start;

        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == num_start {
            return Err(parse_err(i, ParseKind::InvalidByte));
        }
        let num_end = i;

        let mut den_range = None;
        if i < bytes.len() && bytes[i] == b'/' {
            i += 1;
            let den_start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == den_start {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
            den_range = Some((den_start, i));
        }

        if i < bytes.len() {
            return Err(parse_err(i, ParseKind::InvalidByte));
        }

        let num_digits = num_end - num_start;
        if num_digits >= 2 && bytes[num_start] == b'0' {
            return Err(parse_err(num_start + 1, ParseKind::NonCanonical));
        }
        if negative && num_digits == 1 && bytes[num_start] == b'0' {
            return Err(parse_err(0, ParseKind::NonCanonical));
        }

        if let Some((den_start, den_end)) = den_range {
            if den_end - den_start >= 2 && bytes[den_start] == b'0' {
                return Err(parse_err(den_start + 1, ParseKind::NonCanonical));
            }
        }

        let num_limit = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        let mut num_mag = 0u128;
        for &b in &bytes[num_start..num_end] {
            num_mag = num_mag
                .saturating_mul(10)
                .saturating_add((b - b'0') as u128);
            if num_mag > num_limit {
                return Err(parse_err(0, ParseKind::OutOfRange));
            }
        }

        let mut den_mag = 1u128;
        if let Some((den_start, den_end)) = den_range {
            den_mag = 0;
            for &b in &bytes[den_start..den_end] {
                den_mag = den_mag
                    .saturating_mul(10)
                    .saturating_add((b - b'0') as u128);
                if den_mag > i64::MAX as u128 {
                    return Err(parse_err(den_start, ParseKind::OutOfRange));
                }
            }

            if den_mag == 0 {
                return Err(parse_err(den_start, ParseKind::ZeroDenominator));
            }

            let slash = den_start - 1;
            if den_mag == 1 || num_mag == 0 || gcd_u128(num_mag, den_mag) > 1 {
                return Err(parse_err(slash, ParseKind::NonCanonical));
            }
        }

        let signed_num = if negative {
            -(num_mag as i128)
        } else {
            num_mag as i128
        };
        rational_from_parts(signed_num, den_mag as i128)
            .map_err(|_| parse_err(0, ParseKind::OutOfRange))
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

        for r in 0..self.rows {
            for c in 0..rhs.cols {
                let mut acc = Rational { num: 0, den: 1 };
                for k in 0..self.cols {
                    let a = self.data[r * self.cols + k];
                    let b = rhs.data[k * rhs.cols + c];
                    let term = a.mul(&b).map_err(|source| MatrixError::Arithmetic {
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
        let mut m = self.clone();
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let mut p = 0usize;

        for col in 0..pivot_cols {
            if p == m.rows {
                break;
            }

            let mut best: Option<usize> = None;
            for row in p..m.rows {
                let value = m.data[row * m.cols + col];
                if value.num == 0 {
                    continue;
                }
                let better = match best {
                    None => true,
                    Some(old) => {
                        let old_value = m.data[old * m.cols + col];
                        (value.den, value.num.unsigned_abs(), row)
                            < (old_value.den, old_value.num.unsigned_abs(), old)
                    }
                };
                if better {
                    best = Some(row);
                }
            }

            let Some(q) = best else {
                continue;
            };

            if q != p {
                for c in 0..m.cols {
                    m.data.swap(p * m.cols + c, q * m.cols + c);
                }
                swaps.push((p, q));
            }

            let pivot = m.data[p * m.cols + col];
            let mut normalized = Vec::with_capacity(m.cols);
            for c in 0..m.cols {
                let value = m.data[p * m.cols + c]
                    .div(&pivot)
                    .map_err(|source| MatrixError::Arithmetic {
                        row: p,
                        col: c,
                        step: p,
                        source,
                    })?;
                normalized.push(value);
            }
            for c in 0..m.cols {
                m.data[p * m.cols + c] = normalized[c];
            }

            for row in 0..m.rows {
                if row == p {
                    continue;
                }
                let factor = m.data[row * m.cols + col];
                if factor.num == 0 {
                    continue;
                }

                for c in 0..m.cols {
                    let product = factor
                        .mul(&m.data[p * m.cols + c])
                        .map_err(|source| MatrixError::Arithmetic {
                            row,
                            col: c,
                            step: p,
                            source,
                        })?;
                    let updated = m.data[row * m.cols + c]
                        .sub(&product)
                        .map_err(|source| MatrixError::Arithmetic {
                            row,
                            col: c,
                            step: p,
                            source,
                        })?;
                    m.data[row * m.cols + c] = updated;
                }
            }

            pivots.push(col);
            p += 1;
        }

        Ok(Reduction {
            matrix: m,
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
                data.push(if r == c {
                    Rational { num: 1, den: 1 }
                } else {
                    Rational { num: 0, den: 1 }
                });
            }
        }

        let augmented = Self {
            rows: n,
            cols: width,
            data,
        };
        let reduction = augmented.eliminate(n)?;

        if reduction.pivots.len() < n {
            return Err(MatrixError::Singular {
                rank: reduction.pivots.len(),
            });
        }

        let mut result = Vec::with_capacity(n * n);
        for r in 0..n {
            for c in n..width {
                result.push(reduction.matrix.data[r * width + c]);
            }
        }

        Ok(Self {
            rows: n,
            cols: n,
            data: result,
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