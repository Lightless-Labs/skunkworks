use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    // Little-endian base-10 digits. Zero is represented as [0].
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

        // Invalid bytes take precedence over separator and numeric errors.
        if let Some(index) = (magnitude_start..bytes.len())
            .find(|&i| !bytes[i].is_ascii_digit() && bytes[i] != b'_')
        {
            return Err(Error::InvalidDigit { index });
        }

        let mut digit_count = 0usize;
        let mut first_digit_index = None;
        let mut first_digit = None;
        let mut first_bad_separator = None;

        for i in magnitude_start..bytes.len() {
            if bytes[i].is_ascii_digit() {
                digit_count += 1;
                if first_digit_index.is_none() {
                    first_digit_index = Some(i);
                    first_digit = Some(bytes[i]);
                }
            } else {
                let prev_is_digit = i > magnitude_start && bytes[i - 1].is_ascii_digit();
                let next_is_digit = i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit();
                if !(prev_is_digit && next_is_digit) && first_bad_separator.is_none() {
                    first_bad_separator = Some(i);
                }
            }
        }

        if let Some(index) = first_bad_separator {
            return Err(Error::Separator { index });
        }

        if digit_count > 1 && first_digit == Some(b'0') {
            return Err(Error::LeadingZero {
                index: first_digit_index.unwrap(),
            });
        }

        if digit_count > max_digits {
            let excess_number = max_digits + 1;
            let mut seen = 0usize;
            for i in magnitude_start..bytes.len() {
                if bytes[i].is_ascii_digit() {
                    seen += 1;
                    if seen == excess_number {
                        return Err(Error::TooManyDigits { limit: max_digits, index: i });
                    }
                }
            }
        }

        let mut digits = Vec::with_capacity(digit_count);
        for &byte in bytes[magnitude_start..].iter().rev() {
            if byte.is_ascii_digit() {
                digits.push(byte - b'0');
            }
        }

        Ok(Self { digits })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let significant_len = self.digits.len().max(1);
        let width = significant_len.max(min_digits);
        let mut out = String::with_capacity(width + if group > 0 { width / group } else { 0 });

        for position in 0..width {
            if group > 0 && position > 0 && (width - position) % group == 0 {
                out.push('_');
            }

            let from_right = width - 1 - position;
            let digit = if from_right < self.digits.len() {
                self.digits[from_right]
            } else {
                0
            };
            out.push(char::from(b'0' + digit));
        }

        out
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        let left_len = self.canonical_len();
        let right_len = rhs.canonical_len();

        match left_len.cmp(&right_len) {
            Ordering::Equal => {
                for i in (0..left_len).rev() {
                    match self.digit_at(i).cmp(&rhs.digit_at(i)) {
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
        let length = self.digits.len().max(rhs.digits.len());
        let mut result = Vec::with_capacity(length + 1);
        let mut carry = 0u8;

        for i in 0..length {
            let sum = self.digit_at(i) + rhs.digit_at(i) + carry;
            result.push(sum % 10);
            carry = sum / 10;
        }

        if carry != 0 {
            result.push(carry);
        }

        Self::from_digits(result)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, Error> {
        if self.cmp_value(rhs) == Ordering::Less {
            return Err(Error::Underflow);
        }

        let mut result = Vec::with_capacity(self.digits.len());
        let mut borrow = 0i8;

        for i in 0..self.digits.len() {
            let mut difference = self.digit_at(i) as i8 - rhs.digit_at(i) as i8 - borrow;
            if difference < 0 {
                difference += 10;
                borrow = 1;
            } else {
                borrow = 0;
            }
            result.push(difference as u8);
        }

        Ok(Self::from_digits(result))
    }

    fn digit_at(&self, index: usize) -> u8 {
        self.digits.get(index).copied().unwrap_or(0)
    }

    fn canonical_len(&self) -> usize {
        self.digits
            .iter()
            .rposition(|&digit| digit != 0)
            .map_or(1, |index| index + 1)
    }

    fn from_digits(mut digits: Vec<u8>) -> Self {
        while digits.len() > 1 && digits.last() == Some(&0) {
            digits.pop();
        }
        if digits.is_empty() {
            digits.push(0);
        }
        Self { digits }
    }
}