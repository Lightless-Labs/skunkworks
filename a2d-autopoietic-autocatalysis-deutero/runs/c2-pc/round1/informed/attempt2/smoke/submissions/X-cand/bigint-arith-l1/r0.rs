#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    // Little-endian decimal digits; zero is represented as [0].
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

        // Scan every magnitude byte for invalid digits before checking separators.
        if let Some(index) = (start..bytes.len())
            .find(|&i| bytes[i] != b'_' && !bytes[i].is_ascii_digit())
        {
            return Err(Error::InvalidDigit { index });
        }

        // Find the first underscore that is not immediately between digits.
        if let Some(index) = (start..bytes.len()).find(|&i| {
            bytes[i] == b'_'
                && (i == start
                    || i + 1 == bytes.len()
                    || !bytes[i - 1].is_ascii_digit()
                    || !bytes[i + 1].is_ascii_digit())
        }) {
            return Err(Error::Separator { index });
        }

        let first_digit_index = start;
        let digit_count = bytes[start..].iter().filter(|&&b| b != b'_').count();

        if digit_count > 1 && bytes[first_digit_index] == b'0' {
            return Err(Error::LeadingZero {
                index: first_digit_index,
            });
        }

        if digit_count > max_digits {
            let excess_number = max_digits + 1;
            let mut seen = 0;
            let index = (start..bytes.len())
                .find(|&i| {
                    if bytes[i] == b'_' {
                        false
                    } else {
                        seen += 1;
                        seen == excess_number
                    }
                })
                .unwrap_or(first_digit_index);
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
        let mut out = String::with_capacity(width + if group > 0 { width / group } else { 0 });

        for position in 0..width {
            if group > 0 && position > 0 && (width - position) % group == 0 {
                out.push('_');
            }

            let digit = if position < padding {
                0
            } else {
                self.digits[magnitude_len - 1 - (position - padding)]
            };
            out.push((b'0' + digit) as char);
        }

        out
    }

    pub fn cmp_value(&self, rhs: &Self) -> std::cmp::Ordering {
        self.digits
            .len()
            .cmp(&rhs.digits.len())
            .then_with(|| self.digits.iter().rev().cmp(rhs.digits.iter().rev()))
    }

    pub fn add(&self, rhs: &Self) -> Self {
        let max_len = self.digits.len().max(rhs.digits.len());
        let mut digits = Vec::with_capacity(max_len + 1);
        let mut carry = 0u8;

        for i in 0..max_len {
            let sum = self.digits.get(i).copied().unwrap_or(0)
                + rhs.digits.get(i).copied().unwrap_or(0)
                + carry;
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
        let mut borrow = 0i8;

        for i in 0..self.digits.len() {
            let mut difference = self.digits[i] as i8
                - rhs.digits.get(i).copied().unwrap_or(0) as i8
                - borrow;

            if difference < 0 {
                difference += 10;
                borrow = 1;
            } else {
                borrow = 0;
            }

            digits.push(difference as u8);
        }

        while digits.len() > 1 && digits.last() == Some(&0) {
            digits.pop();
        }

        Ok(Self { digits })
    }
}