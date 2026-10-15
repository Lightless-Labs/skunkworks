use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    // Little-endian decimal digits; zero is represented by [0].
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

        let start = usize::from(bytes[0] == b'+');
        if start == bytes.len() && start == 1 {
            return Err(Error::SignOnly);
        }

        // Check every magnitude byte before considering separator placement.
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' && !byte.is_ascii_digit() {
                return Err(Error::InvalidDigit { index });
            }
        }

        // Find the leftmost malformed separator.
        for index in start..bytes.len() {
            if bytes[index] == b'_' {
                let prev_is_digit = index > start && bytes[index - 1].is_ascii_digit();
                let next_is_digit =
                    index + 1 < bytes.len() && bytes[index + 1].is_ascii_digit();
                if !(prev_is_digit && next_is_digit) {
                    return Err(Error::Separator { index });
                }
            }
        }

        let digit_count = bytes[start..].iter().filter(|&&byte| byte != b'_').count();
        let first_digit_index = (start..bytes.len())
            .find(|&index| bytes[index] != b'_')
            .expect("magnitude is nonempty after the earlier checks");

        if digit_count > 1 && bytes[first_digit_index] == b'0' {
            return Err(Error::LeadingZero {
                index: first_digit_index,
            });
        }

        if digit_count > max_digits {
            let excess_number = max_digits.saturating_add(1);
            let mut seen = 0usize;
            let mut excess_index = first_digit_index;
            for index in start..bytes.len() {
                if bytes[index] != b'_' {
                    seen += 1;
                    if seen == excess_number {
                        excess_index = index;
                        break;
                    }
                }
            }
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: excess_index,
            });
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
        let output_len = natural_len.max(min_digits);
        let padding = output_len - self.digits.len();
        let separators = if group == 0 {
            0
        } else {
            (output_len - 1) / group
        };
        let mut result = String::with_capacity(output_len.saturating_add(separators));

        for position in 0..output_len {
            if position > 0 && group > 0 && (output_len - position) % group == 0 {
                result.push('_');
            }

            if position < padding {
                result.push('0');
            } else {
                let digit_index = self.digits.len() - 1 - (position - padding);
                result.push((b'0' + self.digits[digit_index]) as char);
            }
        }

        result
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

        for index in 0..max_len {
            let a = self.digits.get(index).copied().unwrap_or(0);
            let b = rhs.digits.get(index).copied().unwrap_or(0);
            let sum = a + b + carry;
            digits.push(sum % 10);
            carry = sum / 10;
        }

        if carry > 0 {
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

        for index in 0..self.digits.len() {
            let a = self.digits[index] as i8 - borrow;
            let b = rhs.digits.get(index).copied().unwrap_or(0) as i8;

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

fn main() {}