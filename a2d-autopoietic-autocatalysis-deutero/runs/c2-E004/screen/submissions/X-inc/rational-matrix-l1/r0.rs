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

fn from_i128_parts(num: i128, den: i128) -> Result<Rational, RationalError> {
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
        from_i128_parts(num as i128, den as i128)
    }

    pub fn numerator(&self) -> i64 {
        self.num
    }

    pub fn denominator(&self) -> i64 {
        self.den
    }

    pub fn add(&self, rhs: &Self) -> Result<Self, RationalError> {
        let g = gcd(self.den as u128, rhs.den as u128) as i128;
        let left_scale = rhs.den as i128 / g;
        let right_scale = self.den as i128 / g;
        let n = self.num as i128 * left_scale + rhs.num as i128 * right_scale;
        let d = self.den as i128 * left_scale;
        from_i128_parts(n, d)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let g = gcd(self.den as u128, rhs.den as u128) as i128;
        let left_scale = rhs.den as i128 / g;
        let right_scale = self.den as i128 / g;
        let n = self.num as i128 * left_scale - rhs.num as i128 * right_scale;
        let d = self.den as i128 * left_scale;
        from_i128_parts(n, d)
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        let g1 = gcd(self.num.unsigned_abs() as u128, rhs.den as u128) as i128;
        let g2 = gcd(rhs.num.unsigned_abs() as u128, self.den as u128) as i128;
        let n = (self.num as i128 / g1) * (rhs.num as i128 / g2);
        let d = (self.den as i128 / g2) * (rhs.den as i128 / g1);
        from_i128_parts(n, d)
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        let n = self.num as i128 * rhs.den as i128;
        let d = self.den as i128 * rhs.num as i128;
        from_i128_parts(n, d)
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        if self.num == i64::MIN {
            return Err(RationalError::Overflow);
        }
        Ok(Rational {
            num: -self.num,
            den: self.den,
        })
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
        if bytes[0] == b'-' {
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
        let mut slash = None;
        let mut den_start = 0;
        let mut den_end = 0;

        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
            slash = Some(i);
            i += 1;
            den_start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == den_start {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
            den_end = i;
            if i < bytes.len() {
                return Err(RationalError::Parse {
                    at: i,
                    kind: ParseKind::InvalidByte,
                });
            }
        }

        if num_end - num_start >= 2 && bytes[num_start] == b'0' {
            return Err(RationalError::Parse {
                at: num_start + 1,
                kind: ParseKind::NonCanonical,
            });
        }
        if bytes[0] == b'-' && num_end - num_start == 1 && bytes[num_start] == b'0' {
            return Err(RationalError::Parse {
                at: 0,
                kind: ParseKind::NonCanonical,
            });
        }
        if slash.is_some() && den_end - den_start >= 2 && bytes[den_start] == b'0' {
            return Err(RationalError::Parse {
                at: den_start + 1,
                kind: ParseKind::NonCanonical,
            });
        }

        let negative = bytes[0] == b'-';
        let mut nmag: u128 = 0;
        for &b in &bytes[num_start..num_end] {
            nmag = nmag * 10 + (b - b'0') as u128;
            if nmag > (1u128 << 63) {
                return Err(RationalError::Parse {
                    at: 0,
                    kind: ParseKind::OutOfRange,
                });
            }
        }
        if (!negative && nmag > i64::MAX as u128) || (negative && nmag > (1u128 << 63)) {
            return Err(RationalError::Parse {
                at: 0,
                kind: ParseKind::OutOfRange,
            });
        }

        let mut dmag = 1u128;
        if slash.is_some() {
            dmag = 0;
            for &b in &bytes[den_start..den_end] {
                dmag = dmag * 10 + (b - b'0') as u128;
                if dmag > i64::MAX as u128 {
                    return Err(RationalError::Parse {
                        at: den_start,
                        kind: ParseKind::OutOfRange,
                    });
                }
            }
            if dmag == 0 {
                return Err(RationalError::Parse {
                    at: den_start,
                    kind: ParseKind::ZeroDenominator,
                });
            }
            if dmag == 1 {
                return Err(RationalError::Parse {
                    at: slash.unwrap(),
                    kind: ParseKind::NonCanonical,
                });
            }
            if nmag == 0 {
                return Err(RationalError::Parse {
                    at: slash.unwrap(),
                    kind: ParseKind::NonCanonical,
                });
            }
            if gcd(nmag, dmag) > 1 {
                return Err(RationalError::Parse {
                    at: slash.unwrap(),
                    kind: ParseKind::NonCanonical,
                });
            }
        }

        let n = if negative {
            if nmag == 1u128 << 63 {
                i64::MIN
            } else {
                -(nmag as i64)
            }
        } else {
            nmag as i64
        };

        Ok(Rational {
            num: n,
            den: dmag as i64,
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