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

        let (num, den) = if den < 0 {
            (
                num.checked_neg().ok_or(RationalError::Overflow)?,
                den.checked_neg().ok_or(RationalError::Overflow)?,
            )
        } else {
            (num, den)
        };

        if num == 0 {
            return Ok(Self { num: 0, den: 1 });
        }

        let divisor = gcd_u128(num.unsigned_abs(), den as u128);
        let reduced_num = num / divisor as i128;
        let reduced_den = den / divisor as i128;

        if reduced_num < i64::MIN as i128
            || reduced_num > i64::MAX as i128
            || reduced_den < 1
            || reduced_den > i64::MAX as i128
        {
            return Err(RationalError::Overflow);
        }

        Ok(Self {
            num: reduced_num as i64,
            den: reduced_den as i64,
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

        let common = gcd_u128(b as u128, d as u128) as i128;
        let left_scale = d / common;
        let right_scale = b / common;
        let numerator = a * left_scale + c * right_scale;
        let denominator = b * left_scale;
        Self::from_i128(numerator, denominator)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let a = self.num as i128;
        let b = self.den as i128;
        let c = rhs.num as i128;
        let d = rhs.den as i128;

        let common = gcd_u128(b as u128, d as u128) as i128;
        let left_scale = d / common;
        let right_scale = b / common;
        let numerator = a * left_scale - c * right_scale;
        let denominator = b * left_scale;
        Self::from_i128(numerator, denominator)
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        let mut a = self.num as i128;
        let mut b = self.den as i128;
        let mut c = rhs.num as i128;
        let mut d = rhs.den as i128;

        let g1 = gcd_u128(a.unsigned_abs(), d as u128) as i128;
        let g2 = gcd_u128(c.unsigned_abs(), b as u128) as i128;
        a /= g1;
        d /= g1;
        c /= g2;
        b /= g2;

        Self::from_i128(a * c, b * d)
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }

        let mut a = self.num as i128;
        let mut b = self.den as i128;
        let mut c = rhs.den as i128;
        let mut d = rhs.num as i128;

        if d < 0 {
            c = -c;
            d = -d;
        }

        let g1 = gcd_u128(a.unsigned_abs(), d as u128) as i128;
        let g2 = gcd_u128(c as u128, b as u128) as i128;
        a /= g1;
        d /= g1;
        c /= g2;
        b /= g2;

        Self::from_i128(a * c, b * d)
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        let num = self.num.checked_neg().ok_or(RationalError::Overflow)?;
        Ok(Self { num, den: self.den })
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(RationalError::Parse {
                at: 0,
                kind: ParseKind::Empty,
            });
        }

        let slash = bytes.iter().position(|&b| b == b'/');
        let numerator_end = slash.unwrap_or(bytes.len());

        let mut i = 0;
        if bytes[i] == b'-' {
            i += 1;
            if i == numerator_end {
                return Err(RationalError::Parse {
                    at: text.len(),
                    kind: ParseKind::InvalidByte,
                });
            }
        }

        while i < numerator_end {
            if !bytes[i].is_ascii_digit() {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
            i += 1;
        }

        let denominator_start = if let Some(pos) = slash {
            let start = pos + 1;
            if start == bytes.len() {
                return Err(RationalError::Parse {
                    at: text.len(),
                    kind: ParseKind::InvalidByte,
                });
            }
            for (offset, &byte) in bytes[start..].iter().enumerate() {
                if !byte.is_ascii_digit() {
                    return Err(RationalError::Parse {
                        at: start + offset,
                        kind: ParseKind::InvalidByte,
                    });
                }
            }
            Some(start)
        } else {
            None
        };

        let numerator_token = &bytes[..numerator_end];
        let negative = numerator_token.first() == Some(&b'-');
        let numerator_digits = if negative {
            &numerator_token[1..]
        } else {
            numerator_token
        };

        if numerator_digits.len() >= 2 && numerator_digits[0] == b'0' {
            return Err(RationalError::Parse {
                at: if negative { 2 } else { 1 },
                kind: ParseKind::NonCanonical,
            });
        }
        if negative && numerator_digits == b"0" {
            return Err(RationalError::Parse {
                at: 0,
                kind: ParseKind::NonCanonical,
            });
        }

        if let (Some(start), Some(end)) = (denominator_start, Some(bytes.len())) {
            let digits = &bytes[start..end];
            if digits.len() >= 2 && digits[0] == b'0' {
                return Err(RationalError::Parse {
                    at: start + 1,
                    kind: ParseKind::NonCanonical,
                });
            }
        }

        let numerator_magnitude = parse_bounded_magnitude(
            numerator_digits,
            if negative {
                1u128 << 63
            } else {
                i64::MAX as u128
            },
        )
        .ok_or(RationalError::Parse {
            at: 0,
            kind: ParseKind::OutOfRange,
        })?;

        let numerator = if negative {
            if numerator_magnitude == 1u128 << 63 {
                i64::MIN
            } else {
                -(numerator_magnitude as i64)
            }
        } else {
            numerator_magnitude as i64
        };

        let (denominator, denominator_start) = if let Some(start) = denominator_start {
            let magnitude = parse_bounded_magnitude(&bytes[start..], i64::MAX as u128)
                .ok_or(RationalError::Parse {
                    at: start,
                    kind: ParseKind::OutOfRange,
                })?;
            (magnitude as i64, Some(start))
        } else {
            (1, None)
        };

        if let Some(start) = denominator_start {
            if denominator == 0 {
                return Err(RationalError::Parse {
                    at: start,
                    kind: ParseKind::ZeroDenominator,
                });
            }
            let slash_at = start - 1;
            if denominator == 1 {
                return Err(RationalError::Parse {
                    at: slash_at,
                    kind: ParseKind::NonCanonical,
                });
            }
            if numerator == 0 {
                return Err(RationalError::Parse {
                    at: slash_at,
                    kind: ParseKind::NonCanonical,
                });
            }
            if gcd_u128(numerator.unsigned_abs() as u128, denominator as u128) > 1 {
                return Err(RationalError::Parse {
                    at: slash_at,
                    kind: ParseKind::NonCanonical,
                });
            }
        }

        Ok(Self {
            num: numerator,
            den: denominator,
        })
    }
}

fn parse_bounded_magnitude(digits: &[u8], limit: u128) -> Option<u128> {
    let mut value = 0u128;
    for &byte in digits {
        let digit = (byte - b'0') as u128;
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
        let mut output = Vec::with_capacity(output_len);

        for row in 0..self.rows {
            for col in 0..rhs.cols {
                let mut acc = Rational { num: 0, den: 1 };
                for step in 0..self.cols {
                    let left_index = row * self.cols + step;
                    let right_index = step * rhs.cols + col;
                    let product = self.data[left_index]
                        .mul(&rhs.data[right_index])
                        .map_err(|source| MatrixError::Arithmetic {
                            row,
                            col,
                            step,
                            source,
                        })?;
                    acc = acc.add(&product).map_err(|source| MatrixError::Arithmetic {
                        row,
                        col,
                        step,
                        source,
                    })?;
                }
                output.push(acc);
            }
        }

        Ok(Self {
            rows: self.rows,
            cols: rhs.cols,
            data: output,
        })
    }
}

fn main() {}