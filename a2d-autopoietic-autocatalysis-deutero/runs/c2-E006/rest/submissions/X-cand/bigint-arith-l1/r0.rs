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
    // Little-endian decimal digits; zero is represented as [0].
    digits: Vec<u8>,
}

impl Big {
    pub fn parse(text: &str, max_digits: usize) -> Result<Self, Error> {
        let bytes = text.as_bytes();

        if bytes.is_empty() {
            return Err(Error::Empty);
        }

        let start = usize::from(bytes[0] == b'+');
        if start == bytes.len() {
            return Err(Error::SignOnly);
        }

        // Invalid bytes take precedence over malformed separators.
        if let Some(index) = (start..bytes.len())
            .find(|&i| bytes[i] != b'_' && !bytes[i].is_ascii_digit())
        {
            return Err(Error::InvalidDigit { index });
        }

        // Validate separators after the full invalid-byte scan.
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

        let mut first_digit_index = None;
        let mut digit_count = 0usize;
        let mut first_digit = 0u8;
        let mut excess_index = None;

        for (i, &byte) in bytes.iter().enumerate().skip(start) {
            if byte.is_ascii_digit() {
                if first_digit_index.is_none() {
                    first_digit_index = Some(i);
                    first_digit = byte - b'0';
                }
                digit_count = digit_count.saturating_add(1);
                if digit_count > max_digits && excess_index.is_none() {
                    excess_index = Some(i);
                }
            }
        }

        let first_index = first_digit_index.expect("validated magnitude contains digits");

        if digit_count > 1 && first_digit == 0 {
            return Err(Error::LeadingZero { index: first_index });
        }

        if let Some(index) = excess_index {
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index,
            });
        }

        let mut digits = Vec::with_capacity(digit_count);
        for &byte in bytes.iter().skip(start) {
            if byte.is_ascii_digit() {
                digits.push(byte - b'0');
            }
        }
        digits.reverse();

        Ok(Self { digits })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let natural_len = self.digits.len().max(1);
        let len = natural_len.max(min_digits);
        let padding = len - natural_len;

        let mut out = String::new();
        for position in 0..len {
            if group > 0 && position > 0 && (len - position) % group == 0 {
                out.push('_');
            }

            if position < padding {
                out.push('0');
            } else {
                let digit_index_from_left = position - padding;
                let digit = self.digits[natural_len - 1 - digit_index_from_left];
                out.push(char::from(b'0' + digit));
            }
        }
        out
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match self.digits.len().cmp(&rhs.digits.len()) {
            Ordering::Equal => self.digits.iter().rev().cmp(rhs.digits.iter().rev()),
            other => other,
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        let len = self.digits.len().max(rhs.digits.len());
        let mut digits = Vec::with_capacity(len.saturating_add(1));
        let mut carry = 0u8;

        for i in 0..len {
            let a = self.digits.get(i).copied().unwrap_or(0);
            let b = rhs.digits.get(i).copied().unwrap_or(0);
            let sum = a + b + carry;
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

        let mut digits = Vec::with_capacity(self.digits.len());
        let mut borrow = 0i8;

        for i in 0..self.digits.len() {
            let a = self.digits[i] as i8 - borrow;
            let b = rhs.digits.get(i).copied().unwrap_or(0) as i8;

            if a < b {
                digits.push((a + 10 - b) as u8);
                borrow = 1;
            } else {
                digits.push((a - b) as u8);
                borrow = 0;
            }
        }

        while digits.len() > 1 && digits.last() == Some(&0) {
            digits.pop();
        }

        Ok(Self { digits })
    }
}