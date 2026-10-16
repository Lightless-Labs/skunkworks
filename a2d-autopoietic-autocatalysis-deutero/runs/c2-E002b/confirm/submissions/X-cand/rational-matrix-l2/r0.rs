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

        let g = gcd_u128(n.unsigned_abs(), d as u128) as i128;
        n /= g;
        d /= g;

        if n < i64::MIN as i128 || n > i64::MAX as i128 || d < 1 || d > i64::MAX as i128 {
            return Err(RationalError::Overflow);
        }
        Ok(Self { num: n as i64, den: d as i64 })
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
        let b = rhs.num as i128;
        let ad = self.den as i128;
        let bd = rhs.den as i128;
        let g = gcd_u128(ad as u128, bd as u128) as i128;
        let left_scale = bd / g;
        let right_scale = ad / g;
        let n = a * left_scale + b * right_scale;
        let d = ad * left_scale;
        Self::from_i128(n, d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let a = self.num as i128;
        let b = rhs.num as i128;
        let ad = self.den as i128;
        let bd = rhs.den as i128;
        let g = gcd_u128(ad as u128, bd as u128) as i128;
        let left_scale = bd / g;
        let right_scale = ad / g;
        let n = a * left_scale - b * right_scale;
        let d = ad * left_scale;
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
        match self.num.checked_neg() {
            Some(n) => Ok(Self { num: n, den: self.den }),
            None => Err(RationalError::Overflow),
        }
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        let parse_err = |at, kind| RationalError::Parse { at, kind };

        if bytes.is_empty() {
            return Err(parse_err(0, ParseKind::Empty));
        }

        let mut i = 0;
        let negative = bytes[0] == b'-';
        if negative {
            i = 1;
        }

        let numerator_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == numerator_start {
            return Err(parse_err(i, ParseKind::InvalidByte));
        }

        let numerator_end = i;
        let mut denominator_start = None;
        let mut denominator_end = None;

        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
            i += 1;
            let start = i;
            denominator_start = Some(start);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
            denominator_end = Some(i);
            if i < bytes.len() {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
        }

        let num_digits_start = if negative { 1 } else { 0 };
        let num_digits = numerator_end - num_digits_start;
        if num_digits >= 2 && bytes[num_digits_start] == b'0' {
            return Err(parse_err(num_digits_start + 1, ParseKind::NonCanonical));
        }
        if negative && num_digits == 1 && bytes[1] == b'0' {
            return Err(parse_err(0, ParseKind::NonCanonical));
        }

        if let (Some(start), Some(end)) = (denominator_start, denominator_end) {
            if end - start >= 2 && bytes[start] == b'0' {
                return Err(parse_err(start + 1, ParseKind::NonCanonical));
            }
        }

        let mut magnitude: u128 = 0;
        for &byte in &bytes[num_digits_start..numerator_end] {
            magnitude = magnitude * 10 + (byte - b'0') as u128;
            let limit = if negative { 1u128 << 63 } else { i64::MAX as u128 };
            if magnitude > limit {
                return Err(parse_err(0, ParseKind::OutOfRange));
            }
        }

        let denominator = if let (Some(start), Some(end)) = (denominator_start, denominator_end) {
            let mut value: u128 = 0;
            for &byte in &bytes[start..end] {
                value = value * 10 + (byte - b'0') as u128;
                if value > i64::MAX as u128 {
                    return Err(parse_err(start, ParseKind::OutOfRange));
                }
            }
            value as u64
        } else {
            1
        };

        if denominator_start.is_some() && denominator == 0 {
            return Err(parse_err(denominator_start.unwrap(), ParseKind::ZeroDenominator));
        }

        if denominator_start.is_some() {
            let slash = numerator_end;
            if denominator == 1 {
                return Err(parse_err(slash, ParseKind::NonCanonical));
            }
            if magnitude == 0 {
                return Err(parse_err(slash, ParseKind::NonCanonical));
            }
            if gcd_u128(magnitude, denominator as u128) > 1 {
                return Err(parse_err(slash, ParseKind::NonCanonical));
            }
        }

        let signed_num = if negative {
            if magnitude == 1u128 << 63 { i64::MIN } else { -(magnitude as i64) }
        } else {
            magnitude as i64
        };
        Ok(Self { num: signed_num, den: denominator as i64 })
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
        for col in 0..self.cols {
            for row in 0..self.rows {
                data.push(self.data[row * self.cols + col]);
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

        let output_len = self.rows.checked_mul(rhs.cols).ok_or(MatrixError::ShapeOverflow)?;
        let mut data = Vec::with_capacity(output_len);

        if output_len == 0 {
            return Ok(Self { rows: self.rows, cols: rhs.cols, data });
        }

        for row in 0..self.rows {
            for col in 0..rhs.cols {
                let mut acc = Rational { num: 0, den: 1 };
                for step in 0..self.cols {
                    let left = self.data[row * self.cols + step];
                    let right = rhs.data[step * rhs.cols + col];
                    let term = left.mul(&right).map_err(|source| MatrixError::Arithmetic {
                        row, col, step, source,
                    })?;
                    acc = acc.add(&term).map_err(|source| MatrixError::Arithmetic {
                        row, col, step, source,
                    })?;
                }
                data.push(acc);
            }
        }

        Ok(Self { rows: self.rows, cols: rhs.cols, data })
    }
}