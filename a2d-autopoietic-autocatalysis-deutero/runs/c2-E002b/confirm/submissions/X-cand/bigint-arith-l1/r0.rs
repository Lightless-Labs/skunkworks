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
    // Base-10 digits, most significant first; zero is represented as [0].
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

        // Invalid bytes take precedence over every separator error.
        if let Some(index) = (start..bytes.len())
            .find(|&i| bytes[i] != b'_' && !bytes[i].is_ascii_digit())
        {
            return Err(Error::InvalidDigit { index });
        }

        // A separator must have an ASCII digit on both sides.
        if let Some(index) = (start..bytes.len()).find(|&i| {
            bytes[i] == b'_'
                && (i == start
                    || i + 1 == bytes.len()
                    || !bytes[i - 1].is_ascii_digit()
                    || !bytes[i + 1].is_ascii_digit())
        }) {
            return Err(Error::Separator { index });
        }

        let first_digit = (start..bytes.len()).find(|&i| bytes[i] != b'_').unwrap();
        let digit_count = bytes[start..].iter().filter(|&&b| b != b'_').count();

        if digit_count > 1 && bytes[first_digit] == b'0' {
            return Err(Error::LeadingZero { index: first_digit });
        }

        if digit_count > max_digits {
            let excess_number = max_digits + 1;
            let index = bytes[start..]
                .iter()
                .enumerate()
                .filter(|(_, b)| **b != b'_')
                .nth(excess_number - 1)
                .map(|(i, _)| start + i)
                .unwrap();
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index,
            });
        }

        let digits: Vec<u8> = bytes[start..]
            .iter()
            .filter(|&&b| b != b'_')
            .map(|&b| b - b'0')
            .collect();

        Ok(Self::from_digits(digits))
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let width = self.digits.len().max(min_digits).max(1);
        let padding = width - self.digits.len();
        let mut output = String::with_capacity(width + if group > 0 { width / group } else { 0 });

        for i in 0..width {
            if group > 0 && i > 0 && (width - i) % group == 0 {
                output.push('_');
            }
            if i < padding {
                output.push('0');
            } else {
                output.push((b'0' + self.digits[i - padding]) as char);
            }
        }
        output
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match self.digits.len().cmp(&rhs.digits.len()) {
            Ordering::Equal => self.digits.cmp(&rhs.digits),
            other => other,
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        let mut result = Vec::with_capacity(self.digits.len().max(rhs.digits.len()) + 1);
        let mut carry = 0u8;
        let mut left = self.digits.iter().rev();
        let mut right = rhs.digits.iter().rev();

        loop {
            match (left.next(), right.next()) {
                (None, None) => break,
                (a, b) => {
                    let sum = a.copied().unwrap_or(0) + b.copied().unwrap_or(0) + carry;
                    result.push(sum % 10);
                    carry = sum / 10;
                }
            }
        }

        if carry != 0 {
            result.push(carry);
        }
        result.reverse();
        Self::from_digits(result)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, Error> {
        if self.cmp_value(rhs) == Ordering::Less {
            return Err(Error::Underflow);
        }

        let mut result = Vec::with_capacity(self.digits.len());
        let mut borrow = 0i16;
        let mut right = rhs.digits.iter().rev();

        for &digit in self.digits.iter().rev() {
            let mut value = digit as i16 - borrow - right.next().copied().unwrap_or(0) as i16;
            if value < 0 {
                value += 10;
                borrow = 1;
            } else {
                borrow = 0;
            }
            result.push(value as u8);
        }

        result.reverse();
        Ok(Self::from_digits(result))
    }

    fn from_digits(mut digits: Vec<u8>) -> Self {
        let first_nonzero = digits.iter().position(|&digit| digit != 0);
        match first_nonzero {
            Some(index) => {
                if index > 0 {
                    digits.drain(..index);
                }
            }
            None => digits = vec![0],
        }
        Self { digits }
    }
}