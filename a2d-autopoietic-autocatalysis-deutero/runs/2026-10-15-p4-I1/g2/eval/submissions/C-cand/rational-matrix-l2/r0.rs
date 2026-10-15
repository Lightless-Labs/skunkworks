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

fn rational_from_parts(num: i128, den: i128) -> Result<Rational, RationalError> {
    if den == 0 {
        return Err(RationalError::ZeroDenominator);
    }
    if num == 0 {
        return Ok(Rational { num: 0, den: 1 });
    }

    let (mut n, mut d) = (num, den);
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
    Ok(Rational {
        num: n as i64,
        den: d as i64,
    })
}

impl Rational {
    pub fn new(num: i64, den: i64) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        rational_from_parts(num as i128, den as i128)
    }

    pub fn numerator(&self) -> i64 {
        self.num
    }

    pub fn denominator(&self) -> i64 {
        self.den
    }

    pub fn add(&self, rhs: &Self) -> Result<Self, RationalError> {
        let a = self.num as i128 * rhs.den as i128;
        let b = rhs.num as i128 * self.den as i128;
        let d = self.den as i128 * rhs.den as i128;
        rational_from_parts(a + b, d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let a = self.num as i128 * rhs.den as i128;
        let b = rhs.num as i128 * self.den as i128;
        let d = self.den as i128 * rhs.den as i128;
        rational_from_parts(a - b, d)
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_from_parts(
            self.num as i128 * rhs.num as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        rational_from_parts(
            self.num as i128 * rhs.den as i128,
            self.den as i128 * rhs.num as i128,
        )
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        rational_from_parts(-(self.num as i128), self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        let parse_error = |at, kind| RationalError::Parse { at, kind };

        if bytes.is_empty() {
            return Err(parse_error(0, ParseKind::Empty));
        }

        let mut slash = None;
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'/' {
                if slash.is_some() {
                    return Err(parse_error(i, ParseKind::InvalidByte));
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
            return Err(parse_error(i, ParseKind::InvalidByte));
        }

        let num_end = slash.unwrap_or(bytes.len());
        let num_start = usize::from(bytes.first() == Some(&b'-'));
        if num_start == num_end {
            return Err(parse_error(num_end, ParseKind::InvalidByte));
        }
        if let Some(s) = slash {
            if s + 1 == bytes.len() {
                return Err(parse_error(bytes.len(), ParseKind::InvalidByte));
            }
        }

        let den_start = slash.map(|s| s + 1);
        let num_digits = &bytes[num_start..num_end];
        if num_digits.len() > 1 && num_digits[0] == b'0' {
            return Err(parse_error(num_start + 1, ParseKind::NonCanonical));
        }
        if bytes.first() == Some(&b'-') && num_digits == b"0" {
            return Err(parse_error(0, ParseKind::NonCanonical));
        }
        if let Some(ds) = den_start {
            let den_digits = &bytes[ds..];
            if den_digits.len() > 1 && den_digits[0] == b'0' {
                return Err(parse_error(ds + 1, ParseKind::NonCanonical));
            }
        }

        let negative = bytes[0] == b'-';
        let limit: u128 = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        let mut nmag = 0u128;
        for &b in num_digits {
            nmag = nmag
                .saturating_mul(10)
                .saturating_add((b - b'0') as u128);
            if nmag > limit {
                return Err(parse_error(0, ParseKind::OutOfRange));
            }
        }

        let mut dmag = 1u128;
        if let Some(ds) = den_start {
            dmag = 0;
            for &b in &bytes[ds..] {
                dmag = dmag
                    .saturating_mul(10)
                    .saturating_add((b - b'0') as u128);
                if dmag > i64::MAX as u128 {
                    return Err(parse_error(ds, ParseKind::OutOfRange));
                }
            }
            if dmag == 0 {
                return Err(parse_error(ds, ParseKind::ZeroDenominator));
            }
        }

        if let Some(s) = slash {
            if dmag == 1 {
                return Err(parse_error(s, ParseKind::NonCanonical));
            }
            if nmag == 0 {
                return Err(parse_error(s, ParseKind::NonCanonical));
            }
            if gcd_u128(nmag, dmag) > 1 {
                return Err(parse_error(s, ParseKind::NonCanonical));
            }
        }

        let n = if negative {
            if nmag == (1u128 << 63) {
                i64::MIN
            } else {
                -(nmag as i64)
            }
        } else {
            nmag as i64
        };
        Ok(Rational {
            num: n,
            den: dmag as i64,
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
                    let left = self.get(r, k).map_err(|_| MatrixError::Index { row: r, col: k })?;
                    let right = rhs.get(k, c).map_err(|_| MatrixError::Index { row: k, col: c })?;
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

fn main() {}