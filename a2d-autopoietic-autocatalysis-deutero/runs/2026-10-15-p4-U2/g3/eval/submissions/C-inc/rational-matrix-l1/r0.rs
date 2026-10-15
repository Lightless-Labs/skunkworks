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

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

fn rational_from_i128(num: i128, den: i128) -> Result<Rational, RationalError> {
    if den == 0 {
        return Err(RationalError::ZeroDenominator);
    }
    let mut n = num;
    let mut d = den;
    if d < 0 {
        n = -n;
        d = -d;
    }
    if n == 0 {
        return Ok(Rational { num: 0, den: 1 });
    }
    let g = gcd(n.unsigned_abs(), d as u128) as i128;
    n /= g;
    d /= g;
    if n < i64::MIN as i128 || n > i64::MAX as i128 || d > i64::MAX as i128 {
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
        rational_from_i128(num as i128, den as i128)
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
        rational_from_i128(n, d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let n = self.num as i128 * rhs.den as i128
            - rhs.num as i128 * self.den as i128;
        let d = self.den as i128 * rhs.den as i128;
        rational_from_i128(n, d)
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        rational_from_i128(
            self.num as i128 * rhs.num as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        rational_from_i128(
            self.num as i128 * rhs.den as i128,
            self.den as i128 * rhs.num as i128,
        )
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        let n = -(self.num as i128);
        rational_from_i128(n, self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        let fail = |at, kind| RationalError::Parse { at, kind };

        if bytes.is_empty() {
            return Err(fail(0, ParseKind::Empty));
        }

        let mut i = 0;
        if bytes[i] == b'-' {
            i += 1;
            if i == bytes.len() {
                return Err(fail(i, ParseKind::InvalidByte));
            }
        }

        let num_start = 0;
        let num_digits_start = if bytes[0] == b'-' { 1 } else { 0 };
        let mut slash = None;
        let mut j = num_digits_start;
        while j < bytes.len() {
            match bytes[j] {
                b'0'..=b'9' => j += 1,
                b'/' => {
                    slash = Some(j);
                    break;
                }
                _ => return Err(fail(j, ParseKind::InvalidByte)),
            }
        }
        if j == num_digits_start {
            return Err(fail(j, ParseKind::InvalidByte));
        }

        let den_start;
        if let Some(slash_at) = slash {
            den_start = slash_at + 1;
            if den_start == bytes.len() {
                return Err(fail(bytes.len(), ParseKind::InvalidByte));
            }
            let mut k = den_start;
            while k < bytes.len() {
                if !bytes[k].is_ascii_digit() {
                    return Err(fail(k, ParseKind::InvalidByte));
                }
                k += 1;
            }
        } else {
            den_start = bytes.len();
        }

        let num_end = slash.unwrap_or(bytes.len());
        let num_digits = &bytes[num_digits_start..num_end];

        if num_digits.len() >= 2 && num_digits[0] == b'0' {
            return Err(fail(num_digits_start + 1, ParseKind::NonCanonical));
        }
        if bytes[0] == b'-' && num_digits == b"0" {
            return Err(fail(0, ParseKind::NonCanonical));
        }

        if let Some(slash_at) = slash {
            let den_digits = &bytes[den_start..];
            if den_digits.len() >= 2 && den_digits[0] == b'0' {
                return Err(fail(den_start + 1, ParseKind::NonCanonical));
            }
            let den_mag = parse_bounded(den_digits, i64::MAX as u128);
            if den_mag.is_none() {
                return Err(fail(den_start, ParseKind::OutOfRange));
            }
            let num_limit = if bytes[0] == b'-' {
                1u128 << 63
            } else {
                i64::MAX as u128
            };
            let num_mag = parse_bounded(num_digits, num_limit);
            if num_mag.is_none() {
                return Err(fail(num_start, ParseKind::OutOfRange));
            }

            let den_mag = den_mag.unwrap();
            if den_mag == 0 {
                return Err(fail(den_start, ParseKind::ZeroDenominator));
            }
            if den_mag == 1 {
                return Err(fail(slash_at, ParseKind::NonCanonical));
            }
            if num_mag == Some(0) {
                return Err(fail(slash_at, ParseKind::NonCanonical));
            }
            if gcd(num_mag.unwrap(), den_mag) > 1 {
                return Err(fail(slash_at, ParseKind::NonCanonical));
            }
            let magnitude = num_mag.unwrap();
            let n = if bytes[0] == b'-' {
                if magnitude == 1u128 << 63 {
                    i64::MIN
                } else {
                    -(magnitude as i64)
                }
            } else {
                magnitude as i64
            };
            return Ok(Rational {
                num: n,
                den: den_mag as i64,
            });
        }

        let limit = if bytes[0] == b'-' {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        let magnitude = parse_bounded(num_digits, limit)
            .ok_or_else(|| fail(num_start, ParseKind::OutOfRange))?;
        let n = if bytes[0] == b'-' {
            if magnitude == 1u128 << 63 {
                i64::MIN
            } else {
                -(magnitude as i64)
            }
        } else {
            magnitude as i64
        };
        Ok(Rational { num: n, den: 1 })
    }
}

fn parse_bounded(digits: &[u8], limit: u128) -> Option<u128> {
    let mut value = 0u128;
    for &byte in digits {
        let digit = (byte - b'0') as u128;
        value = value.checked_mul(10)?.checked_add(digit)?;
        if value > limit {
            return None;
        }
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

fn main() {}