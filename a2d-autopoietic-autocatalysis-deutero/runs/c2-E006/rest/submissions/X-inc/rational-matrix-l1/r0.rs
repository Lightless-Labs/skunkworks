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
        let a = self.num as i128;
        let b = self.den as i128;
        let c = rhs.num as i128;
        let d = rhs.den as i128;
        let g = gcd_i128(b, d);
        let left = a * (d / g);
        let right = c * (b / g);
        let den = (b / g) * d;
        Self::from_i128(left + right, den)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let a = self.num as i128;
        let b = self.den as i128;
        let c = rhs.num as i128;
        let d = rhs.den as i128;
        let g = gcd_i128(b, d);
        let left = a * (d / g);
        let right = c * (b / g);
        let den = (b / g) * d;
        Self::from_i128(left - right, den)
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        let mut a = self.num as i128;
        let mut b = self.den as i128;
        let mut c = rhs.num as i128;
        let mut d = rhs.den as i128;

        let g1 = gcd_i128(a, d);
        a /= g1;
        d /= g1;
        let g2 = gcd_i128(c, b);
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

        let g1 = gcd_i128(a, d);
        a /= g1;
        d /= g1;
        let g2 = gcd_i128(c, b);
        c /= g2;
        b /= g2;

        let mut numerator = a * c;
        let mut denominator = b * d;
        if denominator < 0 {
            numerator = -numerator;
            denominator = -denominator;
        }
        Self::from_i128(numerator, denominator)
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        let n = -(self.num as i128);
        Self::from_i128(n, self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        let parse_error = |at, kind| RationalError::Parse { at, kind };

        if bytes.is_empty() {
            return Err(parse_error(0, ParseKind::Empty));
        }

        let mut i = 0;
        let negative = bytes[0] == b'-';
        if negative {
            i += 1;
        }

        let numerator_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == numerator_start {
            if i == bytes.len() {
                return Err(parse_error(i, ParseKind::InvalidByte));
            }
            return Err(parse_error(i, ParseKind::InvalidByte));
        }
        let numerator_end = i;

        let mut denominator_start = None;
        let mut denominator_end = None;
        let slash_at;
        if i < bytes.len() && bytes[i] == b'/' {
            slash_at = i;
            i += 1;
            denominator_start = Some(i);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == denominator_start.unwrap() {
                return Err(parse_error(i, ParseKind::InvalidByte));
            }
            denominator_end = Some(i);
        } else {
            slash_at = 0;
        }

        if i < bytes.len() {
            return Err(parse_error(i, ParseKind::InvalidByte));
        }

        let num_digits = numerator_end - numerator_start;
        if num_digits >= 2 && bytes[numerator_start] == b'0' {
            return Err(parse_error(numerator_start + 1, ParseKind::NonCanonical));
        }
        if negative && num_digits == 1 && bytes[numerator_start] == b'0' {
            return Err(parse_error(0, ParseKind::NonCanonical));
        }

        if let Some(start) = denominator_start {
            let end = denominator_end.unwrap();
            if end - start >= 2 && bytes[start] == b'0' {
                return Err(parse_error(start + 1, ParseKind::NonCanonical));
            }
        }

        let numerator_magnitude = parse_bounded_magnitude(
            &bytes[numerator_start..numerator_end],
            if negative { 1u128 << 63 } else { i64::MAX as u128 },
        )
        .ok_or_else(|| parse_error(0, ParseKind::OutOfRange))?;

        let numerator = if negative {
            if numerator_magnitude == 1u128 << 63 {
                i64::MIN
            } else {
                -(numerator_magnitude as i64)
            }
        } else {
            numerator_magnitude as i64
        };

        let Some(start) = denominator_start else {
            return Ok(Self {
                num: numerator,
                den: 1,
            });
        };
        let end = denominator_end.unwrap();
        let denominator_magnitude = parse_bounded_magnitude(
            &bytes[start..end],
            i64::MAX as u128,
        )
        .ok_or_else(|| parse_error(start, ParseKind::OutOfRange))?;

        if denominator_magnitude == 0 {
            return Err(parse_error(start, ParseKind::ZeroDenominator));
        }
        if denominator_magnitude == 1 {
            return Err(parse_error(slash_at, ParseKind::NonCanonical));
        }
        if numerator == 0 {
            return Err(parse_error(slash_at, ParseKind::NonCanonical));
        }

        if gcd_u128(numerator_magnitude, denominator_magnitude) > 1 {
            return Err(parse_error(slash_at, ParseKind::NonCanonical));
        }

        Ok(Self {
            num: numerator,
            den: denominator_magnitude as i64,
        })
    }

    fn from_i128(mut num: i128, mut den: i128) -> Result<Self, RationalError> {
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

        let gcd = gcd_i128(num, den);
        num /= gcd;
        den /= gcd;

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

fn gcd_i128(a: i128, b: i128) -> i128 {
    gcd_u128(a.unsigned_abs(), b.unsigned_abs()) as i128
}

fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let remainder = a % b;
        a = b;
        b = remainder;
    }
    a
}

fn parse_bounded_magnitude(digits: &[u8], limit: u128) -> Option<u128> {
    let mut value = 0u128;
    for &digit in digits {
        let digit = (digit - b'0') as u128;
        if value > (limit - digit) / 10 {
            return None;
        }
        value = value * 10 + digit;
    }
    Some(value)
}