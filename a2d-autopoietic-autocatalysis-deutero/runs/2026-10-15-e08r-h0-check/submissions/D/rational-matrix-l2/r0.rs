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

fn rational_from_i128(num: i128, den: i128) -> Result<Rational, RationalError> {
    if den == 0 {
        return Err(RationalError::ZeroDenominator);
    }

    let (mut n, mut d) = (num, den);
    if d < 0 {
        n = -n;
        d = -d;
    }
    if n == 0 {
        return Ok(Rational { num: 0, den: 1 });
    }

    let g = gcd_u128(n.unsigned_abs(), d as u128) as i128;
    n /= g;
    d /= g;

    let num = i64::try_from(n).map_err(|_| RationalError::Overflow)?;
    let den = i64::try_from(d).map_err(|_| RationalError::Overflow)?;
    Ok(Rational { num, den })
}

impl Rational {
    pub fn new(num: i64, den: i64) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }

        let mut n = num as i128;
        let mut d = den as i128;
        if d < 0 {
            n = -n;
            d = -d;
        }
        if n == 0 {
            return Ok(Self { num: 0, den: 1 });
        }

        let g = gcd_u128(n.unsigned_abs(), d as u128) as i128;
        n /= g;
        d /= g;

        let num = i64::try_from(n).map_err(|_| RationalError::Overflow)?;
        let den = i64::try_from(d).map_err(|_| RationalError::Overflow)?;
        Ok(Self { num, den })
    }

    pub fn numerator(&self) -> i64 {
        self.num
    }

    pub fn denominator(&self) -> i64 {
        self.den
    }

    pub fn add(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_from_i128(
            self.num as i128 * rhs.den as i128 + rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_from_i128(
            self.num as i128 * rhs.den as i128 - rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_from_i128(
            self.num as i128 * rhs.num as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        rational_from_i128(
            self.num as i128 * rhs.den as i128,
            self.den as i128 * rhs.num as i128,
        )
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        let n = -(self.num as i128);
        i64::try_from(n)
            .map(|num| Self { num, den: self.den })
            .map_err(|_| RationalError::Overflow)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        let parse_error = |at, kind| RationalError::Parse { at, kind };

        if bytes.is_empty() {
            return Err(parse_error(0, ParseKind::Empty));
        }

        let mut i = 0;
        let negative = bytes[0] == b'-';
        if negative {
            i += 1;
        }

        let numerator_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == numerator_start {
            return Err(parse_error(i, ParseKind::InvalidByte));
        }

        let numerator_end = i;
        let mut denominator_start = None;
        let mut denominator_end = None;
        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(parse_error(i, ParseKind::InvalidByte));
            }
            i += 1;
            let start = i;
            denominator_start = Some(start);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return Err(parse_error(i, ParseKind::InvalidByte));
            }
            denominator_end = Some(i);
            if i < bytes.len() {
                return Err(parse_error(i, ParseKind::InvalidByte));
            }
        }

        let slash = if denominator_start.is_some() {
            Some(numerator_end)
        } else {
            None
        };

        let numerator_digits = &bytes[numerator_start..numerator_end];
        if numerator_digits.len() >= 2 && numerator_digits[0] == b'0' {
            return Err(parse_error(
                numerator_start + 1,
                ParseKind::NonCanonical,
            ));
        }
        if negative && numerator_digits == b"0" {
            return Err(parse_error(0, ParseKind::NonCanonical));
        }

        if let (Some(start), Some(end)) = (denominator_start, denominator_end) {
            let digits = &bytes[start..end];
            if digits.len() >= 2 && digits[0] == b'0' {
                return Err(parse_error(start + 1, ParseKind::NonCanonical));
            }
        }

        let numerator_limit: u128 = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        let mut numerator_magnitude = 0u128;
        for &digit in numerator_digits {
            numerator_magnitude = numerator_magnitude * 10 + (digit - b'0') as u128;
            if numerator_magnitude > numerator_limit {
                return Err(parse_error(0, ParseKind::OutOfRange));
            }
        }

        let denominator_value = if let (Some(start), Some(end)) =
            (denominator_start, denominator_end)
        {
            let mut value = 0u128;
            for &digit in &bytes[start..end] {
                value = value * 10 + (digit - b'0') as u128;
                if value > i64::MAX as u128 {
                    return Err(parse_error(start, ParseKind::OutOfRange));
                }
            }
            Some(value as i64)
        } else {
            None
        };

        if let Some(den) = denominator_value {
            if den == 0 {
                return Err(parse_error(
                    denominator_start.unwrap(),
                    ParseKind::ZeroDenominator,
                ));
            }
        }

        if let (Some(den), Some(slash_at)) = (denominator_value, slash) {
            if den == 1 || numerator_magnitude == 0 {
                return Err(parse_error(slash_at, ParseKind::NonCanonical));
            }
            if gcd_u128(numerator_magnitude, den as u128) > 1 {
                return Err(parse_error(slash_at, ParseKind::NonCanonical));
            }
        }

        let signed_num = if negative {
            if numerator_magnitude == (1u128 << 63) {
                i64::MIN
            } else {
                -(numerator_magnitude as i64)
            }
        } else {
            numerator_magnitude as i64
        };

        Ok(Self {
            num: signed_num,
            den: denominator_value.unwrap_or(1),
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
        for row in 0..self.cols {
            for col in 0..self.rows {
                data.push(self.data[col * self.cols + row]);
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

        if self.rows == 0 || rhs.cols == 0 {
            return Ok(Self {
                rows: self.rows,
                cols: rhs.cols,
                data,
            });
        }

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