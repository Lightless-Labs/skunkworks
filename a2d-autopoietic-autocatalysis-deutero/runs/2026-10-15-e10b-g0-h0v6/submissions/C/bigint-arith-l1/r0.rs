#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    // Little-endian base-10 digits, with zero represented as [0].
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

        // First scan every magnitude byte for invalid characters. Underscores
        // are ignored here so invalid digits take precedence over separators.
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' && !byte.is_ascii_digit() {
                return Err(Error::InvalidDigit { index });
            }
        }

        let mut first_separator_error = None;
        for index in start..bytes.len() {
            if bytes[index] == b'_' {
                let previous_is_digit = index > start && bytes[index - 1].is_ascii_digit();
                let next_is_digit = index + 1 < bytes.len() && bytes[index + 1].is_ascii_digit();
                if !(previous_is_digit && next_is_digit) {
                    first_separator_error = Some(index);
                    break;
                }
            }
        }

        if let Some(index) = first_separator_error {
            return Err(Error::Separator { index });
        }

        let digit_count = bytes[start..].iter().filter(|&&b| b != b'_').count();
        let first_digit_index = (start..bytes.len())
            .find(|&index| bytes[index] != b'_')
            .expect("nonempty magnitude has a digit");

        if digit_count > 1 && bytes[first_digit_index] == b'0' {
            return Err(Error::LeadingZero {
                index: first_digit_index,
            });
        }

        if digit_count > max_digits {
            let target_digit = max_digits;
            let mut seen = 0usize;
            let excess_index = (start..bytes.len())
                .find(|&index| {
                    if bytes[index] == b'_' {
                        false
                    } else {
                        let is_excess = seen == target_digit;
                        seen += 1;
                        is_excess
                    }
                })
                .expect("digit count exceeds limit");
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
        let actual_len = self.digits.len().max(1);
        let width = actual_len.max(min_digits);
        let padding = width - actual_len;

        let mut output = String::new();
        for i in 0..width {
            if group > 0 && i > 0 && (width - i) % group == 0 {
                output.push('_');
            }

            if i < padding {
                output.push('0');
            } else {
                let digit_index = width - 1 - i;
                output.push(char::from(b'0' + self.digits[digit_index]));
            }
        }
        output
    }

    pub fn cmp_value(&self, rhs: &Self) -> std::cmp::Ordering {
        match self.digits.len().cmp(&rhs.digits.len()) {
            std::cmp::Ordering::Equal => {
                for (&left, &right) in self.digits.iter().rev().zip(rhs.digits.iter().rev()) {
                    match left.cmp(&right) {
                        std::cmp::Ordering::Equal => {}
                        ordering => return ordering,
                    }
                }
                std::cmp::Ordering::Equal
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
        if self.cmp_value(rhs) == std::cmp::Ordering::Less {
            return Err(Error::Underflow);
        }

        let mut digits = Vec::with_capacity(self.digits.len());
        let mut borrow = 0i16;

        for index in 0..self.digits.len() {
            let left = self.digits[index] as i16 - borrow;
            let right = rhs.digits.get(index).copied().unwrap_or(0) as i16;

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

fn main() {}