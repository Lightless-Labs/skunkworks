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
        if num == 0 {
            return Ok(Self { num: 0, den: 1 });
        }

        let (mut n, mut d) = (num, den);
        if d < 0 {
            n = -n;
            d = -d;
        }

        let gcd = gcd_u128(n.unsigned_abs(), d as u128) as i128;
        n /= gcd;
        d /= gcd;

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
        let n = self.num as i128 * rhs.den as i128
            + rhs.num as i128 * self.den as i128;
        let d = self.den as i128 * rhs.den as i128;
        Self::from_i128(n, d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let n = self.num as i128 * rhs.den as i128
            - rhs.num as i128 * self.den as i128;
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
        let error = |at, kind| RationalError::Parse { at, kind };

        if bytes.is_empty() {
            return Err(error(0, ParseKind::Empty));
        }

        let negative = bytes[0] == b'-';
        let num_start = if negative { 1 } else { 0 };

        // Validate the entire grammar before checking canonicality or magnitude.
        if num_start == bytes.len() {
            return Err(error(bytes.len(), ParseKind::InvalidByte));
        }

        let mut i = num_start;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == num_start {
            return Err(error(i, ParseKind::InvalidByte));
        }

        let num_end = i;
        let mut slash_at = None;
        let mut den_start = None;
        let mut den_end = None;

        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(error(i, ParseKind::InvalidByte));
            }
            slash_at = Some(i);
            i += 1;
            den_start = Some(i);

            if i == bytes.len() {
                return Err(error(bytes.len(), ParseKind::InvalidByte));
            }

            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return Err(error(i, ParseKind::InvalidByte));
            }
            den_end = Some(i);

            if i < bytes.len() {
                return Err(error(i, ParseKind::InvalidByte));
            }
        }

        if num_end - num_start >= 2 && bytes[num_start] == b'0' {
            return Err(error(num_start + 1, ParseKind::NonCanonical));
        }
        if negative && num_end - num_start == 1 && bytes[num_start] == b'0' {
            return Err(error(0, ParseKind::NonCanonical));
        }

        if let (Some(start), Some(end)) = (den_start, den_end) {
            if end - start >= 2 && bytes[start] == b'0' {
                return Err(error(start + 1, ParseKind::NonCanonical));
            }
        }

        let numerator_limit = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        let mut numerator_magnitude = 0u128;
        for &byte in &bytes[num_start..num_end] {
            numerator_magnitude = numerator_magnitude * 10 + (byte - b'0') as u128;
            if numerator_magnitude > numerator_limit {
                return Err(error(0, ParseKind::OutOfRange));
            }
        }

        let mut denominator_magnitude = 1u128;
        if let (Some(start), Some(end)) = (den_start, den_end) {
            denominator_magnitude = 0;
            for &byte in &bytes[start..end] {
                denominator_magnitude = denominator_magnitude * 10 + (byte - b'0') as u128;
                if denominator_magnitude > i64::MAX as u128 {
                    return Err(error(start, ParseKind::OutOfRange));
                }
            }
        }

        if let (Some(start), Some(slash)) = (den_start, slash_at) {
            if denominator_magnitude == 0 {
                return Err(error(start, ParseKind::ZeroDenominator));
            }
            if denominator_magnitude == 1 {
                return Err(error(slash, ParseKind::NonCanonical));
            }
            if numerator_magnitude == 0 {
                return Err(error(slash, ParseKind::NonCanonical));
            }
            if gcd_u128(numerator_magnitude, denominator_magnitude) > 1 {
                return Err(error(slash, ParseKind::NonCanonical));
            }
        }

        let signed_numerator = if negative {
            if numerator_magnitude == (1u128 << 63) {
                i64::MIN as i128
            } else {
                -(numerator_magnitude as i128)
            }
        } else {
            numerator_magnitude as i128
        };

        Self::from_i128(signed_numerator, denominator_magnitude as i128)
            .map_err(|_| error(0, ParseKind::OutOfRange))
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

        let output_len = self
            .rows
            .checked_mul(rhs.cols)
            .ok_or(MatrixError::ShapeOverflow)?;
        let mut data = Vec::with_capacity(output_len);

        for row in 0..self.rows {
            for col in 0..rhs.cols {
                let mut accumulator = Rational { num: 0, den: 1 };
                for step in 0..self.cols {
                    let left = self.data[row * self.cols + step];
                    let right = rhs.data[step * rhs.cols + col];
                    let term = left.mul(&right).map_err(|source| {
                        MatrixError::Arithmetic {
                            row,
                            col,
                            step,
                            source,
                        }
                    })?;
                    accumulator = accumulator.add(&term).map_err(|source| {
                        MatrixError::Arithmetic {
                            row,
                            col,
                            step,
                            source,
                        }
                    })?;
                }
                data.push(accumulator);
            }
        }

        Ok(Self {
            rows: self.rows,
            cols: rhs.cols,
            data,
        })
    }
}