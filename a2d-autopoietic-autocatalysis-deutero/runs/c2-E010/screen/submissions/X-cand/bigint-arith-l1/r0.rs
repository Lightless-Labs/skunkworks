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
    // Little-endian base-10 digits. Zero is represented by [0].
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

        // Invalid bytes have priority over every other lexical error.
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' && !byte.is_ascii_digit() {
                return Err(Error::InvalidDigit { index });
            }
        }

        // Once all bytes are known to be digits or underscores, check separators.
        for index in start..bytes.len() {
            if bytes[index] == b'_'
                && (index == start
                    || index + 1 == bytes.len()
                    || !bytes[index - 1].is_ascii_digit()
                    || !bytes[index + 1].is_ascii_digit())
            {
                return Err(Error::Separator { index });
            }
        }

        let first_digit_index = (start..bytes.len())
            .find(|&index| bytes[index].is_ascii_digit())
            .expect("nonempty magnitude with valid separators");

        let digit_count = bytes[start..]
            .iter()
            .filter(|&&byte| byte.is_ascii_digit())
            .count();

        if digit_count > 1 && bytes[first_digit_index] == b'0' {
            return Err(Error::LeadingZero {
                index: first_digit_index,
            });
        }

        if digit_count > max_digits {
            let excess_digit_number = max_digits;
            let index = bytes[start..]
                .iter()
                .enumerate()
                .filter(|(_, byte)| byte.is_ascii_digit())
                .nth(excess_digit_number)
                .map(|(offset, _)| start + offset)
                .expect("digit count exceeds limit");
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index,
            });
        }

        let mut digits = Vec::with_capacity(digit_count);
        for &byte in bytes[start..].iter().rev() {
            if byte.is_ascii_digit() {
                digits.push(byte - b'0');
            }
        }

        Ok(Self { digits })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let natural_len = self.digits.len().max(1);
        let width = natural_len.max(min_digits);
        let padding = width - self.digits.len();

        let mut out = String::with_capacity(
            width + if group > 0 { width.saturating_sub(1) / group } else { 0 },
        );

        for position in 0..width {
            if group > 0 && position > 0 && (width - position) % group == 0 {
                out.push('_');
            }

            if position < padding {
                out.push('0');
            } else {
                let digit_index = self.digits.len() - 1 - (position - padding);
                out.push(char::from(b'0' + self.digits[digit_index]));
            }
        }

        out
    }

    pub fn cmp_value(&self, rhs: &Self) -> std::cmp::Ordering {
        match self.digits.len().cmp(&rhs.digits.len()) {
            std::cmp::Ordering::Equal => {
                for (left, right) in self.digits.iter().rev().zip(rhs.digits.iter().rev()) {
                    match left.cmp(right) {
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
        let mut result = Vec::with_capacity(max_len.saturating_add(1));
        let mut carry = 0u8;

        for index in 0..max_len {
            let left = self.digits.get(index).copied().unwrap_or(0);
            let right = rhs.digits.get(index).copied().unwrap_or(0);
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
        if self.cmp_value(rhs) == std::cmp::Ordering::Less {
            return Err(Error::Underflow);
        }

        let mut result = Vec::with_capacity(self.digits.len());
        let mut borrow = 0i16;

        for index in 0..self.digits.len() {
            let left = i16::from(self.digits[index]) - borrow;
            let right = i16::from(rhs.digits.get(index).copied().unwrap_or(0));

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
