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

fn from_wide(num: i128, den: i128) -> Result<Rational, RationalError> {
    if den == 0 {
        return Err(RationalError::ZeroDenominator);
    }

    let (mut n, mut d) = (num, den);
    if d < 0 {
        n = -n;
        d = -d;
    }

    if n == 0 {
        return Ok(Rational { num: 0, den: 1 });
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

    Ok(Rational {
        num: n as i64,
        den: d as i64,
    })
}

fn gcd_i64_abs_den(num: i64, den: i64) -> u64 {
    gcd_u128(num.unsigned_abs() as u128, den as u128) as u64
}

impl Rational {
    pub fn new(num: i64, den: i64) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        from_wide(num as i128, den as i128)
    }

    pub fn numerator(&self) -> i64 {
        self.num
    }

    pub fn denominator(&self) -> i64 {
        self.den
    }

    pub fn add(&self, rhs: &Self) -> Result<Self, RationalError> {
        let g = gcd_i64_abs_den(self.den, rhs.den) as i128;
        let a = rhs.den as i128 / g;
        let b = self.den as i128 / g;
        let n = self.num as i128 * a + rhs.num as i128 * b;
        let d = self.den as i128 * a;
        from_wide(n, d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let g = gcd_i64_abs_den(self.den, rhs.den) as i128;
        let a = rhs.den as i128 / g;
        let b = self.den as i128 / g;
        let n = self.num as i128 * a - rhs.num as i128 * b;
        let d = self.den as i128 * a;
        from_wide(n, d)
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        let g1 = gcd_u128(self.num.unsigned_abs() as u128, rhs.den as u128) as i128;
        let g2 = gcd_u128(rhs.num.unsigned_abs() as u128, self.den as u128) as i128;
        let n = (self.num as i128 / g1) * (rhs.num as i128 / g2);
        let d = (self.den as i128 / g2) * (rhs.den as i128 / g1);
        from_wide(n, d)
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        let g1 = gcd_u128(self.num.unsigned_abs() as u128, rhs.num.unsigned_abs() as u128) as i128;
        let g2 = gcd_u128(self.den as u128, rhs.den as u128) as i128;

        let mut n = (self.num as i128 / g1) * (rhs.den as i128 / g2);
        let mut d = (self.den as i128 / g2) * (rhs.num as i128 / g1);
        if d < 0 {
            n = -n;
            d = -d;
        }
        from_wide(n, d)
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        let n = self.num.checked_neg().ok_or(RationalError::Overflow)?;
        Ok(Rational { num: n, den: self.den })
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
            if i < bytes.len() {
                return Err(parse_err(i, ParseKind::InvalidByte));
            }
        }

        let num_digits = num_end - num_start;
        if num_digits >= 2 && bytes[num_start] == b'0' {
            return Err(parse_err(num_start + 1, ParseKind::NonCanonical));
        }
        if negative && num_digits == 1 && bytes[num_start] == b'0' {
            return Err(parse_err(0, ParseKind::NonCanonical));
        }

        if let (Some(start), Some(end)) = (den_start, den_end) {
            if end - start >= 2 && bytes[start] == b'0' {
                return Err(parse_err(start + 1, ParseKind::NonCanonical));
            }
        }

        let mut num_mag: u128 = 0;
        for &b in &bytes[num_start..num_end] {
            num_mag = num_mag * 10 + (b - b'0') as u128;
        }
        let num_limit = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        if num_mag > num_limit {
            return Err(parse_err(0, ParseKind::OutOfRange));
        }

        let mut den_value = 1u128;
        if let (Some(start), Some(end)) = (den_start, den_end) {
            den_value = 0;
            for &b in &bytes[start..end] {
                den_value = den_value * 10 + (b - b'0') as u128;
            }
            if den_value > i64::MAX as u128 {
                return Err(parse_err(start, ParseKind::OutOfRange));
            }
            if den_value == 0 {
                return Err(parse_err(start, ParseKind::ZeroDenominator));
            }
            if den_value == 1 {
                return Err(parse_err(start - 1, ParseKind::NonCanonical));
            }
            if num_mag == 0 {
                return Err(parse_err(start - 1, ParseKind::NonCanonical));
            }
            if gcd_u128(num_mag, den_value) > 1 {
                return Err(parse_err(start - 1, ParseKind::NonCanonical));
            }
        }

        let signed_num = if negative {
            if num_mag == 1u128 << 63 {
                i64::MIN
            } else {
                -(num_mag as i64)
            }
        } else {
            num_mag as i64
        };

        Ok(Rational {
            num: signed_num,
            den: den_value as i64,
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