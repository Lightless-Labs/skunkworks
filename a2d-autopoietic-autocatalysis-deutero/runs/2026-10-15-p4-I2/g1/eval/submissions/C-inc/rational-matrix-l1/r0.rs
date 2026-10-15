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

fn from_wide(num: i128, den: i128) -> Result<Rational, RationalError> {
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

    let divisor = gcd(n.unsigned_abs(), d as u128) as i128;
    n /= divisor;
    d /= divisor;

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
        let n = self.num as i128 * rhs.den as i128
            + rhs.num as i128 * self.den as i128;
        let d = self.den as i128 * rhs.den as i128;
        from_wide(n, d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let n = self.num as i128 * rhs.den as i128
            - rhs.num as i128 * self.den as i128;
        let d = self.den as i128 * rhs.den as i128;
        from_wide(n, d)
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
        if bytes.is_empty() {
            return Err(RationalError::Parse {
                at: 0,
                kind: ParseKind::Empty,
            });
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
            return Err(RationalError::Parse {
                at: i,
                kind: ParseKind::InvalidByte,
            });
        }

        let num_end = i;
        let mut den_start = None;
        let mut den_end = None;

        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
            i += 1;
            let start = i;
            if i == bytes.len() || !bytes[i].is_ascii_digit() {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            den_start = Some(start);
            den_end = Some(i);
            if i < bytes.len() {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
        }

        let slash = num_end;
        let num_digits = &bytes[num_start..num_end];
        if num_digits.len() > 1 && num_digits[0] == b'0' {
            return Err(RationalError::Parse {
                at: num_start + 1,
                kind: ParseKind::NonCanonical,
            });
        }
        if negative && num_digits == b"0" {
            return Err(RationalError::Parse {
                at: 0,
                kind: ParseKind::NonCanonical,
            });
        }

        if let (Some(start), Some(end)) = (den_start, den_end) {
            if end - start > 1 && bytes[start] == b'0' {
                return Err(RationalError::Parse {
                    at: start + 1,
                    kind: ParseKind::NonCanonical,
                });
            }
        }

        let mut num_mag: u128 = 0;
        for &digit in num_digits {
            num_mag = num_mag * 10 + (digit - b'0') as u128;
        }
        let num_limit = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        if num_mag > num_limit {
            return Err(RationalError::Parse {
                at: 0,
                kind: ParseKind::OutOfRange,
            });
        }

        let den_value = if let (Some(start), Some(end)) = (den_start, den_end) {
            let mut value: u128 = 0;
            for &digit in &bytes[start..end] {
                value = value * 10 + (digit - b'0') as u128;
            }
            if value > i64::MAX as u128 {
                return Err(RationalError::Parse {
                    at: start,
                    kind: ParseKind::OutOfRange,
                });
            }
            if value == 0 {
                return Err(RationalError::Parse {
                    at: start,
                    kind: ParseKind::ZeroDenominator,
                });
            }
            if value == 1 {
                return Err(RationalError::Parse {
                    at: slash,
                    kind: ParseKind::NonCanonical,
                });
            }
            if num_mag == 0 {
                return Err(RationalError::Parse {
                    at: slash,
                    kind: ParseKind::NonCanonical,
                });
            }
            if gcd(num_mag, value) > 1 {
                return Err(RationalError::Parse {
                    at: slash,
                    kind: ParseKind::NonCanonical,
                });
            }
            value
        } else {
            1
        };

        let signed_num = if negative {
            if num_mag == (1u128 << 63) {
                i64::MIN as i128
            } else {
                -(num_mag as i128)
            }
        } else {
            num_mag as i128
        };

        Ok(Rational {
            num: signed_num as i64,
            den: den_value as i64,
        })
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

fn main() {}