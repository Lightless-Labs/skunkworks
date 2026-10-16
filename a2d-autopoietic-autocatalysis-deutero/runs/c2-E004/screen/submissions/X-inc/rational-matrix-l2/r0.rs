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
            (-num, -den)
        } else {
            (num, den)
        };
        if num == 0 {
            return Ok(Self { num: 0, den: 1 });
        }
        let g = gcd_u128(num.unsigned_abs(), den as u128) as i128;
        let n = num / g;
        let d = den / g;
        if n < i64::MIN as i128 || n > i64::MAX as i128 || d < 1 || d > i64::MAX as i128 {
            return Err(RationalError::Overflow);
        }
        Ok(Self {
            num: n as i64,
            den: d as i64,
        })
    }

    fn from_i128_pair(num: i128, den: i128) -> Result<Self, RationalError> {
        Self::from_i128(num, den)
    }

    pub fn new(num: i64, den: i64) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        let n = num as i128;
        let d = den as i128;
        Self::from_i128_pair(n, d)
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
        let mut a = self.num as i128;
        let mut b = rhs.num as i128;
        let mut c = self.den as i128;
        let mut d = rhs.den as i128;

        let g1 = gcd_u128(a.unsigned_abs(), d as u128) as i128;
        let g2 = gcd_u128(b.unsigned_abs(), c as u128) as i128;
        a /= g1;
        d /= g1;
        b /= g2;
        c /= g2;

        Self::from_i128(a * b, c * d)
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        let mut a = self.num as i128;
        let mut b = rhs.den as i128;
        let mut c = self.den as i128;
        let mut d = rhs.num as i128;

        let g1 = gcd_u128(a.unsigned_abs(), d.unsigned_abs()) as i128;
        let g2 = gcd_u128(b as u128, c as u128) as i128;
        a /= g1;
        d /= g1;
        b /= g2;
        c /= g2;

        Self::from_i128(a * b, c * d)
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        let n = -(self.num as i128);
        Self::from_i128(n, self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(RationalError::Parse {
                at: 0,
                kind: ParseKind::Empty,
            });
        }

        let mut slash = None;
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'/' {
                if slash.is_some() {
                    return Err(RationalError::Parse {
                        at: i,
                        kind: ParseKind::InvalidByte,
                    });
                }
                slash = Some(i);
            }
        }

        let num_end = slash.unwrap_or(bytes.len());
        let neg = bytes[0] == b'-';
        let digit_start = usize::from(neg);

        if digit_start == num_end {
            return Err(RationalError::Parse {
                at: num_end,
                kind: ParseKind::InvalidByte,
            });
        }

        for i in 0..num_end {
            if i == 0 && neg {
                continue;
            }
            if !bytes[i].is_ascii_digit() {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
        }

        if let Some(s) = slash {
            if s + 1 == bytes.len() {
                return Err(RationalError::Parse {
                    at: bytes.len(),
                    kind: ParseKind::InvalidByte,
                });
            }
            for i in s + 1..bytes.len() {
                if !bytes[i].is_ascii_digit() {
                    return Err(RationalError::Parse {
                        at: i,
                        kind: ParseKind::InvalidByte,
                    });
                }
            }
        }

        let num_digits = &bytes[digit_start..num_end];
        let den_digits = slash.map(|s| &bytes[s + 1..]);

        if num_digits.len() >= 2 && num_digits[0] == b'0' {
            return Err(RationalError::Parse {
                at: digit_start + 1,
                kind: ParseKind::NonCanonical,
            });
        }
        if neg && num_digits == b"0" {
            return Err(RationalError::Parse {
                at: 0,
                kind: ParseKind::NonCanonical,
            });
        }
        if let Some(digits) = den_digits {
            if digits.len() >= 2 && digits[0] == b'0' {
                return Err(RationalError::Parse {
                    at: num_end + 2,
                    kind: ParseKind::NonCanonical,
                });
            }
        }

        let num_limit: u128 = if neg {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        let num_mag = parse_magnitude(num_digits, num_limit).ok_or(RationalError::Parse {
            at: 0,
            kind: ParseKind::OutOfRange,
        })?;

        let den_mag = if let Some(digits) = den_digits {
            Some(parse_magnitude(digits, i64::MAX as u128).ok_or(
                RationalError::Parse {
                    at: num_end + 1,
                    kind: ParseKind::OutOfRange,
                },
            )?)
        } else {
            None
        };

        if let Some(d) = den_mag {
            if d == 0 {
                return Err(RationalError::Parse {
                    at: num_end + 1,
                    kind: ParseKind::ZeroDenominator,
                });
            }
            if d == 1 {
                return Err(RationalError::Parse {
                    at: num_end,
                    kind: ParseKind::NonCanonical,
                });
            }
            if num_mag == 0 {
                return Err(RationalError::Parse {
                    at: num_end,
                    kind: ParseKind::NonCanonical,
                });
            }
            if gcd_u128(num_mag, d) > 1 {
                return Err(RationalError::Parse {
                    at: num_end,
                    kind: ParseKind::NonCanonical,
                });
            }
        }

        let n = if neg {
            if num_mag == 1u128 << 63 {
                i64::MIN
            } else {
                -(num_mag as i64)
            }
        } else {
            num_mag as i64
        };
        let d = den_mag.unwrap_or(1) as i64;
        Ok(Self { num: n, den: d })
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
        let index = row
            .checked_mul(self.cols)
            .and_then(|base| base.checked_add(col))
            .ok_or(MatrixError::Index { row, col })?;
        self.data.get(index).copied().ok_or(MatrixError::Index { row, col })
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

        if self.rows == 0 || rhs.cols == 0 {
            return Ok(Self {
                rows: self.rows,
                cols: rhs.cols,
                data,
            });
        }

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