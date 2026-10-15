use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
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

        // Invalid bytes take precedence over separator and numeric errors.
        if let Some(index) = (start..bytes.len())
            .find(|&i| bytes[i] != b'_' && !bytes[i].is_ascii_digit())
        {
            return Err(Error::InvalidDigit { index });
        }

        // Then validate every underscore against its immediate neighbors.
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
            let excess_digit_number = max_digits;
            let index = (start..bytes.len())
                .filter(|&i| bytes[i] != b'_')
                .nth(excess_digit_number)
                .unwrap();
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index,
            });
        }

        let digits = bytes[start..]
            .iter()
            .filter(|&&b| b != b'_')
            .map(|&b| b - b'0')
            .collect();

        Ok(Self { digits })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let width = min_digits.max(self.digits.len()).max(1);
        let padding = width - self.digits.len();
        let mut out = String::with_capacity(width + if group > 0 { (width - 1) / group } else { 0 });

        for position in 0..width {
            if group > 0 && position > 0 && (width - position) % group == 0 {
                out.push('_');
            }

            if position < padding {
                out.push('0');
            } else {
                out.push((b'0' + self.digits[position - padding]) as char);
            }
        }

        out
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        self.digits
            .len()
            .cmp(&rhs.digits.len())
            .then_with(|| self.digits.cmp(&rhs.digits))
    }

    pub fn add(&self, rhs: &Self) -> Self {
        let mut result = Vec::with_capacity(self.digits.len().max(rhs.digits.len()) + 1);
        let mut carry = 0u8;
        let mut left = self.digits.len();
        let mut right = rhs.digits.len();

        while left > 0 || right > 0 || carry > 0 {
            let a = if left > 0 {
                left -= 1;
                self.digits[left]
            } else {
                0
            };
            let b = if right > 0 {
                right -= 1;
                rhs.digits[right]
            } else {
                0
            };

            let sum = a + b + carry;
            result.push(sum % 10);
            carry = sum / 10;
        }

        result.reverse();
        Self { digits: result }
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, Error> {
        if self.cmp_value(rhs) == Ordering::Less {
            return Err(Error::Underflow);
        }

        let mut result = Vec::with_capacity(self.digits.len());
        let mut borrow = 0i8;
        let mut left = self.digits.len();
        let mut right = rhs.digits.len();

        while left > 0 {
            left -= 1;
            let a = self.digits[left] as i8 - borrow;
            let b = if right > 0 {
                right -= 1;
                rhs.digits[right] as i8
            } else {
                0
            };

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
        result.reverse();

        Ok(Self { digits: result })
    }
}

fn main() {}