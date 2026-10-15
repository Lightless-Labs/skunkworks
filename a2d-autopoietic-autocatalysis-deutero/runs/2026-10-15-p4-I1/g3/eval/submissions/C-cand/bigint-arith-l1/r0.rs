use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    // Little-endian base-10 digits; zero is represented by [0].
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
        if start == bytes.len() {
            return Err(Error::SignOnly);
        }

        if let Some(index) = (start..bytes.len())
            .find(|&i| bytes[i] != b'_' && !bytes[i].is_ascii_digit())
        {
            return Err(Error::InvalidDigit { index });
        }

        if let Some(index) = (start..bytes.len()).find(|&i| {
            bytes[i] == b'_'
                && (i == start
                    || i + 1 == bytes.len()
                    || !bytes[i - 1].is_ascii_digit()
                    || !bytes[i + 1].is_ascii_digit())
        }) {
            return Err(Error::Separator { index });
        }

        let first_digit_index = (start..bytes.len())
            .find(|&i| bytes[i] != b'_')
            .expect("nonempty magnitude with valid separators");
        let digit_count = bytes[start..]
            .iter()
            .filter(|&&b| b != b'_')
            .count();

        if digit_count > 1 && bytes[first_digit_index] == b'0' {
            return Err(Error::LeadingZero {
                index: first_digit_index,
            });
        }

        if digit_count > max_digits {
            let index = bytes[start..]
                .iter()
                .enumerate()
                .filter(|&(_, &b)| b != b'_')
                .nth(max_digits)
                .map(|(offset, _)| start + offset)
                .expect("excess digit exists");
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index,
            });
        }

        let digits = bytes[start..]
            .iter()
            .filter(|&&b| b != b'_')
            .rev()
            .map(|&b| b - b'0')
            .collect();

        Ok(Self { digits })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let magnitude_len = self.digits.len().max(1);
        let width = magnitude_len.max(min_digits);
        let padding = width - magnitude_len;
        let separators = if group > 0 {
            width.saturating_sub(1) / group
        } else {
            0
        };
        let mut output =
            String::with_capacity(width.saturating_add(separators));

        for position in 0..width {
            if group > 0 && position > 0 && (width - position) % group == 0 {
                output.push('_');
            }

            let digit = if position < padding {
                0
            } else {
                self.digits[width - position - 1 - padding]
            };
            output.push(char::from(b'0' + digit));
        }

        output
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match self.digits.len().cmp(&rhs.digits.len()) {
            Ordering::Equal => self.digits.iter().rev().cmp(rhs.digits.iter().rev()),
            ordering => ordering,
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        let max_len = self.digits.len().max(rhs.digits.len());
        let mut result = Vec::with_capacity(max_len.saturating_add(1));
        let mut carry = 0u8;

        for i in 0..max_len {
            let sum = self.digits.get(i).copied().unwrap_or(0)
                + rhs.digits.get(i).copied().unwrap_or(0)
                + carry;
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
        let mut borrow = 0i16;

        for i in 0..self.digits.len() {
            let lhs_digit = self.digits[i] as i16 - borrow;
            let rhs_digit = rhs.digits.get(i).copied().unwrap_or(0) as i16;
            if lhs_digit < rhs_digit {
                result.push((lhs_digit + 10 - rhs_digit) as u8);
                borrow = 1;
            } else {
                result.push((lhs_digit - rhs_digit) as u8);
                borrow = 0;
            }
        }

        while result.len() > 1 && result.last() == Some(&0) {
            result.pop();
        }

        Ok(Self { digits: result })
    }
}

fn main() {}