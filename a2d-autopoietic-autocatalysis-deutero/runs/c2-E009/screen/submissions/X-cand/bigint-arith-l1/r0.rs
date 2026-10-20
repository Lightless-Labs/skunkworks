use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    // Little-endian base-10 digits. Zero is represented as [0].
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
    Underflow,
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

        // Invalid bytes take precedence over every separator error.
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' && !byte.is_ascii_digit() {
                return Err(Error::InvalidDigit { index });
            }
        }

        // Underscores must have a digit immediately on both sides.
        let mut separator_error = None;
        for index in start..bytes.len() {
            if bytes[index] == b'_' {
                let has_digit_before = index > start && bytes[index - 1].is_ascii_digit();
                let has_digit_after =
                    index + 1 < bytes.len() && bytes[index + 1].is_ascii_digit();
                if !(has_digit_before && has_digit_after) {
                    separator_error = Some(index);
                    break;
                }
            }
        }
        if let Some(index) = separator_error {
            return Err(Error::Separator { index });
        }

        let mut first_digit_index = None;
        let mut digit_count = 0usize;
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' {
                if first_digit_index.is_none() {
                    first_digit_index = Some(index);
                }
                digit_count += 1;
            }
        }

        let first_digit_index = first_digit_index.expect("nonempty magnitude has a digit");
        if digit_count > 1 && bytes[first_digit_index] == b'0' {
            return Err(Error::LeadingZero {
                index: first_digit_index,
            });
        }

        if digit_count > max_digits {
            let excess_number = max_digits + 1;
            let mut seen = 0usize;
            for (index, &byte) in bytes.iter().enumerate().skip(start) {
                if byte != b'_' {
                    seen += 1;
                    if seen == excess_number {
                        return Err(Error::TooManyDigits {
                            limit: max_digits,
                            index,
                        });
                    }
                }
            }
        }

        let mut digits = Vec::new();
        for &byte in bytes[start..].iter().rev() {
            if byte != b'_' {
                digits.push(byte - b'0');
            }
        }

        Ok(Self { digits })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let natural_len = self.digits.len().max(1);
        let width = natural_len.max(min_digits);
        let padding = width - natural_len;

        let mut output = String::new();
        for position in 0..width {
            if group > 0 && position > 0 && (width - position) % group == 0 {
                output.push('_');
            }

            if position < padding {
                output.push('0');
            } else {
                let digit_index = width - 1 - position;
                output.push(char::from(b'0' + self.digits[digit_index]));
            }
        }
        output
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match self.digits.len().cmp(&rhs.digits.len()) {
            Ordering::Equal => {
                for (left, right) in self.digits.iter().rev().zip(rhs.digits.iter().rev()) {
                    match left.cmp(right) {
                        Ordering::Equal => {}
                        ordering => return ordering,
                    }
                }
                Ordering::Equal
            }
            ordering => ordering,
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        let mut digits = Vec::new();
        let mut carry = 0u8;
        let max_len = self.digits.len().max(rhs.digits.len());

        for index in 0..max_len {
            let left = self.digits.get(index).copied().unwrap_or(0);
            let right = rhs.digits.get(index).copied().unwrap_or(0);
            let sum = left + right + carry;
            digits.push(sum % 10);
            carry = sum / 10;
        }

        if carry != 0 {
            digits.push(carry);
        }

        Self { digits }
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, Error> {
        if self.cmp_value(rhs) == Ordering::Less {
            return Err(Error::Underflow);
        }

        let mut digits = Vec::new();
        let mut borrow = 0i8;

        for index in 0..self.digits.len() {
            let left = self.digits[index] as i8 - borrow;
            let right = rhs.digits.get(index).copied().unwrap_or(0) as i8;
            if left < right {
                digits.push((left + 10 - right) as u8);
                borrow = 1;
            } else {
                digits.push((left - right) as u8);
                borrow = 0;
            }
        }

        while digits.len() > 1 && digits.last() == Some(&0) {
            digits.pop();
        }

        Ok(Self { digits })
    }
}