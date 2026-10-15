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
        Self::from_i128(
            self.num as i128 * rhs.num as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        Self::from_i128(
            self.num as i128 * rhs.den as i128,
            self.den as i128 * rhs.num as i128,
        )
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        Self::from_i128(-(self.num as i128), self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
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
            } else if b.is_ascii_digit() {
                // Digits are valid in either token.
            } else if b == b'-' && i == 0 {
                // The only permitted minus is the leading numerator sign.
            } else {
                return Err(parse_error(i, ParseKind::InvalidByte));
            }
        }

        let numerator_end = slash.unwrap_or(bytes.len());
        let numerator_start = usize::from(bytes[0] == b'-');
        if numerator_start == numerator_end {
            return Err(parse_error(bytes.len(), ParseKind::InvalidByte));
        }
        if let Some(s) = slash {
            if s + 1 == bytes.len() {
                return Err(parse_error(bytes.len(), ParseKind::InvalidByte));
            }
        }

        let numerator_digits = &bytes[numerator_start..numerator_end];
        if numerator_digits.len() >= 2 && numerator_digits[0] == b'0' {
            return Err(parse_error(numerator_start + 1, ParseKind::NonCanonical));
        }
        if bytes[0] == b'-' && numerator_digits == b"0" {
            return Err(parse_error(0, ParseKind::NonCanonical));
        }

        let denominator_digits = slash.map(|s| &bytes[s + 1..]);
        if let Some(digits) = denominator_digits {
            if digits.len() >= 2 && digits[0] == b'0' {
                return Err(parse_error(slash.unwrap() + 2, ParseKind::NonCanonical));
            }
        }

        let negative = bytes[0] == b'-';
        let numerator_magnitude = parse_magnitude(numerator_digits);
        let numerator_limit = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        if numerator_magnitude > numerator_limit {
            return Err(parse_error(0, ParseKind::OutOfRange));
        }

        let numerator = if negative {
            if numerator_magnitude == (1u128 << 63) {
                i64::MIN
            } else {
                -(numerator_magnitude as i64)
            }
        } else {
            numerator_magnitude as i64
        };

        let Some(den_digits) = denominator_digits else {
            return Ok(Self { num: numerator, den: 1 });
        };

        let den_at = slash.unwrap() + 1;
        let denominator_magnitude = parse_magnitude(den_digits);
        if denominator_magnitude > i64::MAX as u128 {
            return Err(parse_error(den_at, ParseKind::OutOfRange));
        }
        if denominator_magnitude == 0 {
            return Err(parse_error(den_at, ParseKind::ZeroDenominator));
        }
        if denominator_magnitude == 1 {
            return Err(parse_error(slash.unwrap(), ParseKind::NonCanonical));
        }
        if numerator == 0 {
            return Err(parse_error(slash.unwrap(), ParseKind::NonCanonical));
        }
        if gcd_u128(numerator_magnitude, denominator_magnitude) > 1 {
            return Err(parse_error(slash.unwrap(), ParseKind::NonCanonical));
        }

        Ok(Self {
            num: numerator,
            den: denominator_magnitude as i64,
        })
    }

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

        let divisor = gcd_u128(n.unsigned_abs(), d as u128);
        n /= divisor as i128;
        d /= divisor as i128;

        if n < i64::MIN as i128
            || n > i64::MAX as i128
            || d < 1
            || d > i64::MAX as i128
        {
            return Err(RationalError::Overflow);
        }

        Ok(Self {
            num: n as i64,
            den: d as i64,
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

fn parse_magnitude(digits: &[u8]) -> u128 {
    digits.iter().fold(0u128, |value, &digit| {
        value * 10 + (digit - b'0') as u128
    })
}

fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

fn main() {}