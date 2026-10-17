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
    // Canonical, little-endian decimal digits. Zero is represented as [0].
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

        // Invalid bytes take precedence over all separator and numeric errors.
        if let Some(index) = (start..bytes.len())
            .find(|&i| bytes[i] != b'_' && !bytes[i].is_ascii_digit())
        {
            return Err(Error::InvalidDigit { index });
        }

        // Validate separators only after the entire invalid-byte scan succeeds.
        if let Some(index) = (start..bytes.len()).find(|&i| {
            bytes[i] == b'_'
                && (i == start
                    || i + 1 == bytes.len()
                    || !bytes[i - 1].is_ascii_digit()
                    || !bytes[i + 1].is_ascii_digit())
        }) {
            return Err(Error::Separator { index });
        }

        let digit_count = bytes[start..].iter().filter(|&&b| b != b'_').count();
        let first_digit_index = (start..bytes.len()).find(|&i| bytes[i] != b'_').unwrap();

        if digit_count > 1 && bytes[first_digit_index] == b'0' {
            return Err(Error::LeadingZero {
                index: first_digit_index,
            });
        }

        if digit_count > max_digits {
            let excess_number = max_digits + 1;
            let mut seen = 0usize;
            for i in start..bytes.len() {
                if bytes[i] != b'_' {
                    seen += 1;
                    if seen == excess_number {
                        return Err(Error::TooManyDigits { limit: max_digits, index: i });
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

        Ok(Self { digits })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let natural_len = self.digits.len().max(1);
        let width = natural_len.max(min_digits);
        let padding = width - natural_len;

        let mut out = String::with_capacity(width + if group > 0 { width / group } else { 0 });
        for position in 0..width {
            if group > 0 && position > 0 && (width - position) % group == 0 {
                out.push('_');
            }

            if position < padding {
                out.push('0');
            } else {
                let digit_index = width - 1 - position;
                out.push((b'0' + self.digits[digit_index]) as char);
            }
        }

        out
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match self.digits.len().cmp(&rhs.digits.len()) {
            Ordering::Equal => {
                for (&left, &right) in self.digits.iter().rev().zip(rhs.digits.iter().rev()) {
                    match left.cmp(&right) {
                        Ordering::Equal => {}
                        other => return other,
                    }
                }
                Ordering::Equal
            }
            other => other,
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        let mut result = Vec::with_capacity(self.digits.len().max(rhs.digits.len()) + 1);
        let mut carry = 0u8;
        let len = self.digits.len().max(rhs.digits.len());

        for i in 0..len {
            let left = self.digits.get(i).copied().unwrap_or(0);
            let right = rhs.digits.get(i).copied().unwrap_or(0);
            let sum = left + right + carry;
            result.push(sum % 10);
            carry = sum / 10;
        }

        if carry != 0 {
            result.push(carry);
        }

        Self { digits: result }
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, Error> {
        if self.cmp_value(rhs) == Ordering::Less {
            return Err(Error::Underflow);
        }

        let mut result = Vec::with_capacity(self.digits.len());
        let mut borrow = 0i8;

        for i in 0..self.digits.len() {
            let left = self.digits[i] as i8 - borrow;
            let right = rhs.digits.get(i).copied().unwrap_or(0) as i8;
            if left < right {
                result.push((left + 10 - right) as u8);
                borrow = 1;
            } else {
                result.push((left - right) as u8);
                borrow = 0;
            }
        }

        while result.len() > 1 && result.last() == Some(&0) {
            result.pop();
        }

        Ok(Self { digits: result })
    }
}