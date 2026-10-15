use std::cmp::Ordering;
use std::fmt;

#[derive(Clone)]
pub struct Big {
    negative: bool,
    // Little-endian decimal digits. Zero is represented by [0] and is never negative.
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

impl PartialEq for Big {
    fn eq(&self, other: &Self) -> bool {
        self.negative == other.negative && self.digits == other.digits
    }
}

impl Eq for Big {}

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

        let mut digit_count = 0usize;
        let mut first_digit_index = None;
        let mut previous_was_digit = false;
        let mut first_bad_separator = None;

        // Scan all magnitude bytes for invalid bytes, while recording separator
        // and digit metadata. Invalid bytes always take precedence.
        let mut first_invalid = None;
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            match byte {
                b'0'..=b'9' => {
                    digit_count = digit_count.saturating_add(1);
                    if first_digit_index.is_none() {
                        first_digit_index = Some(index);
                    }
                    previous_was_digit = true;
                }
                b'_' => {
                    let next_is_digit = bytes
                        .get(index + 1)
                        .is_some_and(|b| b.is_ascii_digit());
                    if (!previous_was_digit || !next_is_digit)
                        && first_bad_separator.is_none()
                    {
                        first_bad_separator = Some(index);
                    }
                    previous_was_digit = false;
                }
                _ => {
                    if first_invalid.is_none() {
                        first_invalid = Some(index);
                    }
                    previous_was_digit = false;
                }
            }
        }

        if let Some(index) = first_invalid {
            return Err(Error::InvalidDigit { index });
        }
        if let Some(index) = first_bad_separator {
            return Err(Error::Separator { index });
        }

        let first_digit_index = first_digit_index.expect("nonempty magnitude has a digit");
        if digit_count > 1 && bytes[start] == b'0' {
            return Err(Error::LeadingZero { index: start });
        }

        if digit_count > max_digits {
            let excess_number = max_digits.saturating_add(1);
            let mut seen = 0usize;
            for (index, &byte) in bytes.iter().enumerate().skip(start) {
                if byte.is_ascii_digit() {
                    seen += 1;
                    if seen == excess_number {
                        return Err(Error::TooManyDigits {
                            limit: max_digits,
                            index,
                        });
                    }
                }
            }
            // If max_digits is usize::MAX, digit_count cannot exceed it.
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: first_digit_index,
            });
        }

        let mut digits = Vec::with_capacity(digit_count);
        for &byte in bytes.iter().skip(start) {
            if byte.is_ascii_digit() {
                digits.push(byte - b'0');
            }
        }
        digits.reverse();

        let is_zero = digits.iter().all(|&digit| digit == 0);
        Ok(Self {
            negative: negative && !is_zero,
            digits,
        })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let magnitude_len = self.digits.len().max(1);
        let width = magnitude_len.max(min_digits);
        let padding = width - magnitude_len;

        let mut out = String::new();
        if self.negative {
            out.push('-');
        }

        for position in 0..width {
            if group > 0 && position > 0 && (width - position) % group == 0 {
                out.push('_');
            }
            let digit = if position < padding {
                0
            } else {
                self.digits[magnitude_len - 1 - (position - padding)]
            };
            out.push(char::from(b'0' + digit));
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
            let digits = add_magnitude(&self.digits, &rhs.digits);
            Self::from_parts(self.negative, digits)
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
                let index = i + j;
                let value = u16::from(result[index]) + u16::from(a) * u16::from(b) + carry;
                result[index] = (value % 10) as u8;
                carry = value / 10;
            }

            let mut index = i + rhs.digits.len();
            while carry > 0 {
                if index == result.len() {
                    result.push(0);
                }
                let value = u16::from(result[index]) + carry;
                result[index] = (value % 10) as u8;
                carry = value / 10;
                index += 1;
            }
        }

        Self::from_parts(self.negative != rhs.negative, result)
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

fn cmp_magnitude(lhs: &[u8], rhs: &[u8]) -> Ordering {
    match lhs.len().cmp(&rhs.len()) {
        Ordering::Equal => lhs.iter().rev().cmp(rhs.iter().rev()),
        other => other,
    }
}

fn add_magnitude(lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(lhs.len().max(rhs.len()) + 1);
    let mut carry = 0u8;
    for i in 0..lhs.len().max(rhs.len()) {
        let sum = lhs.get(i).copied().unwrap_or(0)
            + rhs.get(i).copied().unwrap_or(0)
            + carry;
        result.push(sum % 10);
        carry = sum / 10;
    }
    if carry > 0 {
        result.push(carry);
    }
    result
}

// Requires lhs magnitude >= rhs magnitude.
fn sub_magnitude(lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(lhs.len());
    let mut borrow = 0i8;
    for (i, &digit) in lhs.iter().enumerate() {
        let mut value = i16::from(digit) - i16::from(borrow);
        let subtrahend = i16::from(rhs.get(i).copied().unwrap_or(0));
        if value < subtrahend {
            value += 10;
            borrow = 1;
        } else {
            borrow = 0;
        }
        result.push((value - subtrahend) as u8);
    }
    while result.len() > 1 && result.last() == Some(&0) {
        result.pop();
    }
    result
}

fn main() {}