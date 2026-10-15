use std::cmp::Ordering;
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

impl Rational {
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
        Self::from_i128(
            self.num as i128 * rhs.num as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        Self::from_i128(
            self.num as i128 * rhs.den as i128,
            self.den as i128 * rhs.num as i128,
        )
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        Self::from_i128(-(self.num as i128), self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(parse_err(0, ParseKind::Empty));
        }

        let mut slash = None;
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'/' {
                if slash.is_some() {
                    return Err(parse_err(i, ParseKind::InvalidByte));
                }
                slash = Some(i);
                continue;
            }
            if b.is_ascii_digit() {
                continue;
            }
            if b == b'-' && i == 0 {
                continue;
            }
            return Err(parse_err(i, ParseKind::InvalidByte));
        }

        let num_end = slash.unwrap_or(bytes.len());
        let num_start = usize::from(bytes[0] == b'-');
        if num_start == num_end {
            return Err(parse_err(bytes.len(), ParseKind::InvalidByte));
        }

        if let Some(s) = slash {
            if s + 1 == bytes.len() {
                return Err(parse_err(bytes.len(), ParseKind::InvalidByte));
            }
        }

        if num_end - num_start >= 2 && bytes[num_start] == b'0' {
            return Err(parse_err(num_start + 1, ParseKind::NonCanonical));
        }
        if bytes[0] == b'-' && num_end == 2 && bytes[1] == b'0' {
            return Err(parse_err(0, ParseKind::NonCanonical));
        }

        let den_range = slash.map(|s| s + 1..bytes.len());
        if let Some(range) = &den_range {
            if range.end - range.start >= 2 && bytes[range.start] == b'0' {
                return Err(parse_err(range.start + 1, ParseKind::NonCanonical));
            }
        }

        let negative = bytes[0] == b'-';
        let num_limit: u128 = if negative { 1u128 << 63 } else { i64::MAX as u128 };
        let num_mag = parse_magnitude(&bytes[num_start..num_end], num_limit)
            .ok_or_else(|| parse_err(0, ParseKind::OutOfRange))?;
        let num_i128 = if negative {
            -(num_mag as i128)
        } else {
            num_mag as i128
        };

        let den_value = if let Some(range) = den_range {
            let den = parse_magnitude(&bytes[range.clone()], i64::MAX as u128)
                .ok_or_else(|| parse_err(range.start, ParseKind::OutOfRange))?;
            if den == 0 {
                return Err(parse_err(range.start, ParseKind::ZeroDenominator));
            }
            if den == 1 {
                return Err(parse_err(slash.unwrap(), ParseKind::NonCanonical));
            }
            Some((den as i128, range.start))
        } else {
            None
        };

        if let Some((den, den_at)) = den_value {
            if num_i128 == 0 {
                return Err(parse_err(slash.unwrap(), ParseKind::NonCanonical));
            }
            if gcd_u128(num_i128.unsigned_abs(), den as u128) > 1 {
                return Err(parse_err(slash.unwrap(), ParseKind::NonCanonical));
            }
            let n = i64::try_from(num_i128).map_err(|_| parse_err(0, ParseKind::OutOfRange))?;
            let d = i64::try_from(den).map_err(|_| parse_err(den_at, ParseKind::OutOfRange))?;
            Ok(Self { num: n, den: d })
        } else {
            let n = i64::try_from(num_i128).map_err(|_| parse_err(0, ParseKind::OutOfRange))?;
            Ok(Self { num: n, den: 1 })
        }
    }

    fn from_i128(mut num: i128, mut den: i128) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        if num == 0 {
            return Ok(Self { num: 0, den: 1 });
        }
        if den < 0 {
            num = -num;
            den = -den;
        }
        let g = gcd_u128(num.unsigned_abs(), den as u128) as i128;
        num /= g;
        den /= g;
        if num < i64::MIN as i128 || num > i64::MAX as i128 || den < 1 || den > i64::MAX as i128 {
            return Err(RationalError::Overflow);
        }
        Ok(Self {
            num: num as i64,
            den: den as i64,
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

fn parse_err(at: usize, kind: ParseKind) -> RationalError {
    RationalError::Parse { at, kind }
}

fn parse_magnitude(bytes: &[u8], limit: u128) -> Option<u128> {
    let mut value = 0u128;
    for &b in bytes {
        value = value.checked_mul(10)?.checked_add((b - b'0') as u128)?;
        if value > limit {
            return None;
        }
    }
    Some(value)
}

fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
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
        let len = self.data.len();
        let mut data = vec![Rational { num: 0, den: 1 }; len];
        if self.rows != 0 && self.cols != 0 {
            for r in 0..self.rows {
                for c in 0..self.cols {
                    data[c * self.rows + r] = self.data[r * self.cols + c];
                }
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
        let mut matrix = self.clone();
        let (pivots, swaps) = eliminate(&mut matrix, self.cols)?;
        Ok(Reduction {
            matrix,
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

        let mut augmented = Self {
            rows: n,
            cols: width,
            data,
        };
        let (pivots, _) = eliminate(&mut augmented, n)?;
        if pivots.len() < n {
            return Err(MatrixError::Singular { rank: pivots.len() });
        }

        let right_len = n.checked_mul(n).ok_or(MatrixError::ShapeOverflow)?;
        let mut right = Vec::with_capacity(right_len);
        for r in 0..n {
            for c in 0..n {
                right.push(augmented.data[r * width + n + c]);
            }
        }
        Ok(Self {
            rows: n,
            cols: n,
            data: right,
        })
    }
}

fn eliminate(matrix: &mut Matrix, pivot_limit: usize) -> Result<(Vec<usize>, Vec<(usize, usize)>), MatrixError> {
    let mut pivots = Vec::new();
    let mut swaps = Vec::new();
    let mut p = 0usize;
    let zero = Rational { num: 0, den: 1 };

    for col in 0..pivot_limit {
        if p == matrix.rows {
            break;
        }

        let mut best: Option<usize> = None;
        for row in p..matrix.rows {
            let value = matrix.data[row * matrix.cols + col];
            if value.num == 0 {
                continue;
            }
            if best.map_or(true, |current| {
                pivot_key_cmp(value, row, matrix.data[current * matrix.cols + col], current)
                    == Ordering::Less
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
            let index = p * matrix.cols + c;
            matrix.data[index] = matrix.data[index]
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
            if factor == zero {
                continue;
            }
            for c in 0..matrix.cols {
                let product = factor
                    .mul(&matrix.data[p * matrix.cols + c])
                    .map_err(|source| MatrixError::Arithmetic {
                        row,
                        col: c,
                        step: p,
                        source,
                    })?;
                let index = row * matrix.cols + c;
                matrix.data[index] = matrix.data[index]
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

    Ok((pivots, swaps))
}

fn pivot_key_cmp(a: Rational, a_row: usize, b: Rational, b_row: usize) -> Ordering {
    a.den
        .cmp(&b.den)
        .then_with(|| a.num.unsigned_abs().cmp(&b.num.unsigned_abs()))
        .then_with(|| a_row.cmp(&b_row))
}

fn main() {}