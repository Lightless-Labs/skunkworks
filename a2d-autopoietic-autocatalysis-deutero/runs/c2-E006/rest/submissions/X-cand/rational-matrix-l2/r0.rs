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
        let n = (self.num as i128) * (rhs.num as i128);
        let d = (self.den as i128) * (rhs.den as i128);
        Self::from_i128(n, d)
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        let n = (self.num as i128) * (rhs.den as i128);
        let d = (self.den as i128) * (rhs.num as i128);
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

        let mut i = 0;
        if bytes[i] == b'-' {
            i += 1;
        }

        let num_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == num_start {
            return Err(parse_err(i, ParseKind::InvalidByte));
        }

        let num_end = i;
        let mut slash = None;
        let mut den_start = None;
        let mut den_end = None;

        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
            slash = Some(i);
            i += 1;
            let start = i;
            den_start = Some(start);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
            den_end = Some(i);
            if i < bytes.len() {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
        }

        let negative = bytes[0] == b'-';
        let num_digits = &bytes[num_start..num_end];
        if num_digits.len() > 1 && num_digits[0] == b'0' {
            return Err(parse_err(num_start + 1, ParseKind::NonCanonical));
        }
        if negative && num_digits == b"0" {
            return Err(parse_err(0, ParseKind::NonCanonical));
        }
        if let (Some(start), Some(end)) = (den_start, den_end) {
            if end - start > 1 && bytes[start] == b'0' {
                return Err(parse_err(start + 1, ParseKind::NonCanonical));
            }
        }

        let num_limit: u128 = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        let num_mag = parse_bounded_digits(num_digits, num_limit)
            .ok_or_else(|| parse_err(0, ParseKind::OutOfRange))?;
        let num = if negative {
            if num_mag == (1u128 << 63) {
                i64::MIN
            } else {
                -(num_mag as i64)
            }
        } else {
            num_mag as i64
        };

        let (den, den_at) = if let (Some(start), Some(end)) = (den_start, den_end) {
            let digits = &bytes[start..end];
            let magnitude = parse_bounded_digits(digits, i64::MAX as u128)
                .ok_or_else(|| parse_err(start, ParseKind::OutOfRange))?;
            (magnitude as i64, start)
        } else {
            (1, 0)
        };

        if slash.is_some() && den == 0 {
            return Err(parse_err(den_at, ParseKind::ZeroDenominator));
        }
        if slash.is_some() && den == 1 {
            return Err(parse_err(slash.unwrap(), ParseKind::NonCanonical));
        }
        if slash.is_some() && num == 0 {
            return Err(parse_err(slash.unwrap(), ParseKind::NonCanonical));
        }
        if slash.is_some() && gcd_u128(num.unsigned_abs() as u128, den as u128) > 1 {
            return Err(parse_err(slash.unwrap(), ParseKind::NonCanonical));
        }

        Ok(Self { num, den })
    }
}

fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

fn parse_bounded_digits(digits: &[u8], limit: u128) -> Option<u128> {
    let mut value = 0u128;
    for &digit in digits {
        let d = (digit - b'0') as u128;
        if value > (limit.saturating_sub(d)) / 10 {
            return None;
        }
        value = value * 10 + d;
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
            .and_then(|base| base.checked_add(col))
            .ok_or(MatrixError::Index { row, col })?;
        self.data
            .get(index)
            .copied()
            .ok_or(MatrixError::Index { row, col })
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