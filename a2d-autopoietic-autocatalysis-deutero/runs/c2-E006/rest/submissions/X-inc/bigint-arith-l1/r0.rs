use std::cmp::Ordering;
use std::fmt;

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

#[derive(Clone, PartialEq, Eq)]
pub struct Big {
    // Little-endian base-10 digits, with no redundant high zeroes.
    digits: Vec<u8>,
}

impl fmt::Debug for Big {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Big")
            .field("digits", &self.format(0, 0))
            .finish()
    }
}

impl Big {
    pub fn parse(text: &str, max_digits: usize) -> Result<Self, Error> {
        let bytes = text.as_bytes();

        if bytes.is_empty() {
            return Err(Error::Empty);
        }

        let magnitude_start = usize::from(bytes[0] == b'+');
        if magnitude_start == bytes.len() {
            return Err(Error::SignOnly);
        }

        // Invalid bytes take precedence over separator errors.
        if let Some(index) = (magnitude_start..bytes.len())
            .find(|&i| bytes[i] != b'_' && !bytes[i].is_ascii_digit())
        {
            return Err(Error::InvalidDigit { index });
        }

        // Check every underscore against its immediate magnitude neighbors.
        for i in magnitude_start..bytes.len() {
            if bytes[i] == b'_'
                && (i == magnitude_start
                    || i + 1 == bytes.len()
                    || !bytes[i - 1].is_ascii_digit()
                    || !bytes[i + 1].is_ascii_digit())
            {
                return Err(Error::Separator { index: i });
            }
        }

        let digit_count = bytes[magnitude_start..]
            .iter()
            .filter(|&&b| b != b'_')
            .count();
        let first_digit_index = (magnitude_start..bytes.len())
            .find(|&i| bytes[i] != b'_')
            .expect("nonempty magnitude has a digit");

        if digit_count > 1 && bytes[first_digit_index] == b'0' {
            return Err(Error::LeadingZero {
                index: first_digit_index,
            });
        }

        if digit_count > max_digits {
            let excess_number = max_digits + 1;
            let mut seen = 0usize;
            for i in magnitude_start..bytes.len() {
                if bytes[i] != b'_' {
                    seen += 1;
                    if seen == excess_number {
                        return Err(Error::TooManyDigits {
                            limit: max_digits,
                            index: i,
                        });
                    }
                }
            }
        }

        let mut digits = Vec::with_capacity(digit_count);
        for &byte in bytes[magnitude_start..].iter().rev() {
            if byte != b'_' {
                digits.push(byte - b'0');
            }
        }

        let mut result = Self { digits };
        result.normalize();
        Ok(result)
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let magnitude = self.magnitude_string();
        let width = min_digits.max(magnitude.len());
        let padding = width - magnitude.len();

        let mut output = String::with_capacity(
            width.saturating_add(if group > 0 { width.saturating_sub(1) / group } else { 0 }),
        );

        for position in 0..width {
            if group > 0 && position > 0 && (width - position) % group == 0 {
                output.push('_');
            }
            if position < padding {
                output.push('0');
            } else {
                output.push(magnitude.as_bytes()[position - padding] as char);
            }
        }

        output
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match self.digits.len().cmp(&rhs.digits.len()) {
            Ordering::Equal => self.digits.iter().rev().cmp(rhs.digits.iter().rev()),
            other => other,
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        let max_len = self.digits.len().max(rhs.digits.len());
        let mut digits = Vec::with_capacity(max_len.saturating_add(1));
        let mut carry = 0u8;

        for i in 0..max_len {
            let sum = self.digits.get(i).copied().unwrap_or(0)
                + rhs.digits.get(i).copied().unwrap_or(0)
                + carry;
            digits.push(sum % 10);
            carry = sum / 10;
        }

        if carry != 0 {
            digits.push(carry);
        }

        let mut result = Self { digits };
        result.normalize();
        result
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, Error> {
        if self.cmp_value(rhs) == Ordering::Less {
            return Err(Error::Underflow);
        }

        let mut digits = Vec::with_capacity(self.digits.len());
        let mut borrow = 0i16;

        for i in 0..self.digits.len() {
            let mut difference = self.digits[i] as i16
                - rhs.digits.get(i).copied().unwrap_or(0) as i16
                - borrow;

            if difference < 0 {
                difference += 10;
                borrow = 1;
            } else {
                borrow = 0;
            }
            digits.push(difference as u8);
        }

        let mut result = Self { digits };
        result.normalize();
        Ok(result)
    }

    fn normalize(&mut self) {
        while self.digits.len() > 1 && self.digits.last() == Some(&0) {
            self.digits.pop();
        }
        if self.digits.is_empty() {
            self.digits.push(0);
        }
    }

    fn magnitude_string(&self) -> String {
        self.digits
            .iter()
            .rev()
            .map(|&digit| (b'0' + digit) as char)
            .collect()
    }
}