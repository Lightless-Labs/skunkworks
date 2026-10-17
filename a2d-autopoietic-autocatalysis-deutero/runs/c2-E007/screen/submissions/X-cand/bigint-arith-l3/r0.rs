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
    // Little-endian base-10 digits. Zero is represented by [0] and is never negative.
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

        let magnitude = &bytes[start..];

        if let Some(offset) = magnitude.iter().position(|&b| b != b'_' && !b.is_ascii_digit()) {
            return Err(Error::InvalidDigit { index: start + offset });
        }

        for i in 0..magnitude.len() {
            if magnitude[i] == b'_'
                && (i == 0
                    || i + 1 == magnitude.len()
                    || !magnitude[i - 1].is_ascii_digit()
                    || !magnitude[i + 1].is_ascii_digit())
            {
                return Err(Error::Separator { index: start + i });
            }
        }

        let first_digit = magnitude.iter().position(|&b| b != b'_').unwrap();
        let digit_count = magnitude.iter().filter(|&&b| b != b'_').count();

        if digit_count > 1 && magnitude[first_digit] == b'0' {
            return Err(Error::LeadingZero {
                index: start + first_digit,
            });
        }

        if digit_count > max_digits {
            let excess_number = max_digits + 1;
            let mut seen = 0;
            let mut index = start;
            for (i, &b) in magnitude.iter().enumerate() {
                if b != b'_' {
                    seen += 1;
                    if seen == excess_number {
                        index = start + i;
                        break;
                    }
                }
            }
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index,
            });
        }

        let mut digits = Vec::with_capacity(digit_count);
        for &b in magnitude.iter().rev() {
            if b != b'_' {
                digits.push(b - b'0');
            }
        }

        Ok(Self::from_parts(negative, digits))
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let natural_len = self.digits.len().max(1);
        let width = natural_len.max(min_digits);
        let mut magnitude = String::with_capacity(width + width / group.max(1));
        let padding = width - natural_len;

        for i in (0..width).rev() {
            if group > 0 && i + 1 < width && (i + 1) % group == 0 {
                magnitude.push('_');
            }
            if i >= natural_len {
                magnitude.push('0');
            } else {
                magnitude.push((b'0' + self.digits[i]) as char);
            }
        }

        if self.negative {
            let mut result = String::with_capacity(magnitude.len() + 1);
            result.push('-');
            result.push_str(&magnitude);
            result
        } else {
            magnitude
        }
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
                Ordering::Equal => Self::zero(),
                Ordering::Greater => Self::from_parts(
                    self.negative,
                    sub_magnitude(&self.digits, &rhs.digits),
                ),
                Ordering::Less => Self::from_parts(
                    rhs.negative,
                    sub_magnitude(&rhs.digits, &self.digits),
                ),
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

        let mut result = vec![0u8; self.digits.len() + rhs.digits.len()];
        for (i, &a) in self.digits.iter().enumerate() {
            let mut carry = 0u16;
            for (j, &b) in rhs.digits.iter().enumerate() {
                let k = i + j;
                let value = result[k] as u16 + a as u16 * b as u16 + carry;
                result[k] = (value % 10) as u8;
                carry = value / 10;
            }

            let mut k = i + rhs.digits.len();
            while carry > 0 {
                if k == result.len() {
                    result.push(0);
                }
                let value = result[k] as u16 + carry;
                result[k] = (value % 10) as u8;
                carry = value / 10;
                k += 1;
            }
        }

        Self::from_parts(self.negative != rhs.negative, result)
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }

        match cmp_magnitude(&self.digits, &rhs.digits) {
            Ordering::Less => return Ok((Self::zero(), self.clone())),
            Ordering::Equal => {
                return Ok((
                    Self::from_parts(self.negative != rhs.negative, vec![1]),
                    Self::zero(),
                ));
            }
            Ordering::Greater => {}
        }

        let mut quotient_be = Vec::with_capacity(self.digits.len());
        let mut remainder = vec![0u8];

        for &digit in self.digits.iter().rev() {
            remainder.insert(0, digit);
            normalize(&mut remainder);

            let mut qdigit = 0u8;
            while cmp_magnitude(&remainder, &rhs.digits) != Ordering::Less {
                remainder = sub_magnitude(&remainder, &rhs.digits);
                qdigit += 1;
            }
            quotient_be.push(qdigit);
        }

        quotient_be.reverse();
        let quotient = Self::from_parts(self.negative != rhs.negative, quotient_be);
        let remainder = Self::from_parts(self.negative, remainder);
        Ok((quotient, remainder))
    }

    fn zero() -> Self {
        Self {
            negative: false,
            digits: vec![0],
        }
    }

    fn is_zero(&self) -> bool {
        self.digits.len() == 1 && self.digits[0] == 0
    }

    fn from_parts(negative: bool, mut digits: Vec<u8>) -> Self {
        normalize(&mut digits);
        let is_zero = digits.len() == 1 && digits[0] == 0;
        Self {
            negative: negative && !is_zero,
            digits,
        }
    }
}

fn normalize(digits: &mut Vec<u8>) {
    while digits.len() > 1 && digits.last() == Some(&0) {
        digits.pop();
    }
    if digits.is_empty() {
        digits.push(0);
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

fn sub_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(a.len());
    let mut borrow = 0i16;
    for i in 0..a.len() {
        let mut digit = a[i] as i16 - borrow - b.get(i).copied().unwrap_or(0) as i16;
        if digit < 0 {
            digit += 10;
            borrow = 1;
        } else {
            borrow = 0;
        }
        result.push(digit as u8);
    }
    normalize(&mut result);
    result
}