use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Empty,
    SignOnly,
    InvalidDigit { index: usize },
    Separator { index: usize },
    LeadingZero { index: usize },
    TooManyDigits { limit: usize, index: usize },
    DivisionByZero,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    negative: bool,
    // Little-endian base-10 digits. Zero is represented as [0] and is never negative.
    digits: Vec<u8>,
}

impl Big {
    pub fn parse(text: &str, max_digits: usize) -> Result<Self, Error> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(Error::Empty);
        }

        let (negative, start) = match bytes[0] {
            b'+' => (false, 1),
            b'-' => (true, 1),
            _ => (false, 0),
        };
        if start == bytes.len() {
            return Err(Error::SignOnly);
        }

        let mut invalid = None;
        let mut digit_count = 0usize;
        let mut first_digit = None;

        for (i, &b) in bytes.iter().enumerate().skip(start) {
            if b == b'_' {
                continue;
            }
            if !b.is_ascii_digit() {
                if invalid.is_none() {
                    invalid = Some(i);
                }
            } else {
                digit_count += 1;
                if first_digit.is_none() {
                    first_digit = Some((i, b));
                }
            }
        }
        if let Some(index) = invalid {
            return Err(Error::InvalidDigit { index });
        }

        for i in start..bytes.len() {
            if bytes[i] == b'_'
                && (i == start
                    || i + 1 == bytes.len()
                    || !bytes[i - 1].is_ascii_digit()
                    || !bytes[i + 1].is_ascii_digit())
            {
                return Err(Error::Separator { index: i });
            }
        }

        if digit_count > 1 {
            let (index, first) = first_digit.expect("a nonempty magnitude has a digit");
            if first == b'0' {
                return Err(Error::LeadingZero { index });
            }
        }

        if digit_count > max_digits {
            let wanted = max_digits.saturating_add(1);
            let mut seen = 0usize;
            for (i, &b) in bytes.iter().enumerate().skip(start) {
                if b.is_ascii_digit() {
                    seen += 1;
                    if seen == wanted {
                        return Err(Error::TooManyDigits {
                            limit: max_digits,
                            index: i,
                        });
                    }
                }
            }
        }

        let mut digits = Vec::with_capacity(digit_count.max(1));
        for &b in bytes[start..].iter().rev() {
            if b.is_ascii_digit() {
                digits.push(b - b'0');
            }
        }
        if digits.is_empty() {
            digits.push(0);
        }
        Ok(Self::from_parts(negative, digits))
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let magnitude_len = self.digits.len().max(1);
        let width = magnitude_len.max(min_digits);
        let mut out = String::with_capacity(width + width / group.max(1) + 1);

        if self.negative {
            out.push('-');
        }

        for pos in 0..width {
            if group > 0 && pos > 0 && (width - pos) % group == 0 {
                out.push('_');
            }
            let from_right = width - 1 - pos;
            if from_right >= self.digits.len() {
                out.push('0');
            } else {
                out.push(char::from(b'0' + self.digits[from_right]));
            }
        }
        out
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.negative, rhs.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => cmp_magnitude(&self.digits, &rhs.digits),
            (true, true) => cmp_magnitude(&rhs.digits, &self.digits),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            Self::from_parts(
                self.negative,
                add_magnitude(&self.digits, &rhs.digits),
            )
        } else {
            match cmp_magnitude(&self.digits, &rhs.digits) {
                Ordering::Greater => Self::from_parts(
                    self.negative,
                    sub_magnitude(&self.digits, &rhs.digits),
                ),
                Ordering::Less => Self::from_parts(
                    rhs.negative,
                    sub_magnitude(&rhs.digits, &self.digits),
                ),
                Ordering::Equal => Self::zero(),
            }
        }
    }

    pub fn sub(&self, rhs: &Self) -> Self {
        self.add(&rhs.neg())
    }

    pub fn neg(&self) -> Self {
        if self.is_zero() {
            Self::zero()
        } else {
            Self {
                negative: !self.negative,
                digits: self.digits.clone(),
            }
        }
    }

    pub fn signum(&self) -> i8 {
        if self.is_zero() {
            0
        } else if self.negative {
            -1
        } else {
            1
        }
    }

    pub fn mul(&self, rhs: &Self) -> Self {
        if self.is_zero() || rhs.is_zero() {
            return Self::zero();
        }
        Self::from_parts(
            self.negative ^ rhs.negative,
            mul_magnitude(&self.digits, &rhs.digits),
        )
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }

        let (quotient, remainder) = div_rem_magnitude(&self.digits, &rhs.digits);
        Ok((
            Self::from_parts(self.negative ^ rhs.negative, quotient),
            Self::from_parts(self.negative, remainder),
        ))
    }

    fn zero() -> Self {
        Self {
            negative: false,
            digits: vec![0],
        }
    }

    fn from_parts(negative: bool, digits: Vec<u8>) -> Self {
        let mut value = Self {
            negative,
            digits: normalize(digits),
        };
        if value.is_zero() {
            value.negative = false;
        }
        value
    }

    fn is_zero(&self) -> bool {
        self.digits.len() == 1 && self.digits[0] == 0
    }
}

fn normalize(mut digits: Vec<u8>) -> Vec<u8> {
    while digits.len() > 1 && digits.last() == Some(&0) {
        digits.pop();
    }
    if digits.is_empty() {
        vec![0]
    } else {
        digits
    }
}

fn cmp_magnitude(a: &[u8], b: &[u8]) -> Ordering {
    match a.len().cmp(&b.len()) {
        Ordering::Equal => {
            for i in (0..a.len()).rev() {
                match a[i].cmp(&b[i]) {
                    Ordering::Equal => {}
                    other => return other,
                }
            }
            Ordering::Equal
        }
        other => other,
    }
}

fn add_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(a.len().max(b.len()) + 1);
    let mut carry = 0u8;
    for i in 0..a.len().max(b.len()) {
        let sum = a.get(i).copied().unwrap_or(0) + b.get(i).copied().unwrap_or(0) + carry;
        result.push(sum % 10);
        carry = sum / 10;
    }
    if carry != 0 {
        result.push(carry);
    }
    result
}

// Requires a >= b.
fn sub_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(a.len());
    let mut borrow = 0i8;
    for i in 0..a.len() {
        let mut digit = a[i] as i8 - borrow - b.get(i).copied().unwrap_or(0) as i8;
        if digit < 0 {
            digit += 10;
            borrow = 1;
        } else {
            borrow = 0;
        }
        result.push(digit as u8);
    }
    normalize(result)
}

fn mul_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut result = vec![0u8; a.len() + b.len()];
    for (i, &ad) in a.iter().enumerate() {
        let mut carry = 0u16;
        for (j, &bd) in b.iter().enumerate() {
            let k = i + j;
            let value = result[k] as u16 + ad as u16 * bd as u16 + carry;
            result[k] = (value % 10) as u8;
            carry = value / 10;
        }
        let mut k = i + b.len();
        while carry > 0 {
            let value = result[k] as u16 + carry;
            result[k] = (value % 10) as u8;
            carry = value / 10;
            k += 1;
        }
    }
    normalize(result)
}

fn mul_magnitude_digit(a: &[u8], digit: u8) -> Vec<u8> {
    if digit == 0 {
        return vec![0];
    }
    let mut result = Vec::with_capacity(a.len() + 1);
    let mut carry = 0u16;
    for &value in a {
        let product = value as u16 * digit as u16 + carry;
        result.push((product % 10) as u8);
        carry = product / 10;
    }
    while carry > 0 {
        result.push((carry % 10) as u8);
        carry /= 10;
    }
    normalize(result)
}

// Magnitudes are little-endian.
fn div_rem_magnitude(dividend: &[u8], divisor: &[u8]) -> (Vec<u8>, Vec<u8>) {
    if cmp_magnitude(dividend, divisor) == Ordering::Less {
        return (vec![0], dividend.to_vec());
    }

    let mut quotient_be = Vec::with_capacity(dividend.len());
    let mut remainder = vec![0u8];

    for &digit in dividend.iter().rev() {
        if remainder.len() == 1 && remainder[0] == 0 {
            remainder[0] = digit;
        } else {
            remainder.insert(0, digit);
        }
        remainder = normalize(remainder);

        let mut qdigit = 0u8;
        while qdigit < 9
            && cmp_magnitude(&remainder, &mul_magnitude_digit(divisor, qdigit + 1))
                != Ordering::Less
        {
            qdigit += 1;
        }
        if qdigit != 0 {
            remainder = sub_magnitude(
                &remainder,
                &mul_magnitude_digit(divisor, qdigit),
            );
        }
        quotient_be.push(qdigit);
    }

    quotient_be.reverse();
    (normalize(quotient_be), normalize(remainder))
}