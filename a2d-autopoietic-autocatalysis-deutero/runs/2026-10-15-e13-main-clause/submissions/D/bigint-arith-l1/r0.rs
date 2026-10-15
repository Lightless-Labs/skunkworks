use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    // Decimal digits, most significant first; zero is represented as [0].
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

        let magnitude_start = if bytes[0] == b'+' { 1 } else { 0 };
        if magnitude_start == bytes.len() {
            return Err(Error::SignOnly);
        }

        // Scan all magnitude bytes for invalid characters before checking
        // separator placement.
        if let Some(index) = (magnitude_start..bytes.len())
            .find(|&i| !bytes[i].is_ascii_digit() && bytes[i] != b'_')
        {
            return Err(Error::InvalidDigit { index });
        }

        // Underscores must have a digit on both sides.
        if let Some(index) = (magnitude_start..bytes.len()).find(|&i| {
            bytes[i] == b'_'
                && (i == magnitude_start
                    || i + 1 == bytes.len()
                    || !bytes[i - 1].is_ascii_digit()
                    || !bytes[i + 1].is_ascii_digit())
        }) {
            return Err(Error::Separator { index });
        }

        let first_digit_index = (magnitude_start..bytes.len())
            .find(|&i| bytes[i] != b'_')
            .expect("nonempty magnitude with valid separators");

        let digit_count = bytes[magnitude_start..]
            .iter()
            .filter(|&&byte| byte != b'_')
            .count();

        if digit_count > 1 && bytes[first_digit_index] == b'0' {
            return Err(Error::LeadingZero {
                index: first_digit_index,
            });
        }

        if digit_count > max_digits {
            let excess_digit_number = max_digits + 1;
            let index = bytes[magnitude_start..]
                .iter()
                .enumerate()
                .filter(|(_, byte)| **byte != b'_')
                .nth(excess_digit_number - 1)
                .map(|(offset, _)| magnitude_start + offset)
                .expect("excess digit exists when count exceeds limit");
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index,
            });
        }

        let digits = bytes[magnitude_start..]
            .iter()
            .filter(|&&byte| byte != b'_')
            .map(|&byte| byte - b'0')
            .collect();

        Ok(Self { digits })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let width = self.digits.len().max(min_digits).max(1);
        let padding = width - self.digits.len();
        let mut output = String::with_capacity(width + width.saturating_sub(1) / group.max(1));

        for position in 0..width {
            if group > 0 && position > 0 && (width - position) % group == 0 {
                output.push('_');
            }

            if position < padding {
                output.push('0');
            } else {
                output.push(char::from(b'0' + self.digits[position - padding]));
            }
        }

        output
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match self.digits.len().cmp(&rhs.digits.len()) {
            Ordering::Equal => self.digits.cmp(&rhs.digits),
            ordering => ordering,
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        let mut left = self.digits.iter().rev();
        let mut right = rhs.digits.iter().rev();
        let mut result = Vec::with_capacity(self.digits.len().max(rhs.digits.len()) + 1);
        let mut carry = 0u8;

        loop {
            let a = left.next().copied();
            let b = right.next().copied();
            if a.is_none() && b.is_none() && carry == 0 {
                break;
            }

            let sum = a.unwrap_or(0) + b.unwrap_or(0) + carry;
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

        let mut left = self.digits.iter().rev();
        let mut right = rhs.digits.iter().rev();
        let mut result = Vec::with_capacity(self.digits.len());
        let mut borrow = 0i16;

        while let Some(&digit) = left.next() {
            let mut value = i16::from(digit) - borrow;
            let subtrahend = right.next().copied().map(i16::from).unwrap_or(0);
            if value < subtrahend {
                value += 10;
                borrow = 1;
            } else {
                borrow = 0;
            }
            result.push((value - subtrahend) as u8);
        }

        while result.len() > 1 && result.last() == Some(&0) {
            result.pop();
        }
        result.reverse();

        Ok(Self { digits: result })
    }
}