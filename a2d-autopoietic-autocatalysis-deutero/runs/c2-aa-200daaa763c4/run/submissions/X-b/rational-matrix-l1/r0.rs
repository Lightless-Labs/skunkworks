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

        let (mut n, mut d) = (num, den);
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
        let n = -(self.num as i128);
        Self::from_i128(n, self.den as i128)
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

        let num_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == num_start {
            if i == bytes.len() {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
            return Err(parse_err(i, ParseKind::InvalidByte));
        }

        let num_end = i;
        let mut den_start = None;
        let mut den_end = None;
        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
            i += 1;
            let start = i;
            den_start = Some(start);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
            den_end = Some(i);
        }
        if i < bytes.len() {
            return Err(parse_err(i, ParseKind::InvalidByte));
        }

        let num_digits_start = if negative { 1 } else { 0 };
        let num_digits = num_end - num_digits_start;
        if num_digits >= 2 && bytes[num_digits_start] == b'0' {
            return Err(parse_err(num_digits_start + 1, ParseKind::NonCanonical));
        }
        if negative && num_digits == 1 && bytes[0] == b'-' && bytes[1] == b'0' {
            return Err(parse_err(0, ParseKind::NonCanonical));
        }

        if let (Some(start), Some(end)) = (den_start, den_end) {
            if end - start >= 2 && bytes[start] == b'0' {
                return Err(parse_err(start + 1, ParseKind::NonCanonical));
            }
        }

        let num_mag = parse_magnitude(&bytes[num_digits_start..num_end]);
        let num_limit = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        if num_mag.map_or(true, |v| v > num_limit) {
            return Err(parse_err(0, ParseKind::OutOfRange));
        }

        let num_mag = num_mag.unwrap();
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
        let den_mag = parse_magnitude(&bytes[start..end]);
        if den_mag.map_or(true, |v| v > i64::MAX as u128) {
            return Err(parse_err(start, ParseKind::OutOfRange));
        }
        let den_mag = den_mag.unwrap() as i64;

        if den_mag == 0 {
            return Err(parse_err(start, ParseKind::ZeroDenominator));
        }

        let slash = start - 1;
        if den_mag == 1 {
            return Err(parse_err(slash, ParseKind::NonCanonical));
        }
        if num == 0 {
            return Err(parse_err(slash, ParseKind::NonCanonical));
        }

        if gcd_u128(num.unsigned_abs() as u128, den_mag as u128) > 1 {
            return Err(parse_err(slash, ParseKind::NonCanonical));
        }

        Ok(Self { num, den: den_mag })
    }
}

fn parse_magnitude(bytes: &[u8]) -> Option<u128> {
    let mut value = 0u128;
    for &b in bytes {
        let digit = (b - b'0') as u128;
        value = value.checked_mul(10)?.checked_add(digit)?;
    }
    Some(value)
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