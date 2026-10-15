use std::cmp::Ordering;
use std::fmt;

#[derive(Clone, PartialEq, Eq)]
pub struct Big {
    // Little-endian base-10 digits. Zero is represented as [0].
    digits: Vec<u8>,
}

impl fmt::Debug for Big {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Big").field("digits", &self.digits).finish()
    }
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

        let magnitude_start = if bytes[0] == b'+' { 1 } else { 0 };
        if magnitude_start == bytes.len() {
            return Err(Error::SignOnly);
        }

        // Invalid bytes take precedence over separator errors.
        if let Some(index) = (magnitude_start..bytes.len())
            .find(|&i| bytes[i] != b'_' && !bytes[i].is_ascii_digit())
        {
            return Err(Error::InvalidDigit { index });
        }

        let mut digit_positions = Vec::new();
        for i in magnitude_start..bytes.len() {
            if bytes[i] == b'_' {
                let valid_before = i > magnitude_start && bytes[i - 1].is_ascii_digit();
                let valid_after = i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit();
                if !valid_before || !valid_after {
                    return Err(Error::Separator { index: i });
                }
            } else {
                digit_positions.push(i);
            }
        }

        if digit_positions.len() > 1 && bytes[digit_positions[0]] == b'0' {
            return Err(Error::LeadingZero {
                index: digit_positions[0],
            });
        }

        if digit_positions.len() > max_digits {
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: digit_positions[max_digits],
            });
        }

        let digits = digit_positions
            .iter()
            .rev()
            .map(|&i| bytes[i] - b'0')
            .collect();

        Ok(Self { digits })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let actual_len = self.digits.len().max(1);
        let width = actual_len.max(min_digits);
        let padding = width - actual_len;
        let mut out = String::new();

        for i in 0..width {
            if group > 0 && i > 0 && (width - i) % group == 0 {
                out.push('_');
            }
            if i < padding {
                out.push('0');
            } else {
                let digit_index = width - 1 - i;
                out.push(char::from(b'0' + self.digits[digit_index]));
            }
        }

        out
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        self.digits
            .len()
            .cmp(&rhs.digits.len())
            .then_with(|| self.digits.iter().rev().cmp(rhs.digits.iter().rev()))
    }

    pub fn add(&self, rhs: &Self) -> Self {
        let mut result = Vec::with_capacity(self.digits.len().max(rhs.digits.len()) + 1);
        let mut carry = 0u8;
        let len = self.digits.len().max(rhs.digits.len());

        for i in 0..len {
            let a = self.digits.get(i).copied().unwrap_or(0);
            let b = rhs.digits.get(i).copied().unwrap_or(0);
            let sum = a + b + carry;
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
            let a = self.digits[i] as i16 - borrow;
            let b = rhs.digits.get(i).copied().unwrap_or(0) as i16;
            if a < b {
                result.push((a + 10 - b) as u8);
                borrow = 1;
            } else {
                result.push((a - b) as u8);
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