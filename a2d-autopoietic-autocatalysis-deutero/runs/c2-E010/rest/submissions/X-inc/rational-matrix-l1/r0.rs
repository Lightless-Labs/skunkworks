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
        Self::from_wide(num as i128, den as i128)
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
        Self::from_wide(n, d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let n = self.num as i128 * rhs.den as i128
            - rhs.num as i128 * self.den as i128;
        let d = self.den as i128 * rhs.den as i128;
        Self::from_wide(n, d)
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        let n = self.num as i128 * rhs.num as i128;
        let d = self.den as i128 * rhs.den as i128;
        Self::from_wide(n, d)
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        let n = self.num as i128 * rhs.den as i128;
        let d = self.den as i128 * rhs.num as i128;
        Self::from_wide(n, d)
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        Self::from_wide(-(self.num as i128), self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        let len = bytes.len();
        if len == 0 {
            return Err(parse_error(0, ParseKind::Empty));
        }

        let mut i = 0;
        let negative = bytes[0] == b'-';
        if negative {
            i = 1;
        }

        let numerator_start = i;
        while i < len && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == numerator_start {
            return Err(parse_error(i, ParseKind::InvalidByte));
        }

        let numerator_end = i;
        let mut denominator_start = None;
        let mut denominator_end = None;

        if i < len && bytes[i] == b'/' {
            i += 1;
            denominator_start = Some(i);
            while i < len && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == denominator_start.unwrap() {
                return Err(parse_error(i, ParseKind::InvalidByte));
            }
            denominator_end = Some(i);
        }

        if i < len {
            return Err(parse_error(i, ParseKind::InvalidByte));
        }

        let numerator_digits = &bytes[numerator_start..numerator_end];
        if numerator_digits.len() >= 2 && numerator_digits[0] == b'0' {
            return Err(parse_error(
                if negative { 2 } else { 1 },
                ParseKind::NonCanonical,
            ));
        }
        if negative && numerator_digits == b"0" {
            return Err(parse_error(0, ParseKind::NonCanonical));
        }

        if let Some(start) = denominator_start {
            let end = denominator_end.unwrap();
            let digits = &bytes[start..end];
            if digits.len() >= 2 && digits[0] == b'0' {
                return Err(parse_error(start + 1, ParseKind::NonCanonical));
            }
        }

        let max_num = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        let numerator_magnitude = parse_magnitude(numerator_digits, max_num)
            .ok_or_else(|| parse_error(0, ParseKind::OutOfRange))?;

        let numerator = if negative {
            -(numerator_magnitude as i128)
        } else {
            numerator_magnitude as i128
        };

        let (denominator, slash_at) = if let Some(start) = denominator_start {
            let end = denominator_end.unwrap();
            let digits = &bytes[start..end];
            let magnitude = parse_magnitude(digits, i64::MAX as u128)
                .ok_or_else(|| parse_error(start, ParseKind::OutOfRange))?;
            (magnitude as i128, start - 1)
        } else {
            return Ok(Self {
                num: numerator as i64,
                den: 1,
            });
        };

        if denominator == 0 {
            return Err(parse_error(
                denominator_start.unwrap(),
                ParseKind::ZeroDenominator,
            ));
        }
        if denominator == 1 {
            return Err(parse_error(slash_at, ParseKind::NonCanonical));
        }
        if numerator == 0 {
            return Err(parse_error(slash_at, ParseKind::NonCanonical));
        }
        if gcd_u128(numerator.unsigned_abs(), denominator as u128) > 1 {
            return Err(parse_error(slash_at, ParseKind::NonCanonical));
        }

        Ok(Self {
            num: numerator as i64,
            den: denominator as i64,
        })
    }

    fn from_wide(mut num: i128, mut den: i128) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        if den < 0 {
            num = -num;
            den = -den;
        }

        if num == 0 {
            return Ok(Self { num: 0, den: 1 });
        }

        let divisor = gcd_u128(num.unsigned_abs(), den as u128) as i128;
        num /= divisor;
        den /= divisor;

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

fn parse_error(at: usize, kind: ParseKind) -> RationalError {
    RationalError::Parse { at, kind }
}

fn parse_magnitude(digits: &[u8], limit: u128) -> Option<u128> {
    let mut value = 0u128;
    for &digit in digits {
        value = value.checked_mul(10)?.checked_add((digit - b'0') as u128)?;
        if value > limit {
            return None;
        }
    }
    Some(value)
}

fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let remainder = a % b;
        a = b;
        b = remainder;
    }
    a
}