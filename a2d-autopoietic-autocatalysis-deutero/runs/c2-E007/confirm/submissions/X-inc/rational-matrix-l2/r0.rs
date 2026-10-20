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

fn abs_i64(n: i64) -> u128 {
    if n < 0 {
        (-(n as i128)) as u128
    } else {
        n as u128
    }
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
        if n < i64::MIN as i128 || n > i64::MAX as i128 || d > i64::MAX as i128 {
            return Err(RationalError::Overflow);
        }
        Ok(Self { num: n as i64, den: d as i64 })
    }

    pub fn numerator(&self) -> i64 {
        self.num
    }

    pub fn denominator(&self) -> i64 {
        self.den
    }

    pub fn add(&self, rhs: &Self) -> Result<Self, RationalError> {
        let g = gcd_u128(self.den as u128, rhs.den as u128) as i128;
        let a = self.den as i128 / g;
        let b = rhs.den as i128 / g;
        let n = (self.num as i128) * b + (rhs.num as i128) * a;
        let d = a * (rhs.den as i128);
        Self::from_i128(n, d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let g = gcd_u128(self.den as u128, rhs.den as u128) as i128;
        let a = self.den as i128 / g;
        let b = rhs.den as i128 / g;
        let n = (self.num as i128) * b - (rhs.num as i128) * a;
        let d = a * (rhs.den as i128);
        Self::from_i128(n, d)
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        let g1 = gcd_u128(abs_i64(self.num), rhs.den as u128) as i128;
        let g2 = gcd_u128(abs_i64(rhs.num), self.den as u128) as i128;
        let n = (self.num as i128 / g1) * (rhs.num as i128 / g2);
        let d = (self.den as i128 / g2) * (rhs.den as i128 / g1);
        Self::from_i128(n, d)
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        let g1 = gcd_u128(abs_i64(self.num), abs_i64(rhs.num)) as i128;
        let g2 = gcd_u128(self.den as u128, rhs.den as u128) as i128;
        let mut n = (self.num as i128 / g1) * (rhs.den as i128 / g2);
        let mut d = (self.den as i128 / g2) * (rhs.num as i128 / g1);
        if d < 0 {
            n = -n;
            d = -d;
        }
        Self::from_i128(n, d)
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        Self::from_i128(-(self.num as i128), self.den as i128)
    }

    fn from_i128(mut n: i128, mut d: i128) -> Result<Self, RationalError> {
        if d == 0 {
            return Err(RationalError::ZeroDenominator);
        }
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
        if n < i64::MIN as i128 || n > i64::MAX as i128 || d > i64::MAX as i128 {
            return Err(RationalError::Overflow);
        }
        Ok(Self { num: n as i64, den: d as i64 })
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        let fail = |at, kind| RationalError::Parse { at, kind };
        if bytes.is_empty() {
            return Err(fail(0, ParseKind::Empty));
        }

        let mut i = 0;
        let negative = bytes[0] == b'-';
        if negative {
            i = 1;
        }
        let num_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == num_start {
            return Err(fail(i, ParseKind::InvalidByte));
        }
        let num_end = i;
        let mut den_start = None;
        let mut den_end = None;
        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(fail(i, ParseKind::InvalidByte));
            }
            i += 1;
            let start = i;
            den_start = Some(start);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return Err(fail(i, ParseKind::InvalidByte));
            }
            den_end = Some(i);
            if i < bytes.len() {
                return Err(fail(i, ParseKind::InvalidByte));
            }
        }

        let num_digits = &bytes[num_start..num_end];
        if num_digits.len() >= 2 && num_digits[0] == b'0' {
            return Err(fail(if negative { 2 } else { 1 }, ParseKind::NonCanonical));
        }
        if negative && num_digits == b"0" {
            return Err(fail(0, ParseKind::NonCanonical));
        }
        if let (Some(s), Some(e)) = (den_start, den_end) {
            if e - s >= 2 && bytes[s] == b'0' {
                return Err(fail(s + 1, ParseKind::NonCanonical));
            }
        }

        let mut num_mag: u128 = 0;
        for &b in num_digits {
            num_mag = num_mag * 10 + (b - b'0') as u128;
            let limit = if negative { 1u128 << 63 } else { i64::MAX as u128 };
            if num_mag > limit {
                return Err(fail(0, ParseKind::OutOfRange));
            }
        }
        let num = if negative {
            if num_mag == 1u128 << 63 { i64::MIN } else { -(num_mag as i64) }
        } else {
            num_mag as i64
        };

        let mut den = 1i64;
        if let (Some(s), Some(e)) = (den_start, den_end) {
            let mut mag: u128 = 0;
            for &b in &bytes[s..e] {
                mag = mag * 10 + (b - b'0') as u128;
                if mag > i64::MAX as u128 {
                    return Err(fail(s, ParseKind::OutOfRange));
                }
            }
            if mag == 0 {
                return Err(fail(s, ParseKind::ZeroDenominator));
            }
            den = mag as i64;
            let slash = s - 1;
            if den == 1 {
                return Err(fail(slash, ParseKind::NonCanonical));
            }
            if num == 0 {
                return Err(fail(slash, ParseKind::NonCanonical));
            }
            if gcd_u128(abs_i64(num), den as u128) > 1 {
                return Err(fail(slash, ParseKind::NonCanonical));
            }
        }

        Ok(Self { num, den })
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
    DataLength { expected: usize, actual: usize },
    Index { row: usize, col: usize },
    DimensionMismatch { left: (usize, usize), right: (usize, usize) },
    Arithmetic { row: usize, col: usize, step: usize, source: RationalError },
    NotSquare { rows: usize, cols: usize },
    Singular { rank: usize },
}

impl Matrix {
    pub fn new(rows: usize, cols: usize, data: Vec<Rational>) -> Result<Self, MatrixError> {
        let expected = rows.checked_mul(cols).ok_or(MatrixError::ShapeOverflow)?;
        if data.len() != expected {
            return Err(MatrixError::DataLength { expected, actual: data.len() });
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
        let index = row.checked_mul(self.cols)
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
        Self { rows: self.cols, cols: self.rows, data }
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, MatrixError> {
        if self.cols != rhs.rows {
            return Err(MatrixError::DimensionMismatch {
                left: (self.rows, self.cols),
                right: (rhs.rows, rhs.cols),
            });
        }
        let len = self.rows.checked_mul(rhs.cols).ok_or(MatrixError::ShapeOverflow)?;
        let mut data = Vec::with_capacity(len);
        for r in 0..self.rows {
            for c in 0..rhs.cols {
                let mut acc = Rational { num: 0, den: 1 };
                for k in 0..self.cols {
                    let left = self.get(r, k).map_err(|_| MatrixError::Index { row: r, col: k })?;
                    let right = rhs.get(k, c).map_err(|_| MatrixError::Index { row: k, col: c })?;
                    let term = left.mul(&right).map_err(|source| MatrixError::Arithmetic {
                        row: r, col: c, step: k, source,
                    })?;
                    acc = acc.add(&term).map_err(|source| MatrixError::Arithmetic {
                        row: r, col: c, step: k, source,
                    })?;
                }
                data.push(acc);
            }
        }
        Ok(Self { rows: self.rows, cols: rhs.cols, data })
    }
}