use std::cmp::Ordering;
use std::fmt;

#[derive(Clone, PartialEq, Eq)]
pub struct Big {
    // Little-endian base-10 digits; zero is represented as [0].
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

        let start = if bytes[0] == b'+' { 1 } else { 0 };
        if start == bytes.len() {
            return Err(Error::SignOnly);
        }

        // Invalid bytes take precedence over every separator error.
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if !byte.is_ascii_digit() && byte != b'_' {
                return Err(Error::InvalidDigit { index });
            }
        }

        let magnitude = &bytes[start..];

        // Check all separators before leading-zero and length checks.
        for (relative_index, &byte) in magnitude.iter().enumerate() {
            if byte == b'_' {
                let valid_before =
                    relative_index > 0 && magnitude[relative_index - 1].is_ascii_digit();
                let valid_after = relative_index + 1 < magnitude.len()
                    && magnitude[relative_index + 1].is_ascii_digit();

                if !valid_before || !valid_after {
                    return Err(Error::Separator {
                        index: start + relative_index,
                    });
                }
            }
        }

        let first_digit = magnitude
            .iter()
            .position(u8::is_ascii_digit)
            .expect("nonempty magnitude with valid separators has a digit");
        let digit_count = magnitude.iter().filter(|&&b| b.is_ascii_digit()).count();

        if digit_count > 1 && magnitude[first_digit] == b'0' {
            return Err(Error::LeadingZero {
                index: start + first_digit,
            });
        }

        if digit_count > max_digits {
            let excess_number = max_digits + 1;
            let mut seen = 0usize;
            for (relative_index, &byte) in magnitude.iter().enumerate() {
                if byte.is_ascii_digit() {
                    seen += 1;
                    if seen == excess_number {
                        return Err(Error::TooManyDigits {
                            limit: max_digits,
                            index: start + relative_index,
                        });
                    }
                }
            }
        }

        let digits = magnitude
            .iter()
            .filter(|&&b| b.is_ascii_digit())
            .rev()
            .map(|&b| b - b'0')
            .collect();

        Ok(Self { digits })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let actual_len = self.digits.len().max(1);
        let width = actual_len.max(min_digits);
        let padding = width - actual_len;

        let mut out = String::new();
        for position in 0..width {
            if group > 0 && position > 0 && (width - position) % group == 0 {
                out.push('_');
            }

            if position < padding {
                out.push('0');
            } else {
                let digit_index = actual_len - 1 - (position - padding);
                out.push(char::from(b'0' + self.digits[digit_index]));
            }
        }
        out
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match self.digits.len().cmp(&rhs.digits.len()) {
            Ordering::Equal => self.digits.iter().rev().cmp(rhs.digits.iter().rev()),
            order => order,
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        let max_len = self.digits.len().max(rhs.digits.len());
        let mut result = Vec::with_capacity(max_len + 1);
        let mut carry = 0u8;

        for index in 0..max_len {
            let a = self.digits.get(index).copied().unwrap_or(0);
            let b = rhs.digits.get(index).copied().unwrap_or(0);
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

        for index in 0..self.digits.len() {
            let a = i16::from(self.digits[index]) - borrow;
            let b = i16::from(rhs.digits.get(index).copied().unwrap_or(0));

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