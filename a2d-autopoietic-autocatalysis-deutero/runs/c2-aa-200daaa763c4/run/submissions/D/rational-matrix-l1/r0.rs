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

        let divisor = gcd(n.unsigned_abs(), d as u128) as i128;
        n /= divisor;
        d /= divisor;

        if n < i64::MIN as i128 || n > i64::MAX as i128 || d > i64::MAX as i128 {
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
        Self::from_i128(
            self.num as i128 * rhs.num as i128,
            self.den as i128 * rhs.den as i128,
        )
    }

    pub fn div(&self, rhs: &Self) -> Result<Self, RationalError> {
        if rhs.num == 0 {
            return Err(RationalError::DivideByZero);
        }
        Self::from_i128(
            self.num as i128 * rhs.den as i128,
            self.den as i128 * rhs.num as i128,
        )
    }

    pub fn neg(&self) -> Result<Self, RationalError> {
        let n = -(self.num as i128);
        Self::from_i128(n, self.den as i128)
    }

    pub fn parse(text: &str) -> Result<Self, RationalError> {
        let bytes = text.as_bytes();
        let fail = |at, kind| Err(RationalError::Parse { at, kind });

        if bytes.is_empty() {
            return fail(0, ParseKind::Empty);
        }

        let mut i = 0;
        if bytes[i] == b'-' {
            i += 1;
        }

        let numerator_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == numerator_start {
            return fail(i, ParseKind::InvalidByte);
        }

        let numerator_end = i;
        let mut denominator_start = None;
        let mut denominator_end = None;

        if i < bytes.len() {
            if bytes[i] != b'/' {
                return fail(i, ParseKind::InvalidByte);
            }
            i += 1;
            denominator_start = Some(i);
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == denominator_start.unwrap() {
                return fail(i, ParseKind::InvalidByte);
            }
            denominator_end = Some(i);
            if i < bytes.len() {
                return fail(i, ParseKind::InvalidByte);
            }
        }

        let negative = bytes[0] == b'-';
        let numerator_digits = &bytes[numerator_start..numerator_end];

        if numerator_digits.len() >= 2 && numerator_digits[0] == b'0' {
            return fail(numerator_start + 1, ParseKind::NonCanonical);
        }
        if negative && numerator_digits == b"0" {
            return fail(0, ParseKind::NonCanonical);
        }

        if let (Some(ds), Some(de)) = (denominator_start, denominator_end) {
            if de - ds >= 2 && bytes[ds] == b'0' {
                return fail(ds + 1, ParseKind::NonCanonical);
            }
        }

        let mut magnitude: u128 = 0;
        let limit = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        for &digit in numerator_digits {
            magnitude = magnitude * 10 + (digit - b'0') as u128;
            if magnitude > limit {
                return fail(0, ParseKind::OutOfRange);
            }
        }

        let mut den_magnitude = 1u128;
        if let (Some(ds), Some(de)) = (denominator_start, denominator_end) {
            den_magnitude = 0;
            for &digit in &bytes[ds..de] {
                den_magnitude = den_magnitude * 10 + (digit - b'0') as u128;
                if den_magnitude > i64::MAX as u128 {
                    return fail(ds, ParseKind::OutOfRange);
                }
            }

            if den_magnitude == 0 {
                return fail(ds, ParseKind::ZeroDenominator);
            }

            if den_magnitude == 1 {
                return fail(ds - 1, ParseKind::NonCanonical);
            }

            if magnitude == 0 {
                return fail(ds - 1, ParseKind::NonCanonical);
            }

            if gcd(magnitude, den_magnitude) > 1 {
                return fail(ds - 1, ParseKind::NonCanonical);
            }
        }

        let signed_num = if negative {
            -(magnitude as i128)
        } else {
            magnitude as i128
        };
        Self::from_i128(signed_num, den_magnitude as i128)
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