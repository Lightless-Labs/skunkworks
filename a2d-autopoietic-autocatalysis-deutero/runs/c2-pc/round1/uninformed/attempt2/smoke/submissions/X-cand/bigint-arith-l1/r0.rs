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

        // Invalid bytes take precedence over malformed separators.
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' && !byte.is_ascii_digit() {
                return Err(Error::InvalidDigit { index });
            }
        }

        let magnitude = &bytes[start..];

        // Once the invalid-byte scan succeeds, the first malformed separator
        // takes precedence over leading-zero and digit-count errors.
        for (offset, &byte) in magnitude.iter().enumerate() {
            if byte == b'_' {
                let valid_before = offset > 0 && magnitude[offset - 1].is_ascii_digit();
                let valid_after =
                    offset + 1 < magnitude.len() && magnitude[offset + 1].is_ascii_digit();
                if !valid_before || !valid_after {
                    return Err(Error::Separator {
                        index: start + offset,
                    });
                }
            }
        }

        let digit_count = magnitude.iter().filter(|&&byte| byte != b'_').count();
        let first_digit = magnitude.iter().position(|&byte| byte != b'_').unwrap();

        if digit_count > 1 && magnitude[first_digit] == b'0' {
            return Err(Error::LeadingZero {
                index: start + first_digit,
            });
        }

        if digit_count > max_digits {
            let excess_digit_number = max_digits;
            let mut seen = 0;
            let mut excess_offset = first_digit;
            for (offset, &byte) in magnitude.iter().enumerate() {
                if byte != b'_' {
                    if seen == excess_digit_number {
                        excess_offset = offset;
                        break;
                    }
                    seen += 1;
                }
            }
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: start + excess_offset,
            });
        }

        let mut digits = Vec::with_capacity(digit_count);
        for &byte in magnitude {
            if byte != b'_' {
                digits.push(byte - b'0');
            }
        }
        digits.reverse();

        Ok(Self { digits })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let natural_len = self.digits.len().max(1);
        let width = natural_len.max(min_digits);
        let padding = width - natural_len;

        let mut out = String::with_capacity(width + width / group.max(1));
        for position in 0..width {
            if group > 0 && position > 0 && (width - position) % group == 0 {
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

    pub fn cmp_value(&self, rhs: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        match self.digits.len().cmp(&rhs.digits.len()) {
            Ordering::Equal => {
                for (&left, &right) in self.digits.iter().rev().zip(rhs.digits.iter().rev()) {
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

    pub fn add(&self, rhs: &Self) -> Self {
        let max_len = self.digits.len().max(rhs.digits.len());
        let mut digits = Vec::with_capacity(max_len + 1);
        let mut carry = 0u8;

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
        use std::cmp::Ordering;

        if self.cmp_value(rhs) == Ordering::Less {
            return Err(Error::Underflow);
        }

        let mut digits = Vec::with_capacity(self.digits.len());
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