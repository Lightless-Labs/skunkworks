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

impl Rational {
    fn from_i128(num: i128, den: i128) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }

        let (mut num, mut den) = if den < 0 {
            (-num, -den)
        } else {
            (num, den)
        };

        if num == 0 {
            return Ok(Self { num: 0, den: 1 });
        }

        let gcd = gcd_u128(num.unsigned_abs(), den as u128) as i128;
        num /= gcd;
        den /= gcd;

        if num < i64::MIN as i128 || num > i64::MAX as i128 || den > i64::MAX as i128 {
            return Err(RationalError::Overflow);
        }

        Ok(Self {
            num: num as i64,
            den: den as i64,
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
        let a = self.num as i128;
        let b = self.den as i128;
        let c = rhs.num as i128;
        let d = rhs.den as i128;
        Self::from_i128(a * d + c * b, b * d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let a = self.num as i128;
        let b = self.den as i128;
        let c = rhs.num as i128;
        let d = rhs.den as i128;
        Self::from_i128(a * d - c * b, b * d)
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
            return Err(parse_error(0, ParseKind::Empty));
        }

        let mut i = 0;
        let negative = bytes[0] == b'-';
        if negative {
            i += 1;
        }

        let num_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == num_start {
            return Err(parse_error(i, ParseKind::InvalidByte));
        }

        let num_end = i;
        let mut den_start = None;
        let mut den_end = None;

        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(parse_error(i, ParseKind::InvalidByte));
            }
            i += 1;
            let start = i;
            den_start = Some(start);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return Err(parse_error(i, ParseKind::InvalidByte));
            }
            den_end = Some(i);
            if i < bytes.len() {
                return Err(parse_error(i, ParseKind::InvalidByte));
            }
        }

        if num_end - num_start >= 2 && bytes[num_start] == b'0' {
            return Err(parse_error(num_start + 1, ParseKind::NonCanonical));
        }
        if negative && num_end - num_start == 1 && bytes[num_start] == b'0' {
            return Err(parse_error(0, ParseKind::NonCanonical));
        }
        if let (Some(start), Some(end)) = (den_start, den_end) {
            if end - start >= 2 && bytes[start] == b'0' {
                return Err(parse_error(start + 1, ParseKind::NonCanonical));
            }
        }

        let num_mag = parse_magnitude(&bytes[num_start..num_end]);
        let num_limit = if negative { 1u128 << 63 } else { i64::MAX as u128 };
        if num_mag > num_limit {
            return Err(parse_error(0, ParseKind::OutOfRange));
        }

        let den_mag = if let (Some(start), Some(end)) = (den_start, den_end) {
            let mag = parse_magnitude(&bytes[start..end]);
            if mag > i64::MAX as u128 {
                return Err(parse_error(start, ParseKind::OutOfRange));
            }
            mag
        } else {
            1
        };

        if let (Some(start), Some(end)) = (den_start, den_end) {
            if den_mag == 0 {
                return Err(parse_error(start, ParseKind::ZeroDenominator));
            }
            if den_mag == 1 {
                return Err(parse_error(start - 1, ParseKind::NonCanonical));
            }
            if num_mag == 0 {
                return Err(parse_error(start - 1, ParseKind::NonCanonical));
            }
            if gcd_u128(num_mag, den_mag) > 1 {
                return Err(parse_error(start - 1, ParseKind::NonCanonical));
            }
            let n = if negative {
                if num_mag == (1u128 << 63) {
                    i64::MIN
                } else {
                    -(num_mag as i64)
                }
            } else {
                num_mag as i64
            };
            return Ok(Self {
                num: n,
                den: den_mag as i64,
            });
        }

        let n = if negative {
            if num_mag == (1u128 << 63) {
                i64::MIN
            } else {
                -(num_mag as i64)
            }
        } else {
            num_mag as i64
        };
        Ok(Self { num: n, den: 1 })
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

fn parse_error(at: usize, kind: ParseKind) -> RationalError {
    RationalError::Parse { at, kind }
}

fn parse_magnitude(bytes: &[u8]) -> u128 {
    bytes.iter().fold(0u128, |value, byte| {
        value * 10 + (byte - b'0') as u128
    })
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
        let mut data = Vec::with_capacity(output_len);

        for row in 0..self.rows {
            for col in 0..rhs.cols {
                let mut acc = Rational { num: 0, den: 1 };
                for step in 0..self.cols {
                    let left = self.data[row * self.cols + step];
                    let right = rhs.data[step * rhs.cols + col];
                    let term = left.mul(&right).map_err(|source| MatrixError::Arithmetic {
                        row,
                        col,
                        step,
                        source,
                    })?;
                    acc = acc.add(&term).map_err(|source| MatrixError::Arithmetic {
                        row,
                        col,
                        step,
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
}