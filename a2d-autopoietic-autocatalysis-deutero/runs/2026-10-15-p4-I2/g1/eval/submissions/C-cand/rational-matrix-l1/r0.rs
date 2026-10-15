use std::fmt;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rational {
    num: i64,
    den: i64,
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

fn from_wide(mut num: i128, mut den: i128) -> Result<Rational, RationalError> {
    if den == 0 {
        return Err(RationalError::ZeroDenominator);
    }
    if den < 0 {
        num = -num;
        den = -den;
    }
    if num == 0 {
        return Ok(Rational { num: 0, den: 1 });
    }

    let g = gcd(num.unsigned_abs(), den as u128) as i128;
    num /= g;
    den /= g;

    if num < i64::MIN as i128 || num > i64::MAX as i128 || den > i64::MAX as i128 {
        return Err(RationalError::Overflow);
    }

    Ok(Rational {
        num: num as i64,
        den: den as i64,
    })
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
        from_wide(
            self.num as i128 * rhs.den as i128 + rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        from_wide(
            self.num as i128 * rhs.den as i128 - rhs.num as i128 * self.den as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        from_wide(
            self.num as i128 * rhs.num as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        from_wide(
            self.num as i128 * rhs.den as i128,
            self.den as i128 * rhs.num as i128,
        )
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        from_wide(-(self.num as i128), self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        let fail = |at, kind| RationalError::Parse { at, kind };

        if bytes.is_empty() {
            return Err(fail(0, ParseKind::Empty));
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
            return Err(fail(i, ParseKind::InvalidByte));
        }
        let num_end = i;

        let mut den_start = None;
        let mut den_end = None;
        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(fail(i, ParseKind::InvalidByte));
            }
            i += 1;
            den_start = Some(i);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == den_start.unwrap() {
                return Err(fail(i, ParseKind::InvalidByte));
            }
            den_end = Some(i);
            if i < bytes.len() {
                return Err(fail(i, ParseKind::InvalidByte));
            }
        }

        if num_end - num_start >= 2 && bytes[num_start] == b'0' {
            return Err(fail(if negative { 2 } else { 1 }, ParseKind::NonCanonical));
        }
        if negative && num_end - num_start == 1 && bytes[num_start] == b'0' {
            return Err(fail(0, ParseKind::NonCanonical));
        }

        if let (Some(ds), Some(de)) = (den_start, den_end) {
            if de - ds >= 2 && bytes[ds] == b'0' {
                return Err(fail(ds + 1, ParseKind::NonCanonical));
            }
        }

        let num_limit: u128 = if negative { 1u128 << 63 } else { i64::MAX as u128 };
        let mut num_mag = 0u128;
        for &b in &bytes[num_start..num_end] {
            num_mag = num_mag * 10 + (b - b'0') as u128;
            if num_mag > num_limit {
                return Err(fail(0, ParseKind::OutOfRange));
            }
        }

        let mut den_value = 1u128;
        if let (Some(ds), Some(de)) = (den_start, den_end) {
            den_value = 0;
            for &b in &bytes[ds..de] {
                den_value = den_value * 10 + (b - b'0') as u128;
                if den_value > i64::MAX as u128 {
                    return Err(fail(ds, ParseKind::OutOfRange));
                }
            }
        }

        if let Some(ds) = den_start {
            if den_value == 0 {
                return Err(fail(ds, ParseKind::ZeroDenominator));
            }
            let slash = ds - 1;
            if den_value == 1 {
                return Err(fail(slash, ParseKind::NonCanonical));
            }
            if num_mag == 0 {
                return Err(fail(slash, ParseKind::NonCanonical));
            }
            if gcd(num_mag, den_value) > 1 {
                return Err(fail(slash, ParseKind::NonCanonical));
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

fn main() {}