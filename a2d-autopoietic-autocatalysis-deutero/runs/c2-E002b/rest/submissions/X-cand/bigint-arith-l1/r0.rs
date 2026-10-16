use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Empty,
    SignOnly,
    InvalidDigit { index: usize },
    Separator { index: usize },
    LeadingZero { index: usize },
    TooManyDigits { limit: usize, index: usize },
    Underflow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    // Little-endian base-10 digits. Zero is represented as [0].
    digits: Vec<u8>,
}

impl Big {
    pub fn parse(text: &str, max_digits: usize) -> Result<Self, Error> {
        let bytes = text.as_bytes();

        if bytes.is_empty() {
            return Err(Error::Empty);
        }

        let start = if bytes[0] == b'+' { 1 } else { 0 };
        if start == bytes.len() {
            return Err(Error::SignOnly);
        }

        // Invalid bytes take precedence over malformed separators.
        if let Some(index) = (start..bytes.len())
            .find(|&i| bytes[i] != b'_' && !bytes[i].is_ascii_digit())
        {
            return Err(Error::InvalidDigit { index });
        }

        // Validate separators against their immediate magnitude neighbors.
        if let Some(index) = (start..bytes.len()).find(|&i| {
            bytes[i] == b'_'
                && (i == start
                    || i + 1 == bytes.len()
                    || !bytes[i - 1].is_ascii_digit()
                    || !bytes[i + 1].is_ascii_digit())
        }) {
            return Err(Error::Separator { index });
        }

        let first_digit_index = start;
        let digit_count = bytes[start..].iter().filter(|&&b| b != b'_').count();

        if digit_count > 1 && bytes[first_digit_index] == b'0' {
            return Err(Error::LeadingZero {
                index: first_digit_index,
            });
        }

        if digit_count > max_digits {
            let excess_digit_number = max_digits;
            let index = bytes[start..]
                .iter()
                .enumerate()
                .filter(|(_, b)| **b != b'_')
                .nth(excess_digit_number)
                .map(|(offset, _)| start + offset)
                .expect("digit count exceeds limit");
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index,
            });
        }

        let mut digits = Vec::with_capacity(digit_count);
        for &byte in bytes[start..].iter().filter(|&&b| b != b'_').rev() {
            digits.push(byte - b'0');
        }

        Ok(Self::from_digits(digits))
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let natural_len = self.digits.len().max(1);
        let width = min_digits.max(natural_len);
        let padding = width - natural_len;

        let mut out = String::with_capacity(width + if group > 0 { width / group } else { 0 });
        for position in 0..width {
            if group > 0 && position > 0 && (width - position) % group == 0 {
                out.push('_');
            }

            if position < padding {
                out.push('0');
            } else {
                let digit_index = natural_len - 1 - (position - padding);
                out.push(char::from(b'0' + self.digits[digit_index]));
            }
        }
        out
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match self.digits.len().cmp(&rhs.digits.len()) {
            Ordering::Equal => self.digits.iter().rev().cmp(rhs.digits.iter().rev()),
            ordering => ordering,
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        let length = self.digits.len().max(rhs.digits.len());
        let mut result = Vec::with_capacity(length + 1);
        let mut carry = 0u8;

        for i in 0..length {
            let sum = self.digits.get(i).copied().unwrap_or(0)
                + rhs.digits.get(i).copied().unwrap_or(0)
                + carry;
            result.push(sum % 10);
            carry = sum / 10;
        }

        if carry != 0 {
            result.push(carry);
        }

        Self::from_digits(result)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, Error> {
        if self.cmp_value(rhs) == Ordering::Less {
            return Err(Error::Underflow);
        }

        let mut result = Vec::with_capacity(self.digits.len());
        let mut borrow = 0i16;

        for i in 0..self.digits.len() {
            let lhs_digit = self.digits[i] as i16;
            let rhs_digit = rhs.digits.get(i).copied().unwrap_or(0) as i16;
            let mut difference = lhs_digit - rhs_digit - borrow;

            if difference < 0 {
                difference += 10;
                borrow = 1;
            } else {
                borrow = 0;
            }

            result.push(difference as u8);
        }

        Ok(Self::from_digits(result))
    }

    fn from_digits(mut digits: Vec<u8>) -> Self {
        while digits.len() > 1 && digits.last() == Some(&0) {
            digits.pop();
        }

        if digits.is_empty() {
            digits.push(0);
        }

        Self { digits }
    }
}