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
    // Little-endian base-10 digits. The representation is canonical:
    // zero is [0], and nonzero values have no leading zeroes.
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

        // Check every magnitude byte before considering separator errors.
        if let Some(index) = (start..bytes.len())
            .find(|&i| bytes[i] != b'_' && !bytes[i].is_ascii_digit())
        {
            return Err(Error::InvalidDigit { index });
        }

        // Underscores must have an ASCII digit on both sides.
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
            .expect("nonempty magnitude with valid bytes");

        let digit_count = bytes[start..].iter().filter(|&&b| b != b'_').count();

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
                .filter(|(_, b)| **b != b'_')
                .nth(excess_digit_number)
                .map(|(offset, _)| start + offset)
                .expect("digit count exceeds limit");
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
        let natural_len = self.digits.len().max(1);
        let width = natural_len.max(min_digits);
        let padding = width - natural_len;
        let mut out = String::with_capacity(width.saturating_add(width / group.max(1)));

        for pos in 0..width {
            if group > 0 && pos > 0 && (width - pos) % group == 0 {
                out.push('_');
            }

            if pos < padding {
                out.push('0');
            } else {
                let digit_index = width - 1 - pos;
                out.push((b'0' + self.digits[digit_index]) as char);
            }
        }

        out
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match self.digits.len().cmp(&rhs.digits.len()) {
            Ordering::Equal => {
                for (a, b) in self.digits.iter().rev().zip(rhs.digits.iter().rev()) {
                    match a.cmp(b) {
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