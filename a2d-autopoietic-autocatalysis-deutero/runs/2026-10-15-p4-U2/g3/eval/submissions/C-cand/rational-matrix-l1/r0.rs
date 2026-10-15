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

fn gcd(mut a: u128, mut b: u128) -> u128 {
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

        let (mut n, mut d) = (num, den);
        if d < 0 {
            n = -n;
            d = -d;
        }
        if n == 0 {
            return Ok(Self { num: 0, den: 1 });
        }

        let g = gcd(n.unsigned_abs(), d as u128) as i128;
        n /= g;
        d /= g;

        let num = i64::try_from(n).map_err(|_| RationalError::Overflow)?;
        let den = i64::try_from(d).map_err(|_| RationalError::Overflow)?;
        Ok(Self { num, den })
    }

    pub fn new(num: i64, den: i64) -> Result<Self, RationalError> {
        Self::from_i128(num as i128, den as i128)
    }

    pub fn numerator(&self) -> i64 {
        self.num
    }

    pub fn denominator(&self) -> i64 {
        self.den
    }

    pub fn add(&self, rhs: &Self) -> Result<Self, RationalError> {
        let g = gcd(self.den as u128, rhs.den as u128) as i128;
        let a = self.den as i128 / g;
        let b = rhs.den as i128 / g;
        Self::from_i128(
            self.num as i128 * b + rhs.num as i128 * a,
            a * rhs.den as i128,
        )
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let g = gcd(self.den as u128, rhs.den as u128) as i128;
        let a = self.den as i128 / g;
        let b = rhs.den as i128 / g;
        Self::from_i128(
            self.num as i128 * b - rhs.num as i128 * a,
            a * rhs.den as i128,
        )
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        let g1 = gcd(self.num.unsigned_abs() as u128, rhs.den as u128) as i128;
        let g2 = gcd(rhs.num.unsigned_abs() as u128, self.den as u128) as i128;
        Self::from_i128(
            (self.num as i128 / g1) * (rhs.num as i128 / g2),
            (self.den as i128 / g2) * (rhs.den as i128 / g1),
        )
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        let g1 = gcd(self.num.unsigned_abs() as u128, rhs.num.unsigned_abs() as u128) as i128;
        let g2 = gcd(self.den as u128, rhs.den as u128) as i128;
        Self::from_i128(
            (self.num as i128 / g1) * (rhs.den as i128 / g2),
            (self.den as i128 / g2) * (rhs.num as i128 / g1),
        )
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        let num = self.num.checked_neg().ok_or(RationalError::Overflow)?;
        Ok(Self { num, den: self.den })
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
            i += 1;
        }

        let num_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == num_start {
            return Err(parse_err(i, ParseKind::InvalidByte));
        }

        let mut slash = None;
        let mut den_start = None;
        let mut den_end = None;

        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }

            slash = Some(i);
            i += 1;
            den_start = Some(i);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }

            if i == den_start.unwrap() {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }

            den_end = Some(i);
            if i < bytes.len() {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
        }

        let num_end = slash.unwrap_or(bytes.len());
        let num_digits = &bytes[num_start..num_end];
        let den_digits = den_start.map(|start| &bytes[start..den_end.unwrap()]);

        if num_digits.len() >= 2 && num_digits[0] == b'0' {
            return Err(parse_err(if negative { 2 } else { 1 }, ParseKind::NonCanonical));
        }
        if negative && num_digits == b"0" {
            return Err(parse_err(0, ParseKind::NonCanonical));
        }
        if let Some(digits) = den_digits {
            if digits.len() >= 2 && digits[0] == b'0' {
                return Err(parse_err(den_start.unwrap() + 1, ParseKind::NonCanonical));
            }
        }

        let limit = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        let mut magnitude = 0u128;
        for &byte in num_digits {
            magnitude = magnitude
                .saturating_mul(10)
                .saturating_add((byte - b'0') as u128);
            if magnitude > limit {
                return Err(parse_err(0, ParseKind::OutOfRange));
            }
        }

        let num = if negative {
            if magnitude == (1u128 << 63) {
                i64::MIN
            } else {
                -(magnitude as i64)
            }
        } else {
            magnitude as i64
        };

        let Some(digits) = den_digits else {
            return Self::new(num, 1);
        };

        let mut den_magnitude = 0u128;
        for &byte in digits {
            den_magnitude = den_magnitude
                .saturating_mul(10)
                .saturating_add((byte - b'0') as u128);
            if den_magnitude > i64::MAX as u128 {
                return Err(parse_err(den_start.unwrap(), ParseKind::OutOfRange));
            }
        }

        let den = den_magnitude as i64;
        if den == 0 {
            return Err(parse_err(den_start.unwrap(), ParseKind::ZeroDenominator));
        }
        if den == 1 {
            return Err(parse_err(slash.unwrap(), ParseKind::NonCanonical));
        }
        if num == 0 {
            return Err(parse_err(slash.unwrap(), ParseKind::NonCanonical));
        }
        if gcd(num.unsigned_abs() as u128, den as u128) > 1 {
            return Err(parse_err(slash.unwrap(), ParseKind::NonCanonical));
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

fn main() {}