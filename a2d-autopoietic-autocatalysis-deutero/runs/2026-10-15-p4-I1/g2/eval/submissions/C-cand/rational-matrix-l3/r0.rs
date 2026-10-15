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

impl Rational {
    fn from_i128(num: i128, den: i128) -> Result<Self, RationalError> {
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

        if n < i64::MIN as i128
            || n > i64::MAX as i128
            || d < 1
            || d > i64::MAX as i128
        {
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
        Self::from_i128(num as i128, den as i128)
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
        let n = self.num as i128 * a + rhs.num as i128 * b;
        let d = self.den as i128 * a;
        Self::from_i128(n, d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let g = gcd_u128(self.den as u128, rhs.den as u128) as i128;
        let a = rhs.den as i128 / g;
        let b = self.den as i128 / g;
        let n = self.num as i128 * a - rhs.num as i128 * b;
        let d = self.den as i128 * a;
        Self::from_i128(n, d)
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        let g1 = gcd_u128(self.num.unsigned_abs() as u128, rhs.den as u128) as i128;
        let g2 = gcd_u128(rhs.num.unsigned_abs() as u128, self.den as u128) as i128;
        let n = (self.num as i128 / g1) * (rhs.num as i128 / g2);
        let d = (self.den as i128 / g2) * (rhs.den as i128 / g1);
        Self::from_i128(n, d)
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        let g1 = gcd_u128(
            self.num.unsigned_abs() as u128,
            rhs.num.unsigned_abs() as u128,
        ) as i128;
        let g2 = gcd_u128(self.den as u128, rhs.den as u128) as i128;
        let n = (self.num as i128 / g1) * (rhs.den as i128 / g2);
        let d = (self.den as i128 / g2) * (rhs.num as i128 / g1);
        Self::from_i128(n, d)
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        Self::from_i128(-(self.num as i128), self.den as i128)
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
        let mut den_start = None;
        let mut den_end = None;

        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
            i += 1;
            den_start = Some(i);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == den_start.unwrap() {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
            den_end = Some(i);
            if i < bytes.len() {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
        }

        if num_end - num_start >= 2 && bytes[num_start] == b'0' {
            return Err(parse_err(num_start + 1, ParseKind::NonCanonical));
        }
        if negative && num_end == 2 && bytes[1] == b'0' {
            return Err(parse_err(0, ParseKind::NonCanonical));
        }
        if let (Some(ds), Some(de)) = (den_start, den_end) {
            if de - ds >= 2 && bytes[ds] == b'0' {
                return Err(parse_err(ds + 1, ParseKind::NonCanonical));
            }
        }

        let parse_magnitude =
            |start: usize, end: usize, limit: u128| -> Option<u128> {
                let mut value = 0u128;
                for &b in &bytes[start..end] {
                    let digit = (b - b'0') as u128;
                    value = value.checked_mul(10)?.checked_add(digit)?;
                    if value > limit {
                        return None;
                    }
                }
                Some(value)
            };

        let num_limit = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        let mag = parse_magnitude(num_start, num_end, num_limit)
            .ok_or_else(|| parse_err(0, ParseKind::OutOfRange))?;
        let num = if negative {
            if mag == 1u128 << 63 {
                i64::MIN
            } else {
                -(mag as i64)
            }
        } else {
            mag as i64
        };

        let slash = den_start.map(|_| num_end);
        let den = if let (Some(ds), Some(de)) = (den_start, den_end) {
            let value = parse_magnitude(ds, de, i64::MAX as u128)
                .ok_or_else(|| parse_err(ds, ParseKind::OutOfRange))?;
            if value == 0 {
                return Err(parse_err(ds, ParseKind::ZeroDenominator));
            }
            value as i64
        } else {
            1
        };

        if let Some(slash_at) = slash {
            if den == 1 {
                return Err(parse_err(slash_at, ParseKind::NonCanonical));
            }
            if num == 0 {
                return Err(parse_err(slash_at, ParseKind::NonCanonical));
            }
            if gcd_u128(num.unsigned_abs() as u128, den as u128) > 1 {
                return Err(parse_err(slash_at, ParseKind::NonCanonical));
            }
        }

        Ok(Self { num, den })
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
pub struct Matrix {
    rows: usize,
    cols: usize,
    data: Vec<Rational>,
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
        Ok(self.data[row * self.cols + col])
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
        let mut out = Vec::with_capacity(out_len);
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
                out.push(acc);
            }
        }
        Ok(Self {
            rows: self.rows,
            cols: rhs.cols,
            data: out,
        })
    }

    fn eliminate(
        mut matrix: Matrix,
        pivot_cols: usize,
    ) -> Result<Reduction, MatrixError> {
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        let mut p = 0usize;
        let cols = matrix.cols;

        for candidate in 0..pivot_cols {
            if p == matrix.rows {
                break;
            }
            let mut best: Option<usize> = None;
            for row in p..matrix.rows {
                let value = matrix.data[row * cols + candidate];
                if value.num == 0 {
                    continue;
                }
                let replace = match best {
                    None => true,
                    Some(old) => {
                        let current = matrix.data[row * cols + candidate];
                        let prior = matrix.data[old * cols + candidate];
                        (current.den, current.num.unsigned_abs(), row)
                            < (prior.den, prior.num.unsigned_abs(), old)
                    }
                };
                if replace {
                    best = Some(row);
                }
            }
            let Some(q) = best else {
                continue;
            };

            if q != p {
                for c in 0..cols {
                    matrix.data.swap(p * cols + c, q * cols + c);
                }
                swaps.push((p, q));
            }

            let pivot = matrix.data[p * cols + candidate];
            for c in 0..cols {
                let value = matrix.data[p * cols + c];
                matrix.data[p * cols + c] =
                    value.div(&pivot).map_err(|source| MatrixError::Arithmetic {
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
                let factor = matrix.data[row * cols + candidate];
                if factor.num == 0 {
                    continue;
                }
                for c in 0..cols {
                    let product = factor
                        .mul(&matrix.data[p * cols + c])
                        .map_err(|source| MatrixError::Arithmetic {
                            row,
                            col: c,
                            step: p,
                            source,
                        })?;
                    matrix.data[row * cols + c] = matrix.data[row * cols + c]
                        .sub(&product)
                        .map_err(|source| MatrixError::Arithmetic {
                            row,
                            col: c,
                            step: p,
                            source,
                        })?;
                }
            }

            pivots.push(candidate);
            p += 1;
        }

        Ok(Reduction {
            matrix,
            pivots,
            swaps,
        })
    }

    pub fn rref(&self) -> Result<Reduction, MatrixError> {
        Self::eliminate(self.clone(), self.cols)
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
        let len = n.checked_mul(width).ok_or(MatrixError::ShapeOverflow)?;
        let mut data = Vec::with_capacity(len);
        for r in 0..n {
            for c in 0..width {
                if c < n {
                    data.push(self.data[r * n + c]);
                } else {
                    data.push(Rational {
                        num: if c - n == r { 1 } else { 0 },
                        den: 1,
                    });
                }
            }
        }
        let augmented = Matrix {
            rows: n,
            cols: width,
            data,
        };
        let reduction = Self::eliminate(augmented, n)?;
        if reduction.pivots.len() < n {
            return Err(MatrixError::Singular {
                rank: reduction.pivots.len(),
            });
        }

        let mut inverse_data = Vec::with_capacity(n * n);
        for r in 0..n {
            for c in 0..n {
                inverse_data.push(reduction.matrix.data[r * width + n + c]);
            }
        }
        Ok(Self {
            rows: n,
            cols: n,
            data: inverse_data,
        })
    }
}

fn main() {}