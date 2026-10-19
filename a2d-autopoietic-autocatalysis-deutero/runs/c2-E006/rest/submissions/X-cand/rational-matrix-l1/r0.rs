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

impl Rational {
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
        let n = self.num as i128 * rhs.den as i128
            + rhs.num as i128 * self.den as i128;
        let d = self.den as i128 * rhs.den as i128;
        Self::from_i128(n, d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let n = self.num as i128 * rhs.den as i128
            - rhs.num as i128 * self.den as i128;
        let d = self.den as i128 * rhs.den as i128;
        Self::from_i128(n, d)
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        let n = self.num as i128 * rhs.num as i128;
        let d = self.den as i128 * rhs.den as i128;
        Self::from_i128(n, d)
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        let n = self.num as i128 * rhs.den as i128;
        let d = self.den as i128 * rhs.num as i128;
        Self::from_i128(n, d)
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        Self::from_i128(-(self.num as i128), self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(parse_error(0, ParseKind::Empty));
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
            den_start = Some(i);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == den_start.unwrap() {
                return Err(parse_error(i, ParseKind::InvalidByte));
            }
            den_end = Some(i);
            if i < bytes.len() {
                return Err(parse_error(i, ParseKind::InvalidByte));
            }
        }

        let negative = bytes[0] == b'-';
        let num_digits = &bytes[num_start..num_end];
        if num_digits.len() >= 2 && num_digits[0] == b'0' {
            return Err(parse_error(if negative { 2 } else { 1 }, ParseKind::NonCanonical));
        }
        if negative && num_digits == b"0" {
            return Err(parse_error(0, ParseKind::NonCanonical));
        }

        if let (Some(start), Some(end)) = (den_start, den_end) {
            let digits = &bytes[start..end];
            if digits.len() >= 2 && digits[0] == b'0' {
                return Err(parse_error(start + 1, ParseKind::NonCanonical));
            }
        }

        let num_mag = parse_magnitude(num_digits, if negative { 1u128 << 63 } else { i64::MAX as u128 })
            .ok_or_else(|| parse_error(0, ParseKind::OutOfRange))?;
        let num = if negative {
            if num_mag == (1u128 << 63) {
                i64::MIN
            } else {
                -(num_mag as i64)
            }
        } else {
            num_mag as i64
        };

        let Some(start) = den_start else {
            return Ok(Self { num, den: 1 });
        };
        let end = den_end.unwrap();
        let den_mag = parse_magnitude(&bytes[start..end], i64::MAX as u128)
            .ok_or_else(|| parse_error(start, ParseKind::OutOfRange))?;
        let den = den_mag as i64;

        if den == 0 {
            return Err(parse_error(start, ParseKind::ZeroDenominator));
        }
        let slash = start - 1;
        if den == 1 {
            return Err(parse_error(slash, ParseKind::NonCanonical));
        }
        if num == 0 {
            return Err(parse_error(slash, ParseKind::NonCanonical));
        }
        if gcd_u128(num.unsigned_abs() as u128, den as u128) > 1 {
            return Err(parse_error(slash, ParseKind::NonCanonical));
        }

        Ok(Self { num, den })
    }

    fn from_i128(mut num: i128, mut den: i128) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        if num == 0 {
            return Ok(Self { num: 0, den: 1 });
        }
        if den < 0 {
            num = -num;
            den = -den;
        }

        let g = gcd_u128(num.unsigned_abs(), den as u128) as i128;
        num /= g;
        den /= g;

        if num < i64::MIN as i128
            || num > i64::MAX as i128
            || den < 1
            || den > i64::MAX as i128
        {
            return Err(RationalError::Overflow);
        }
        Ok(Self {
            num: num as i64,
            den: den as i64,
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

fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

fn parse_magnitude(bytes: &[u8], limit: u128) -> Option<u128> {
    let mut value = 0u128;
    for &b in bytes {
        let digit = (b - b'0') as u128;
        value = value.checked_mul(10)?.checked_add(digit)?;
        if value > limit {
            return None;
        }
    }
    Some(value)
}

fn parse_error(at: usize, kind: ParseKind) -> RationalError {
    RationalError::Parse { at, kind }
}