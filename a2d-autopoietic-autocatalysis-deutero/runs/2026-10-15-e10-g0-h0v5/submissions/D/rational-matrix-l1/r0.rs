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
        let remainder = a % b;
        a = b;
        b = remainder;
    }
    a
}

impl Rational {
    fn from_i128_parts(num: i128, den: i128) -> Result<Self, RationalError> {
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

        let gcd = gcd_u128(n.unsigned_abs(), d as u128) as i128;
        n /= gcd;
        d /= gcd;

        if n < i64::MIN as i128 || n > i64::MAX as i128 || d < 1 || d > i64::MAX as i128 {
            return Err(RationalError::Overflow);
        }

        Ok(Self {
            num: n as i64,
            den: d as i64,
        })
    }

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

        let gcd = gcd_u128(n.unsigned_abs(), d as u128) as i128;
        n /= gcd;
        d /= gcd;

        if n < i64::MIN as i128 || n > i64::MAX as i128 || d > i64::MAX as i128 {
            return Err(RationalError::Overflow);
        }

        Ok(Self {
            num: n as i64,
            den: d as i64,
        })
    }

    pub fn numerator(&self) -> i64 {
        self.num
    }

    pub fn denominator(&self) -> i64 {
        self.den
    }

    pub fn add(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::from_i128_parts(
            self.num as i128 * rhs.den as i128 + rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::from_i128_parts(
            self.num as i128 * rhs.den as i128 - rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        Self::from_i128_parts(
            self.num as i128 * rhs.num as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }

        Self::from_i128_parts(
            self.num as i128 * rhs.den as i128,
            self.den as i128 * rhs.num as i128,
        )
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        Self::from_i128_parts(-(self.num as i128), self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        let fail = |at, kind| Err(RationalError::Parse { at, kind });

        if bytes.is_empty() {
            return fail(0, ParseKind::Empty);
        }

        let negative = bytes[0] == b'-';
        let num_digits_start = usize::from(negative);
        let mut i = num_digits_start;

        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        let num_end = i;

        if num_end == num_digits_start {
            return fail(i, ParseKind::InvalidByte);
        }

        let mut den_start = None;
        let mut den_end = None;
        if i < bytes.len() {
            if bytes[i] != b'/' {
                return fail(i, ParseKind::InvalidByte);
            }
            i += 1;
            den_start = Some(i);

            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == den_start.unwrap() {
                return fail(i, ParseKind::InvalidByte);
            }
            den_end = Some(i);
        }

        if i < bytes.len() {
            return fail(i, ParseKind::InvalidByte);
        }

        let num_digits = num_end - num_digits_start;
        if num_digits >= 2 && bytes[num_digits_start] == b'0' {
            return fail(num_digits_start + 1, ParseKind::NonCanonical);
        }
        if negative && num_digits == 1 && bytes[1] == b'0' {
            return fail(0, ParseKind::NonCanonical);
        }

        if let (Some(start), Some(end)) = (den_start, den_end) {
            if end - start >= 2 && bytes[start] == b'0' {
                return fail(start + 1, ParseKind::NonCanonical);
            }
        }

        let mut numerator_magnitude = 0u128;
        for &byte in &bytes[num_digits_start..num_end] {
            numerator_magnitude = numerator_magnitude * 10 + (byte - b'0') as u128;
        }

        let numerator_limit = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        if numerator_magnitude > numerator_limit {
            return fail(0, ParseKind::OutOfRange);
        }

        let numerator = if negative {
            if numerator_magnitude == 1u128 << 63 {
                i64::MIN
            } else {
                -(numerator_magnitude as i64)
            }
        } else {
            numerator_magnitude as i64
        };

        let mut denominator = 1u64;
        if let (Some(start), Some(end)) = (den_start, den_end) {
            let mut denominator_magnitude = 0u128;
            for &byte in &bytes[start..end] {
                denominator_magnitude = denominator_magnitude * 10 + (byte - b'0') as u128;
            }

            if denominator_magnitude > i64::MAX as u128 {
                return fail(start, ParseKind::OutOfRange);
            }
            denominator = denominator_magnitude as u64;

            if denominator == 0 {
                return fail(start, ParseKind::ZeroDenominator);
            }

            let slash = start - 1;
            if denominator == 1 {
                return fail(slash, ParseKind::NonCanonical);
            }
            if numerator == 0 {
                return fail(slash, ParseKind::NonCanonical);
            }
            if gcd_u128(numerator.unsigned_abs() as u128, denominator as u128) > 1 {
                return fail(slash, ParseKind::NonCanonical);
            }
        }

        Ok(Self {
            num: numerator,
            den: denominator as i64,
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