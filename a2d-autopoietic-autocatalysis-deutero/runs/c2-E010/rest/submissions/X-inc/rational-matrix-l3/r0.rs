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

fn rational_from_i128_parts(num: i128, den: i128) -> Result<Rational, RationalError> {
    if den == 0 {
        return Err(RationalError::ZeroDenominator);
    }
    if num == 0 {
        return Ok(Rational { num: 0, den: 1 });
    }

    let mut n = num;
    let mut d = den;
    if d < 0 {
        n = n.checked_neg().ok_or(RationalError::Overflow)?;
        d = d.checked_neg().ok_or(RationalError::Overflow)?;
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

fn rational_from_i128_pair(num: i128, den: i128) -> Result<Rational, RationalError> {
    rational_from_i128_parts(num, den)
}

fn checked_rational(num: i128, den: i128) -> Result<Rational, RationalError> {
    rational_from_i128_pair(num, den)
}

impl Rational {
    pub fn new(num: i64, den: i64) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        rational_from_i128_parts(num as i128, den as i128)
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
        let n = (self.num as i128)
            .checked_mul(a)
            .and_then(|x| (rhs.num as i128).checked_mul(b).and_then(|y| x.checked_add(y)))
            .ok_or(RationalError::Overflow)?;
        let d = (self.den as i128)
            .checked_mul(a)
            .ok_or(RationalError::Overflow)?;
        checked_rational(n, d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let g = gcd_u128(self.den as u128, rhs.den as u128) as i128;
        let a = rhs.den as i128 / g;
        let b = self.den as i128 / g;
        let n = (self.num as i128)
            .checked_mul(a)
            .and_then(|x| (rhs.num as i128).checked_mul(b).and_then(|y| x.checked_sub(y)))
            .ok_or(RationalError::Overflow)?;
        let d = (self.den as i128)
            .checked_mul(a)
            .ok_or(RationalError::Overflow)?;
        checked_rational(n, d)
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        let g1 = gcd_u128(self.num.unsigned_abs() as u128, rhs.den as u128) as i128;
        let g2 = gcd_u128(rhs.num.unsigned_abs() as u128, self.den as u128) as i128;
        let n = (self.num as i128 / g1)
            .checked_mul(rhs.num as i128 / g2)
            .ok_or(RationalError::Overflow)?;
        let d = (self.den as i128 / g2)
            .checked_mul(rhs.den as i128 / g1)
            .ok_or(RationalError::Overflow)?;
        checked_rational(n, d)
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        let g1 = gcd_u128(self.num.unsigned_abs() as u128, rhs.num.unsigned_abs() as u128) as i128;
        let g2 = gcd_u128(self.den as u128, rhs.den as u128) as i128;
        let mut n = (self.num as i128 / g1)
            .checked_mul(rhs.den as i128 / g2)
            .ok_or(RationalError::Overflow)?;
        let mut d = (self.den as i128 / g2)
            .checked_mul(rhs.num as i128 / g1)
            .ok_or(RationalError::Overflow)?;
        if d < 0 {
            n = n.checked_neg().ok_or(RationalError::Overflow)?;
            d = d.checked_neg().ok_or(RationalError::Overflow)?;
        }
        checked_rational(n, d)
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        let n = self.num.checked_neg().ok_or(RationalError::Overflow)?;
        Ok(Rational { num: n, den: self.den })
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        fn parse_err(at: usize, kind: ParseKind) -> RationalError {
            RationalError::Parse { at, kind }
        }

        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(parse_err(0, ParseKind::Empty));
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
            return Err(parse_err(i, ParseKind::InvalidByte));
        }

        let num_end = i;
        let slash = if i < bytes.len() && bytes[i] == b'/' {
            let s = i;
            i += 1;
            Some(s)
        } else {
            None
        };

        let den_start = if slash.is_some() { i } else { 0 };
        let den_end;
        if slash.is_some() {
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == den_start {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
            den_end = i;
        } else {
            den_end = 0;
        }

        if i < bytes.len() {
            return Err(parse_err(i, ParseKind::InvalidByte));
        }

        if num_end - num_start >= 2 && bytes[num_start] == b'0' {
            return Err(parse_err(num_start + 1, ParseKind::NonCanonical));
        }
        if negative && num_end - num_start == 1 && bytes[num_start] == b'0' {
            return Err(parse_err(0, ParseKind::NonCanonical));
        }
        if slash.is_some() && den_end - den_start >= 2 && bytes[den_start] == b'0' {
            return Err(parse_err(den_start + 1, ParseKind::NonCanonical));
        }

        let num_limit: u128 = if negative { 1u128 << 63 } else { i64::MAX as u128 };
        let mut num_mag = 0u128;
        for &b in &bytes[num_start..num_end] {
            num_mag = num_mag
                .saturating_mul(10)
                .saturating_add((b - b'0') as u128);
            if num_mag > num_limit {
                return Err(parse_err(0, ParseKind::OutOfRange));
            }
        }

        let den_mag = if slash.is_some() {
            let mut value = 0u128;
            for &b in &bytes[den_start..den_end] {
                value = value
                    .saturating_mul(10)
                    .saturating_add((b - b'0') as u128);
                if value > i64::MAX as u128 {
                    return Err(parse_err(den_start, ParseKind::OutOfRange));
                }
            }
            value
        } else {
            1
        };

        if slash.is_some() && den_mag == 0 {
            return Err(parse_err(den_start, ParseKind::ZeroDenominator));
        }
        if let Some(s) = slash {
            if den_mag == 1 {
                return Err(parse_err(s, ParseKind::NonCanonical));
            }
            if num_mag == 0 {
                return Err(parse_err(s, ParseKind::NonCanonical));
            }
            if gcd_u128(num_mag, den_mag) > 1 {
                return Err(parse_err(s, ParseKind::NonCanonical));
            }
        }

        let n = if negative {
            if num_mag == 1u128 << 63 {
                i64::MIN
            } else {
                -(num_mag as i64)
            }
        } else {
            num_mag as i64
        };

        Ok(Rational {
            num: n,
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
        let idx = row
            .checked_mul(self.cols)
            .and_then(|base| base.checked_add(col))
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
        let size = self
            .rows
            .checked_mul(rhs.cols)
            .ok_or(MatrixError::ShapeOverflow)?;
        let mut data = Vec::with_capacity(size);
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

    pub fn rref(&self) -> Result<Reduction, MatrixError> {
        let mut work = self.clone();
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        eliminate(&mut work, self.cols, &mut pivots, &mut swaps)?;
        Ok(Reduction {
            matrix: work,
            pivots,
            swaps,
        })
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

        let mut augmented = Matrix {
            rows: n,
            cols: width,
            data,
        };
        let mut pivots = Vec::new();
        let mut swaps = Vec::new();
        eliminate(&mut augmented, n, &mut pivots, &mut swaps)?;
        if pivots.len() < n {
            return Err(MatrixError::Singular { rank: pivots.len() });
        }

        let mut inverse_data = Vec::with_capacity(n * n);
        for r in 0..n {
            for c in 0..n {
                inverse_data.push(augmented.data[r * width + n + c]);
            }
        }
        Ok(Matrix {
            rows: n,
            cols: n,
            data: inverse_data,
        })
    }
}

fn eliminate(
    matrix: &mut Matrix,
    candidate_cols: usize,
    pivots: &mut Vec<usize>,
    swaps: &mut Vec<(usize, usize)>,
) -> Result<(), MatrixError> {
    let mut p = 0usize;
    for col in 0..candidate_cols {
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
            if chosen
                .map(|current| {
                    let v = matrix.data[current * matrix.cols + col];
                    key < (v.den, v.num.unsigned_abs(), current)
                })
                .unwrap_or(true)
            {
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
                let current = matrix.data[row * matrix.cols + c];
                let pivot_entry = matrix.data[p * matrix.cols + c];
                let product = factor.mul(&pivot_entry).map_err(|source| {
                    MatrixError::Arithmetic {
                        row,
                        col: c,
                        step: p,
                        source,
                    }
                })?;
                matrix.data[row * matrix.cols + c] =
                    current
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
    Ok(())
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