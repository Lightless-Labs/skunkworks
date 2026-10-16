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
        let b = rhs.num as i128;
        let d1 = self.den as i128;
        let d2 = rhs.den as i128;
        let common = gcd_u128(d1 as u128, d2 as u128) as i128;
        let left_scale = d2 / common;
        let right_scale = d1 / common;
        let num = a * left_scale + b * right_scale;
        let den = d1 * left_scale;
        Self::from_i128(num, den)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let a = self.num as i128;
        let b = rhs.num as i128;
        let d1 = self.den as i128;
        let d2 = rhs.den as i128;
        let common = gcd_u128(d1 as u128, d2 as u128) as i128;
        let left_scale = d2 / common;
        let right_scale = d1 / common;
        let num = a * left_scale - b * right_scale;
        let den = d1 * left_scale;
        Self::from_i128(num, den)
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        let a = self.num as i128;
        let b = rhs.num as i128;
        let d1 = self.den as i128;
        let d2 = rhs.den as i128;

        let g1 = gcd_u128(a.unsigned_abs(), d2 as u128) as i128;
        let g2 = gcd_u128(b.unsigned_abs(), d1 as u128) as i128;
        let num = (a / g1) * (b / g2);
        let den = (d1 / g2) * (d2 / g1);
        Self::from_i128(num, den)
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }

        let a = self.num as i128;
        let b = rhs.num as i128;
        let d1 = self.den as i128;
        let d2 = rhs.den as i128;

        let g1 = gcd_u128(a.unsigned_abs(), b.unsigned_abs()) as i128;
        let g2 = gcd_u128(d1 as u128, d2 as u128) as i128;
        let mut num = (a / g1) * (d2 / g2);
        let den = (d1 / g2) * (b / g1);
        if den < 0 {
            num = -num;
            Self::from_i128(num, -den)
        } else {
            Self::from_i128(num, den)
        }
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        Self::from_i128(-(self.num as i128), self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        let parse_error = |at, kind| RationalError::Parse { at, kind };

        if bytes.is_empty() {
            return Err(parse_error(0, ParseKind::Empty));
        }

        let mut i = 0;
        if bytes[0] == b'-' {
            i = 1;
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

        let negative = bytes[0] == b'-';
        let num_digits_start = if negative { 1 } else { 0 };

        if num_end - num_digits_start >= 2 && bytes[num_digits_start] == b'0' {
            return Err(parse_error(num_digits_start + 1, ParseKind::NonCanonical));
        }
        if negative && num_end == 2 && bytes[1] == b'0' {
            return Err(parse_error(0, ParseKind::NonCanonical));
        }

        if let (Some(start), Some(end)) = (den_start, den_end) {
            if end - start >= 2 && bytes[start] == b'0' {
                return Err(parse_error(start + 1, ParseKind::NonCanonical));
            }
        }

        let mut magnitude: u128 = 0;
        for &byte in &bytes[num_digits_start..num_end] {
            magnitude = magnitude * 10 + (byte - b'0') as u128;
            let limit = if negative {
                1u128 << 63
            } else {
                i64::MAX as u128
            };
            if magnitude > limit {
                return Err(parse_error(0, ParseKind::OutOfRange));
            }
        }

        let numerator = if negative {
            if magnitude == (1u128 << 63) {
                i64::MIN
            } else {
                -(magnitude as i64)
            }
        } else {
            magnitude as i64
        };

        let denominator = if let (Some(start), Some(end)) = (den_start, den_end) {
            let mut value: u128 = 0;
            for &byte in &bytes[start..end] {
                value = value * 10 + (byte - b'0') as u128;
                if value > i64::MAX as u128 {
                    return Err(parse_error(start, ParseKind::OutOfRange));
                }
            }
            value as i64
        } else {
            1
        };

        if den_start.is_some() && denominator == 0 {
            return Err(parse_error(den_start.unwrap(), ParseKind::ZeroDenominator));
        }

        if let Some(start) = den_start {
            if denominator == 1 {
                return Err(parse_error(start - 1, ParseKind::NonCanonical));
            }
            if numerator == 0 {
                return Err(parse_error(start - 1, ParseKind::NonCanonical));
            }
            if gcd_u128(numerator.unsigned_abs() as u128, denominator as u128) > 1 {
                return Err(parse_error(start - 1, ParseKind::NonCanonical));
            }
        }

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