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

impl Rational {
    pub fn new(num: i64, den: i64) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        Self::from_i128(num as i128, den as i128)
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

        let gcd = gcd_u128(num.unsigned_abs(), den as u128) as i128;
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

    pub fn numerator(&self) -> i64 {
        self.num
    }

    pub fn denominator(&self) -> i64 {
        self.den
    }

    pub fn add(&self, rhs: &Self) -> Result<Self, RationalError> {
        let num = self.num as i128 * rhs.den as i128
            + rhs.num as i128 * self.den as i128;
        let den = self.den as i128 * rhs.den as i128;
        Self::from_i128(num, den)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, RationalError> {
        let num = self.num as i128 * rhs.den as i128
            - rhs.num as i128 * self.den as i128;
        let den = self.den as i128 * rhs.den as i128;
        Self::from_i128(num, den)
    }

    pub fn mul(&self, rhs: &Self) -> Result<Self, RationalError> {
        let num = self.num as i128 * rhs.num as i128;
        let den = self.den as i128 * rhs.den as i128;
        Self::from_i128(num, den)
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        let num = self.num as i128 * rhs.den as i128;
        let den = self.den as i128 * rhs.num as i128;
        Self::from_i128(num, den)
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        Self::from_i128(-(self.num as i128), self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(parse_error(0, ParseKind::Empty));
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
            return Err(parse_error(i, ParseKind::InvalidByte));
        }

        let num_end = i;
        let mut den_start = None;
        let mut den_end = None;

        if i < bytes.len() {
            if bytes[i] != b'/' {
                return Err(parse_error(i, ParseKind::InvalidByte));
            }
            i += 1;
            let start = i;
            den_start = Some(start);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return Err(parse_error(i, ParseKind::InvalidByte));
            }
            den_end = Some(i);
            if i < bytes.len() {
                return Err(parse_error(i, ParseKind::InvalidByte));
            }
        }

        let negative = bytes[0] == b'-';
        let numerator_digits_start = if negative { 1 } else { 0 };
        let numerator_digits = num_end - numerator_digits_start;

        if numerator_digits >= 2 && bytes[numerator_digits_start] == b'0' {
            return Err(parse_error(
                numerator_digits_start + 1,
                ParseKind::NonCanonical,
            ));
        }
        if negative && numerator_digits == 1 && bytes[1] == b'0' {
            return Err(parse_error(0, ParseKind::NonCanonical));
        }

        if let (Some(start), Some(end)) = (den_start, den_end) {
            if end - start >= 2 && bytes[start] == b'0' {
                return Err(parse_error(start + 1, ParseKind::NonCanonical));
            }
        }

        let numerator_magnitude = parse_magnitude(&bytes[numerator_digits_start..num_end]);
        let numerator_limit = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        if numerator_magnitude > numerator_limit {
            return Err(parse_error(0, ParseKind::OutOfRange));
        }

        let denominator_magnitude = if let (Some(start), Some(end)) = (den_start, den_end) {
            let magnitude = parse_magnitude(&bytes[start..end]);
            if magnitude > i64::MAX as u128 {
                return Err(parse_error(start, ParseKind::OutOfRange));
            }
            Some(magnitude)
        } else {
            None
        };

        if let (Some(start), Some(magnitude)) = (den_start, denominator_magnitude) {
            if magnitude == 0 {
                return Err(parse_error(start, ParseKind::ZeroDenominator));
            }
        }

        if let (Some(start), Some(magnitude)) = (den_start, denominator_magnitude) {
            let slash = start - 1;
            if magnitude == 1 {
                return Err(parse_error(slash, ParseKind::NonCanonical));
            }
            if numerator_magnitude == 0 {
                return Err(parse_error(slash, ParseKind::NonCanonical));
            }
            if gcd_u128(numerator_magnitude, magnitude) > 1 {
                return Err(parse_error(slash, ParseKind::NonCanonical));
            }
        }

        let numerator = if negative {
            if numerator_magnitude == (1u128 << 63) {
                i64::MIN
            } else {
                -(numerator_magnitude as i64)
            }
        } else {
            numerator_magnitude as i64
        };

        let denominator = denominator_magnitude
            .map(|m| m as i64)
            .unwrap_or(1);

        Ok(Self {
            num: numerator,
            den: denominator,
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

fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

fn parse_magnitude(digits: &[u8]) -> u128 {
    let mut value = 0u128;
    for &byte in digits {
        value = value * 10 + (byte - b'0') as u128;
    }
    value
}

fn parse_error(at: usize, kind: ParseKind) -> RationalError {
    RationalError::Parse { at, kind }
}

fn main() {}