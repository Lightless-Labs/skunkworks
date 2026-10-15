use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    negative: bool,
    // Little-endian decimal digits. Zero is represented as [0] and is never negative.
    digits: Vec<u8>,
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

        let (negative, magnitude_start) = match bytes[0] {
            b'+' => (false, 1),
            b'-' => (true, 1),
            _ => (false, 0),
        };

        if magnitude_start == bytes.len() {
            return Err(Error::SignOnly);
        }

        // Invalid bytes take precedence over all separator errors.
        for (index, &byte) in bytes.iter().enumerate().skip(magnitude_start) {
            if !byte.is_ascii_digit() && byte != b'_' {
                return Err(Error::InvalidDigit { index });
            }
        }

        // With invalid bytes ruled out, find the leftmost malformed separator.
        for index in magnitude_start..bytes.len() {
            if bytes[index] == b'_'
                && (index == magnitude_start
                    || index + 1 == bytes.len()
                    || !bytes[index - 1].is_ascii_digit()
                    || !bytes[index + 1].is_ascii_digit())
            {
                return Err(Error::Separator { index });
            }
        }

        let mut digit_count = 0usize;
        let mut first_digit = None;
        let mut first_excess_digit = None;
        let mut little_endian = Vec::new();

        for (index, &byte) in bytes.iter().enumerate().skip(magnitude_start) {
            if byte.is_ascii_digit() {
                digit_count += 1;
                first_digit.get_or_insert((byte, index));
                if digit_count == max_digits.saturating_add(1) {
                    first_excess_digit = Some(index);
                }
                little_endian.push(byte - b'0');
            }
        }

        if digit_count > 1 && first_digit.is_some_and(|(byte, _)| byte == b'0') {
            return Err(Error::LeadingZero {
                index: first_digit.unwrap().1,
            });
        }

        if digit_count > max_digits {
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: first_excess_digit.unwrap_or_else(|| {
                    first_digit.map_or(magnitude_start, |(_, index)| index)
                }),
            });
        }

        little_endian.reverse();
        while little_endian.len() > 1 && little_endian.last() == Some(&0) {
            little_endian.pop();
        }

        let is_zero = little_endian.len() == 1 && little_endian[0] == 0;
        Ok(Self {
            negative: negative && !is_zero,
            digits: little_endian,
        })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let magnitude_len = self.digits.len().max(min_digits).max(1);
        let padding = magnitude_len - self.digits.len();
        let mut output = String::new();

        if self.negative {
            output.push('-');
        }

        for position in 0..magnitude_len {
            if group > 0 && position > 0 && (magnitude_len - position) % group == 0 {
                output.push('_');
            }

            if position < padding {
                output.push('0');
            } else {
                let digit = self.digits[self.digits.len() - 1 - (position - padding)];
                output.push(char::from(b'0' + digit));
            }
        }

        output
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.negative, rhs.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => compare_magnitude(&self.digits, &rhs.digits),
            (true, true) => compare_magnitude(&rhs.digits, &self.digits),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        match (self.negative, rhs.negative) {
            (false, false) => Self::from_magnitude(
                add_magnitude(&self.digits, &rhs.digits),
                false,
            ),
            (true, true) => Self::from_magnitude(
                add_magnitude(&self.digits, &rhs.digits),
                true,
            ),
            _ => match compare_magnitude(&self.digits, &rhs.digits) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => Self::from_magnitude(
                    subtract_magnitude(&self.digits, &rhs.digits),
                    self.negative,
                ),
                Ordering::Less => Self::from_magnitude(
                    subtract_magnitude(&rhs.digits, &self.digits),
                    rhs.negative,
                ),
            },
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
        for (i, &left) in self.digits.iter().enumerate() {
            let mut carry = 0u16;
            for (j, &right) in rhs.digits.iter().enumerate() {
                let index = i + j;
                let value = u16::from(result[index])
                    + u16::from(left) * u16::from(right)
                    + carry;
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

        Self::from_magnitude(result, self.negative != rhs.negative)
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

    fn from_magnitude(mut digits: Vec<u8>, negative: bool) -> Self {
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

fn compare_magnitude(left: &[u8], right: &[u8]) -> Ordering {
    match left.len().cmp(&right.len()) {
        Ordering::Equal => {
            for (&a, &b) in left.iter().rev().zip(right.iter().rev()) {
                match a.cmp(&b) {
                    Ordering::Equal => {}
                    ordering => return ordering,
                }
            }
            Ordering::Equal
        }
        ordering => ordering,
    }
}

fn add_magnitude(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(left.len().max(right.len()) + 1);
    let mut carry = 0u8;

    for index in 0..left.len().max(right.len()) {
        let sum = left.get(index).copied().unwrap_or(0)
            + right.get(index).copied().unwrap_or(0)
            + carry;
        result.push(sum % 10);
        carry = sum / 10;
    }

    if carry != 0 {
        result.push(carry);
    }

    result
}

// Requires left magnitude >= right magnitude.
fn subtract_magnitude(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(left.len());
    let mut borrow = 0i16;

    for (index, &left_digit) in left.iter().enumerate() {
        let mut digit = i16::from(left_digit)
            - borrow
            - i16::from(right.get(index).copied().unwrap_or(0));
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

fn main() {}