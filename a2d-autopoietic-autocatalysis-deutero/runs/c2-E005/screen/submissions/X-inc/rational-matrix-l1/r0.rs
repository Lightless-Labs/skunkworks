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

fn rational_from_wide(num: i128, den: i128) -> Result<Rational, RationalError> {
    if den == 0 {
        return Err(RationalError::ZeroDenominator);
    }
    if num == 0 {
        return Ok(Rational { num: 0, den: 1 });
    }

    let mut n = num;
    let mut d = den;
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

    Ok(Rational {
        num: n as i64,
        den: d as i64,
    })
}

impl Rational {
    pub fn new(num: i64, den: i64) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        rational_from_wide(num as i128, den as i128)
    }

    pub fn numerator(&self) -> i64 {
        self.num
    }

    pub fn denominator(&self) -> i64 {
        self.den
    }

    pub fn add(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_from_wide(
            self.num as i128 * rhs.den as i128 + rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_from_wide(
            self.num as i128 * rhs.den as i128 - rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_from_wide(
            self.num as i128 * rhs.num as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        rational_from_wide(
            self.num as i128 * rhs.den as i128,
            self.den as i128 * rhs.num as i128,
        )
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        let n = -(self.num as i128);
        if n < i64::MIN as i128 || n > i64::MAX as i128 {
            return Err(RationalError::Overflow);
        }
        Ok(Rational { num: n as i64, den: self.den })
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(RationalError::Parse { at: 0, kind: ParseKind::Empty });
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
        let num_end = i;

        if num_start == num_end {
            if i == bytes.len() {
                return Err(RationalError::Parse { at: i, kind: ParseKind::InvalidByte });
            }
            return Err(RationalError::Parse { at: i, kind: ParseKind::InvalidByte });
        }

        let mut slash = None;
        let mut den_start = 0;
        let mut den_end = 0;
        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(RationalError::Parse { at: i, kind: ParseKind::InvalidByte });
            }
            slash = Some(i);
            i += 1;
            den_start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            den_end = i;
            if den_start == den_end {
                return Err(RationalError::Parse { at: i, kind: ParseKind::InvalidByte });
            }
            if i < bytes.len() {
                return Err(RationalError::Parse { at: i, kind: ParseKind::InvalidByte });
            }
        }

        let parse_err = |at, kind| RationalError::Parse { at, kind };

        let num_digits = num_end - num_start;
        if num_digits >= 2 && bytes[num_start] == b'0' {
            return Err(parse_err(num_start + 1, ParseKind::NonCanonical));
        }
        if negative && num_digits == 1 && bytes[num_start] == b'0' {
            return Err(parse_err(0, ParseKind::NonCanonical));
        }
        if slash.is_some() && den_end - den_start >= 2 && bytes[den_start] == b'0' {
            return Err(parse_err(den_start + 1, ParseKind::NonCanonical));
        }

        let parse_magnitude = |start: usize, end: usize, limit: u128| -> Option<u128> {
            let mut value = 0u128;
            for &b in &bytes[start..end] {
                value = value.checked_mul(10)?.checked_add((b - b'0') as u128)?;
                if value > limit {
                    return None;
                }
            }
            Some(value)
        };

        let num_limit = if negative { 1u128 << 63 } else { i64::MAX as u128 };
        let num_mag = parse_magnitude(num_start, num_end, num_limit)
            .ok_or_else(|| parse_err(0, ParseKind::OutOfRange))?;

        let den_mag = if slash.is_some() {
            Some(
                parse_magnitude(den_start, den_end, i64::MAX as u128)
                    .ok_or_else(|| parse_err(den_start, ParseKind::OutOfRange))?,
            )
        } else {
            None
        };

        if den_mag == Some(0) {
            return Err(parse_err(den_start, ParseKind::ZeroDenominator));
        }

        if let (Some(slash_at), Some(d)) = (slash, den_mag) {
            if d == 1 || num_mag == 0 {
                return Err(parse_err(slash_at, ParseKind::NonCanonical));
            }
            if gcd_u128(num_mag, d) > 1 {
                return Err(parse_err(slash_at, ParseKind::NonCanonical));
            }
        }

        let signed_num = if negative {
            if num_mag == (1u128 << 63) {
                i64::MIN
            } else {
                -(num_mag as i64)
            }
        } else {
            num_mag as i64
        };
        let den = den_mag.unwrap_or(1) as i64;
        Ok(Rational { num: signed_num, den })
    }
}

impl std::fmt::Display for Rational {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.den == 1 {
            write!(f, "{}", self.num)
        } else {
            write!(f, "{}/{}", self.num, self.den)
        }
    }
}