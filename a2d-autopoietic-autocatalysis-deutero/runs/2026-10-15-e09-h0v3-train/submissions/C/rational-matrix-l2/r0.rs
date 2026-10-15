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

        let (mut n, mut d) = (num, den);
        if d < 0 {
            n = -n;
            d = -d;
        }

        let g = gcd_u128(n.unsigned_abs(), d as u128) as i128;
        n /= g;
        d /= g;

        if n < i64::MIN as i128 || n > i64::MAX as i128 || d > i64::MAX as i128 {
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
            self.num as i128 * rhs.den as i128 + rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::from_i128(
            self.num as i128 * rhs.den as i128 - rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
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

        let numerator_start = i;
        if i == bytes.len() {
            return Err(parse_err(i, ParseKind::InvalidByte));
        }
        if !bytes[i].is_ascii_digit() {
            return Err(parse_err(i, ParseKind::InvalidByte));
        }
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        let numerator_end = i;

        let mut slash = None;
        let mut denominator_start = None;
        let mut denominator_end = None;
        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
            slash = Some(i);
            i += 1;
            denominator_start = Some(i);
            if i == bytes.len() {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
            if !bytes[i].is_ascii_digit() {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            denominator_end = Some(i);
            if i < bytes.len() {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
        }

        let numerator_digits = &bytes[numerator_start..numerator_end];
        if numerator_digits.len() >= 2 && numerator_digits[0] == b'0' {
            return Err(parse_err(
                numerator_start + 1,
                ParseKind::NonCanonical,
            ));
        }
        if negative && numerator_digits == b"0" {
            return Err(parse_err(0, ParseKind::NonCanonical));
        }

        if let (Some(start), Some(end)) = (denominator_start, denominator_end) {
            if end - start >= 2 && bytes[start] == b'0' {
                return Err(parse_err(start + 1, ParseKind::NonCanonical));
            }
        }

        let numerator_limit: u128 = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        let numerator_magnitude = parse_magnitude(numerator_digits, numerator_limit)
            .ok_or_else(|| parse_err(0, ParseKind::OutOfRange))?;

        let numerator = if negative {
            if numerator_magnitude == (1u128 << 63) {
                i64::MIN
            } else {
                -(numerator_magnitude as i64)
            }
        } else {
            numerator_magnitude as i64
        };

        let mut denominator = None;
        if let (Some(start), Some(end)) = (denominator_start, denominator_end) {
            let digits = &bytes[start..end];
            let magnitude = parse_magnitude(digits, i64::MAX as u128)
                .ok_or_else(|| parse_err(start, ParseKind::OutOfRange))?;
            denominator = Some(magnitude as i64);
        }

        if let Some(d) = denominator {
            let start = denominator_start.unwrap();
            let slash_at = slash.unwrap();
            if d == 0 {
                return Err(parse_err(start, ParseKind::ZeroDenominator));
            }
            if d == 1 {
                return Err(parse_err(slash_at, ParseKind::NonCanonical));
            }
            if numerator == 0 {
                return Err(parse_err(slash_at, ParseKind::NonCanonical));
            }
            if gcd_u128(numerator.unsigned_abs() as u128, d as u128) > 1 {
                return Err(parse_err(slash_at, ParseKind::NonCanonical));
            }
            Ok(Self {
                num: numerator,
                den: d,
            })
        } else {
            Ok(Self {
                num: numerator,
                den: 1,
            })
        }
    }
}

fn parse_magnitude(digits: &[u8], limit: u128) -> Option<u128> {
    let mut value = 0u128;
    for &b in digits {
        let digit = (b - b'0') as u128;
        value = value.checked_mul(10)?.checked_add(digit)?;
        if value > limit {
            return None;
        }
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
    pub fn new(
        rows: usize,
        cols: usize,
        data: Vec<Rational>,
    ) -> Result<Self, MatrixError> {
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

fn main() {}