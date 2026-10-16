use std::cmp::Ordering;
use std::fmt;

#[derive(Clone, PartialEq, Eq)]
pub struct Big {
    negative: bool,
    // Little-endian base-10 digits. Zero is represented by [0] and is never negative.
    digits: Vec<u8>,
}

impl fmt::Debug for Big {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Big")
            .field("negative", &self.negative)
            .field("digits", &self.digits)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Empty,
    SignOnly,
    InvalidDigit { index: usize },
    Separator { index: usize },
    LeadingZero { index: usize },
    TooManyDigits { limit: usize, index: usize },
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

        // Invalid bytes take precedence over malformed separators.
        if let Some(index) = magnitude.iter().position(|&b| b != b'_' && !b.is_ascii_digit()) {
            return Err(Error::InvalidDigit {
                index: start + index,
            });
        }

        // Validate separators after the full invalid-byte scan.
        for (i, &byte) in magnitude.iter().enumerate() {
            if byte == b'_'
                && (i == 0
                    || i + 1 == magnitude.len()
                    || !magnitude[i - 1].is_ascii_digit()
                    || !magnitude[i + 1].is_ascii_digit())
            {
                return Err(Error::Separator { index: start + i });
            }
        }

        let digit_count = magnitude.iter().filter(|&&b| b.is_ascii_digit()).count();
        let first_digit_offset = magnitude
            .iter()
            .position(|&b| b.is_ascii_digit())
            .expect("validated magnitude contains a digit");

        if digit_count > 1 && magnitude[first_digit_offset] == b'0' {
            return Err(Error::LeadingZero {
                index: start + first_digit_offset,
            });
        }

        if digit_count > max_digits {
            let excess_ordinal = max_digits;
            let excess_offset = magnitude
                .iter()
                .enumerate()
                .filter(|(_, b)| b.is_ascii_digit())
                .nth(excess_ordinal)
                .map(|(i, _)| i)
                .expect("digit count exceeds the limit");
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: start + excess_offset,
            });
        }

        let mut digits = Vec::with_capacity(digit_count);
        for &byte in magnitude.iter().rev() {
            if byte.is_ascii_digit() {
                digits.push(byte - b'0');
            }
        }
        while digits.len() > 1 && digits.last() == Some(&0) {
            digits.pop();
        }

        let is_zero = digits.len() == 1 && digits[0] == 0;
        Ok(Self {
            negative: negative && !is_zero,
            digits,
        })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let magnitude_len = self.digits.len().max(min_digits).max(1);
        let padding = magnitude_len - self.digits.len();

        let mut out = String::new();
        if self.negative {
            out.push('-');
        }

        for position in (0..magnitude_len).rev() {
            if group > 0 && position < magnitude_len - 1 && (position + 1) % group == 0 {
                out.push('_');
            }

            if position >= self.digits.len() {
                out.push('0');
            } else {
                out.push((b'0' + self.digits[position]) as char);
            }
        }

        let _ = padding;
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
            let digits = add_magnitudes(&self.digits, &rhs.digits);
            Self::from_parts(self.negative, digits)
        } else {
            match cmp_magnitude(&self.digits, &rhs.digits) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => Self::from_parts(
                    self.negative,
                    subtract_magnitudes(&self.digits, &rhs.digits),
                ),
                Ordering::Less => Self::from_parts(
                    rhs.negative,
                    subtract_magnitudes(&rhs.digits, &self.digits),
                ),
            }
        }
    }

    pub fn sub(&self, rhs: &Self) -> Self {
        self.add(&rhs.neg())
    }

    pub fn neg(&self) -> Self {
        let mut result = self.clone();
        if !result.is_zero() {
            result.negative = !result.negative;
        }
        result
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
                let index = i + j;
                let value = result[index] as u16 + a as u16 * b as u16 + carry;
                result[index] = (value % 10) as u8;
                carry = value / 10;
            }

            let mut index = i + rhs.digits.len();
            while carry > 0 {
                if index == result.len() {
                    result.push(0);
                }
                let value = result[index] as u16 + carry;
                result[index] = (value % 10) as u8;
                carry = value / 10;
                index += 1;
            }
        }

        Self::from_parts(self.negative ^ rhs.negative, result)
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
        while digits.len() > 1 && digits.last() == Some(&0) {
            digits.pop();
        }
        let is_zero = digits.len() == 1 && digits[0] == 0;
        Self {
            negative: negative && !is_zero,
            digits,
        }
    }
}

fn cmp_magnitude(a: &[u8], b: &[u8]) -> Ordering {
    match a.len().cmp(&b.len()) {
        Ordering::Equal => {
            for (x, y) in a.iter().rev().zip(b.iter().rev()) {
                match x.cmp(y) {
                    Ordering::Equal => {}
                    other => return other,
                }
            }
            Ordering::Equal
        }
        other => other,
    }
}

fn add_magnitudes(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(a.len().max(b.len()) + 1);
    let mut carry = 0u8;
    for i in 0..a.len().max(b.len()) {
        let sum = a.get(i).copied().unwrap_or(0) + b.get(i).copied().unwrap_or(0) + carry;
        result.push(sum % 10);
        carry = sum / 10;
    }
    if carry > 0 {
        result.push(carry);
    }
    result
}

// Precondition: a >= b.
fn subtract_magnitudes(a: &[u8], b: &[u8]) -> Vec<u8> {
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
    while result.len() > 1 && result.last() == Some(&0) {
        result.pop();
    }
    result
}