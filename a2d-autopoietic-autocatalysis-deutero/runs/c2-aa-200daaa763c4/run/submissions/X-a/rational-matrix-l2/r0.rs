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

        let negative = (num < 0) ^ (den < 0);
        let n = num.unsigned_abs();
        let d = den.unsigned_abs();
        let gcd = gcd_u128(n, d);
        let n = n / gcd;
        let d = d / gcd;

        if d > i64::MAX as u128 {
            return Err(RationalError::Overflow);
        }

        let limit = if negative {
            (i64::MAX as u128) + 1
        } else {
            i64::MAX as u128
        };
        if n > limit {
            return Err(RationalError::Overflow);
        }

        let num = if negative {
            if n == (i64::MAX as u128) + 1 {
                i64::MIN
            } else {
                -(n as i64)
            }
        } else {
            n as i64
        };

        Ok(Self {
            num,
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
        let n = (self.num as i128) * (rhs.den as i128)
            + (rhs.num as i128) * (self.den as i128);
        let d = (self.den as i128) * (rhs.den as i128);
        Self::from_i128(n, d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let n = (self.num as i128) * (rhs.den as i128)
            - (rhs.num as i128) * (self.den as i128);
        let d = (self.den as i128) * (rhs.den as i128);
        Self::from_i128(n, d)
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
        if self.num == i64::MIN {
            return Err(RationalError::Overflow);
        }
        Ok(Self {
            num: -self.num,
            den: self.den,
        })
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        let error = |at, kind| RationalError::Parse { at, kind };

        if bytes.is_empty() {
            return Err(error(0, ParseKind::Empty));
        }

        let negative = bytes[0] == b'-';
        let mut i = usize::from(negative);
        let numerator_start = i;

        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == numerator_start {
            return Err(error(i, ParseKind::InvalidByte));
        }

        let numerator_end = i;
        let mut denominator_start = None;
        let mut denominator_end = None;

        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(error(i, ParseKind::InvalidByte));
            }

            i += 1;
            let start = i;
            denominator_start = Some(start);

            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return Err(error(i, ParseKind::InvalidByte));
            }
            denominator_end = Some(i);

            if i < bytes.len() {
                return Err(error(i, ParseKind::InvalidByte));
            }
        }

        let numerator_digits = &bytes[numerator_start..numerator_end];

        if numerator_digits.len() >= 2 && numerator_digits[0] == b'0' {
            return Err(error(
                if negative { 2 } else { 1 },
                ParseKind::NonCanonical,
            ));
        }
        if negative && numerator_digits == b"0" {
            return Err(error(0, ParseKind::NonCanonical));
        }

        if let (Some(start), Some(end)) = (denominator_start, denominator_end) {
            let digits = &bytes[start..end];
            if digits.len() >= 2 && digits[0] == b'0' {
                return Err(error(start + 1, ParseKind::NonCanonical));
            }
        }

        let numerator_limit = if negative {
            (i64::MAX as u128) + 1
        } else {
            i64::MAX as u128
        };
        let mut numerator_magnitude = 0u128;
        for &byte in numerator_digits {
            numerator_magnitude =
                numerator_magnitude * 10 + (byte - b'0') as u128;
            if numerator_magnitude > numerator_limit {
                return Err(error(0, ParseKind::OutOfRange));
            }
        }

        let mut denominator_magnitude = 1u128;
        if let (Some(start), Some(end)) = (denominator_start, denominator_end) {
            denominator_magnitude = 0;
            for &byte in &bytes[start..end] {
                denominator_magnitude =
                    denominator_magnitude * 10 + (byte - b'0') as u128;
                if denominator_magnitude > i64::MAX as u128 {
                    return Err(error(start, ParseKind::OutOfRange));
                }
            }

            if denominator_magnitude == 0 {
                return Err(error(start, ParseKind::ZeroDenominator));
            }
        }

        if let (Some(start), Some(_)) = (denominator_start, denominator_end) {
            let slash = start - 1;
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

        let numerator = if negative {
            if numerator_magnitude == (i64::MAX as u128) + 1 {
                i64::MIN
            } else {
                -(numerator_magnitude as i64)
            }
        } else {
            numerator_magnitude as i64
        };

        let denominator = denominator_magnitude as i64;
        Self::new(numerator, denominator)
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
        let expected = rows
            .checked_mul(cols)
            .ok_or(MatrixError::ShapeOverflow)?;
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
        let index = row * self.cols + col;
        Ok(self.data[index])
    }

    pub fn transpose(&self) -> Self {
        let mut data = Vec::with_capacity(self.data.len());
        for col in 0..self.cols {
            for row in 0..self.rows {
                data.push(self.data[row * self.cols + col]);
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

        let mut output = Vec::with_capacity(output_len);
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
                output.push(accumulator);
            }
        }

        Ok(Self {
            rows: self.rows,
            cols: rhs.cols,
            data: output,
        })
    }
}