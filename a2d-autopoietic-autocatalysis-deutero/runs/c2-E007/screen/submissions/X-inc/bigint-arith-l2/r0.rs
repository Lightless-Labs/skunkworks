use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Empty,
    SignOnly,
    InvalidDigit { index: usize },
    Separator { index: usize },
    LeadingZero { index: usize },
    TooManyDigits { limit: usize, index: usize },
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
            b'-' => (true, 1),
            b'+' => (false, 1),
            _ => (false, 0),
        };

        if start == bytes.len() {
            return Err(Error::SignOnly);
        }

        // Invalid bytes take precedence over all separator errors.
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' && !byte.is_ascii_digit() {
                return Err(Error::InvalidDigit { index });
            }
        }

        // Check every underscore after the invalid-byte scan has succeeded.
        let mut first_separator_error = None;
        for index in start..bytes.len() {
            if bytes[index] == b'_'
                && (index == start
                    || index + 1 == bytes.len()
                    || !bytes[index - 1].is_ascii_digit()
                    || !bytes[index + 1].is_ascii_digit())
            {
                first_separator_error = Some(index);
                break;
            }
        }
        if let Some(index) = first_separator_error {
            return Err(Error::Separator { index });
        }

        let first_digit = (start..bytes.len())
            .find(|&index| bytes[index] != b'_')
            .expect("a nonempty signed magnitude has a digit");
        let digit_count = bytes[start..].iter().filter(|&&byte| byte != b'_').count();

        if digit_count > 1 && bytes[first_digit] == b'0' {
            return Err(Error::LeadingZero { index: first_digit });
        }

        if digit_count > max_digits {
            let excess_digit_number = max_digits;
            let mut seen = 0;
            for index in start..bytes.len() {
                if bytes[index] != b'_' {
                    seen += 1;
                    if seen == excess_digit_number + 1 {
                        return Err(Error::TooManyDigits {
                            limit: max_digits,
                            index,
                        });
                    }
                }
            }
        }

        let mut digits = Vec::with_capacity(digit_count);
        for &byte in bytes[start..].iter().rev() {
            if byte != b'_' {
                digits.push(byte - b'0');
            }
        }

        let is_zero = digits.iter().all(|&digit| digit == 0);
        Ok(Self {
            negative: negative && !is_zero,
            digits,
        })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let magnitude_len = self.digits.len().max(1);
        let padded_len = magnitude_len.max(min_digits);
        let mut magnitude = String::with_capacity(
            padded_len + if group > 0 { padded_len.saturating_sub(1) / group } else { 0 },
        );

        let padding = padded_len - magnitude_len;
        for position in 0..padded_len {
            if group > 0 && position > 0 && (padded_len - position) % group == 0 {
                magnitude.push('_');
            }

            if position < padding {
                magnitude.push('0');
            } else {
                let digit_index = magnitude_len - 1 - (position - padding);
                magnitude.push(char::from(b'0' + self.digits[digit_index]));
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
            (false, false) => compare_magnitudes(&self.digits, &rhs.digits),
            (true, true) => compare_magnitudes(&rhs.digits, &self.digits),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            Self::from_magnitude(
                self.negative,
                add_magnitudes(&self.digits, &rhs.digits),
            )
        } else {
            match compare_magnitudes(&self.digits, &rhs.digits) {
                Ordering::Greater => Self::from_magnitude(
                    self.negative,
                    subtract_magnitudes(&self.digits, &rhs.digits),
                ),
                Ordering::Less => Self::from_magnitude(
                    rhs.negative,
                    subtract_magnitudes(&rhs.digits, &self.digits),
                ),
                Ordering::Equal => Self::zero(),
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

        Self::from_magnitude(
            self.negative ^ rhs.negative,
            multiply_magnitudes(&self.digits, &rhs.digits),
        )
    }

    fn zero() -> Self {
        Self {
            negative: false,
            digits: vec![0],
        }
    }

    fn from_magnitude(negative: bool, mut digits: Vec<u8>) -> Self {
        while digits.len() > 1 && digits.last() == Some(&0) {
            digits.pop();
        }
        let is_zero = digits.len() == 1 && digits[0] == 0;
        Self {
            negative: negative && !is_zero,
            digits,
        }
    }

    fn is_zero(&self) -> bool {
        self.digits.len() == 1 && self.digits[0] == 0
    }
}

fn compare_magnitudes(lhs: &[u8], rhs: &[u8]) -> Ordering {
    match lhs.len().cmp(&rhs.len()) {
        Ordering::Equal => {
            for (&left, &right) in lhs.iter().rev().zip(rhs.iter().rev()) {
                match left.cmp(&right) {
                    Ordering::Equal => {}
                    ordering => return ordering,
                }
            }
            Ordering::Equal
        }
        ordering => ordering,
    }
}

fn add_magnitudes(lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(lhs.len().max(rhs.len()) + 1);
    let mut carry = 0u8;

    for index in 0..lhs.len().max(rhs.len()) {
        let sum = lhs.get(index).copied().unwrap_or(0)
            + rhs.get(index).copied().unwrap_or(0)
            + carry;
        result.push(sum % 10);
        carry = sum / 10;
    }

    if carry != 0 {
        result.push(carry);
    }
    result
}

// Requires lhs >= rhs.
fn subtract_magnitudes(lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(lhs.len());
    let mut borrow = 0i8;

    for index in 0..lhs.len() {
        let mut difference = lhs[index] as i8
            - rhs.get(index).copied().unwrap_or(0) as i8
            - borrow;
        if difference < 0 {
            difference += 10;
            borrow = 1;
        } else {
            borrow = 0;
        }
        result.push(difference as u8);
    }

    while result.len() > 1 && result.last() == Some(&0) {
        result.pop();
    }
    result
}

fn multiply_magnitudes(lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
    let mut result = vec![0u8; lhs.len() + rhs.len()];

    for (left_index, &left_digit) in lhs.iter().enumerate() {
        let mut carry = 0u16;
        for (right_index, &right_digit) in rhs.iter().enumerate() {
            let index = left_index + right_index;
            let product = result[index] as u16
                + left_digit as u16 * right_digit as u16
                + carry;
            result[index] = (product % 10) as u8;
            carry = product / 10;
        }

        let mut index = left_index + rhs.len();
        while carry != 0 {
            if index == result.len() {
                result.push(0);
            }
            let sum = result[index] as u16 + carry;
            result[index] = (sum % 10) as u8;
            carry = sum / 10;
            index += 1;
        }
    }

    while result.len() > 1 && result.last() == Some(&0) {
        result.pop();
    }
    result
}