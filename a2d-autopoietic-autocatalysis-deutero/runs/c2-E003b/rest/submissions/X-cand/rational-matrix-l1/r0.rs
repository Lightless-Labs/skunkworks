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

impl Rational {
    pub fn new(num: i64, den: i64) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        Self::from_i128(num as i128, den as i128)
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
        let len = bytes.len();
        if len == 0 {
            return Err(parse_error(0, ParseKind::Empty));
        }

        let mut i = 0;
        if bytes[i] == b'-' {
            i += 1;
        }

        let numerator_start = 0;
        let numerator_digits_start = i;
        while i < len && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == numerator_digits_start {
            return Err(parse_error(i, ParseKind::InvalidByte));
        }

        let numerator_end = i;
        let mut denominator_start = None;
        let mut denominator_end = None;

        if i < len && bytes[i] == b'/' {
            i += 1;
            let start = i;
            if i == len || !bytes[i].is_ascii_digit() {
                return Err(parse_error(i, ParseKind::InvalidByte));
            }
            while i < len && bytes[i].is_ascii_digit() {
                i += 1;
            }
            denominator_start = Some(start);
            denominator_end = Some(i);
        }

        if i < len {
            return Err(parse_error(i, ParseKind::InvalidByte));
        }

        let negative = bytes[0] == b'-';
        let numerator_token = &bytes[numerator_start..numerator_end];
        let numerator_digits = &bytes[numerator_digits_start..numerator_end];

        if numerator_digits.len() >= 2 && numerator_digits[0] == b'0' {
            return Err(parse_error(
                if negative { 2 } else { 1 },
                ParseKind::NonCanonical,
            ));
        }
        if negative && numerator_digits == b"0" {
            return Err(parse_error(0, ParseKind::NonCanonical));
        }

        if let (Some(start), Some(end)) = (denominator_start, denominator_end) {
            if end - start >= 2 && bytes[start] == b'0' {
                return Err(parse_error(start + 1, ParseKind::NonCanonical));
            }
        }

        let num_magnitude = parse_magnitude(numerator_digits);
        let numerator_limit = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        if num_magnitude > numerator_limit {
            return Err(parse_error(0, ParseKind::OutOfRange));
        }

        let num = if negative {
            if num_magnitude == (1u128 << 63) {
                i64::MIN
            } else {
                -(num_magnitude as i64)
            }
        } else {
            num_magnitude as i64
        };

        let Some(start) = denominator_start else {
            return Ok(Self { num, den: 1 });
        };
        let end = denominator_end.unwrap();
        let den_magnitude = parse_magnitude(&bytes[start..end]);
        if den_magnitude > i64::MAX as u128 {
            return Err(parse_error(start, ParseKind::OutOfRange));
        }
        if den_magnitude == 0 {
            return Err(parse_error(start, ParseKind::ZeroDenominator));
        }
        if den_magnitude == 1 {
            return Err(parse_error(start - 1, ParseKind::NonCanonical));
        }
        if num == 0 {
            return Err(parse_error(start - 1, ParseKind::NonCanonical));
        }
        if gcd_u128(num_magnitude, den_magnitude) > 1 {
            return Err(parse_error(start - 1, ParseKind::NonCanonical));
        }

        Ok(Self {
            num,
            den: den_magnitude as i64,
        })
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

fn parse_error(at: usize, kind: ParseKind) -> RationalError {
    RationalError::Parse { at, kind }
}

fn parse_magnitude(digits: &[u8]) -> u128 {
    digits
        .iter()
        .fold(0u128, |value, &digit| value * 10 + (digit - b'0') as u128)
}

fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}