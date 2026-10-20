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

fn gcd_u128(mut x: u128, mut y: u128) -> u128 {
    while y != 0 {
        let rem = x % y;
        x = y;
        y = rem;
    }
    x
}

fn build(n: i128, d: i128) -> Result<Rational, RationalError> {
    if d == 0 {
        return Err(RationalError::ZeroDenominator);
    }
    let (mut n, mut d) = if d < 0 { (-n, -d) } else { (n, d) };
    if n == 0 {
        return Ok(Rational { num: 0, den: 1 });
    }
    let factor = gcd_u128(n.unsigned_abs(), d as u128) as i128;
    n /= factor;
    d /= factor;
    let n64 = i64::try_from(n).map_err(|_| RationalError::Overflow)?;
    let d64 = i64::try_from(d).map_err(|_| RationalError::Overflow)?;
    if d64 <= 0 {
        return Err(RationalError::Overflow);
    }
    Ok(Rational { num: n64, den: d64 })
}

impl Rational {
    pub fn new(num: i64, den: i64) -> Result<Self, RationalError> {
        if den == 0 {
            Err(RationalError::ZeroDenominator)
        } else {
            build(num as i128, den as i128)
        }
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
        build(a * d + c * b, b * d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let a = self.num as i128;
        let b = self.den as i128;
        let c = rhs.num as i128;
        let d = rhs.den as i128;
        build(a * d - c * b, b * d)
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        build(
            self.num as i128 * rhs.num as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        build(
            self.num as i128 * rhs.den as i128,
            self.den as i128 * rhs.num as i128,
        )
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        build(-(self.num as i128), self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let s = text.as_bytes();
        let err = |at, kind| RationalError::Parse { at, kind };
        if s.is_empty() {
            return Err(err(0, ParseKind::Empty));
        }

        let mut pos = usize::from(s[0] == b'-');
        let negative = pos == 1;
        let n0 = pos;
        while pos < s.len() && s[pos].is_ascii_digit() {
            pos += 1;
        }
        if pos == n0 {
            return Err(err(pos, ParseKind::InvalidByte));
        }
        let n1 = pos;

        let mut dspan = None;
        if pos < s.len() && s[pos] == b'/' {
            pos += 1;
            let d0 = pos;
            while pos < s.len() && s[pos].is_ascii_digit() {
                pos += 1;
            }
            if pos == d0 {
                return Err(err(pos, ParseKind::InvalidByte));
            }
            dspan = Some((d0, pos));
        }
        if pos != s.len() {
            return Err(err(pos, ParseKind::InvalidByte));
        }

        let nd = &s[n0..n1];
        if nd.len() > 1 && nd[0] == b'0' {
            return Err(err(if negative { 2 } else { 1 }, ParseKind::NonCanonical));
        }
        if negative && nd == b"0" {
            return Err(err(0, ParseKind::NonCanonical));
        }
        if let Some((d0, d1)) = dspan {
            if d1 - d0 > 1 && s[d0] == b'0' {
                return Err(err(d0 + 1, ParseKind::NonCanonical));
            }
        }

        let nm = magnitude(nd, if negative { 1u128 << 63 } else { i64::MAX as u128 })
            .ok_or_else(|| err(0, ParseKind::OutOfRange))?;
        let dm = if let Some((d0, d1)) = dspan {
            Some(
                magnitude(&s[d0..d1], i64::MAX as u128)
                    .ok_or_else(|| err(d0, ParseKind::OutOfRange))?,
            )
        } else {
            None
        };

        if let (Some((d0, _)), Some(dv)) = (dspan, dm) {
            if dv == 0 {
                return Err(err(d0, ParseKind::ZeroDenominator));
            }
        }

        if let (Some((d0, _)), Some(dv)) = (dspan, dm) {
            let slash = d0 - 1;
            if dv == 1 || nm == 0 || gcd_u128(nm, dv) != 1 {
                return Err(err(slash, ParseKind::NonCanonical));
            }
            let numerator = if negative { -(nm as i128) } else { nm as i128 };
            build(numerator, dv as i128)
        } else {
            let numerator = if negative { -(nm as i128) } else { nm as i128 };
            build(numerator, 1)
        }
    }
}

fn magnitude(digits: &[u8], limit: u128) -> Option<u128> {
    let mut acc = 0u128;
    for &byte in digits {
        let digit = (byte - b'0') as u128;
        if acc > limit / 10 || (acc == limit / 10 && digit > limit % 10) {
            return None;
        }
        acc = acc * 10 + digit;
    }
    Some(acc)
}

impl fmt::Display for Rational {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.den {
            1 => write!(f, "{}", self.num),
            den => write!(f, "{}/{}", self.num, den),
        }
    }
}
