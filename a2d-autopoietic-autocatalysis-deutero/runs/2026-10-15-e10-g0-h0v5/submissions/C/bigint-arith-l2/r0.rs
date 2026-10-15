use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    negative: bool,
    // Little-endian decimal digits. Zero is represented by [0] with negative=false.
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
}

impl Big {
    pub fn parse(text: &str, max_digits: usize) -> Result<Self, Error> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(Error::Empty);
        }

        let mut negative = false;
        let mut start = 0;
        if bytes[0] == b'+' || bytes[0] == b'-' {
            negative = bytes[0] == b'-';
            start = 1;
            if start == bytes.len() {
                return Err(Error::SignOnly);
            }
        }

        // Invalid bytes outrank all separator and numeric-shape errors.
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' && !byte.is_ascii_digit() {
                return Err(Error::InvalidDigit { index });
            }
        }

        let mut first_digit_index = None;
        let mut digit_count = 0usize;
        let mut first_digit = 0u8;
        let mut malformed_separator = None;

        for index in start..bytes.len() {
            let byte = bytes[index];
            if byte == b'_' {
                let prev_is_digit = index > start && bytes[index - 1].is_ascii_digit();
                let next_is_digit =
                    index + 1 < bytes.len() && bytes[index + 1].is_ascii_digit();
                if (!prev_is_digit || !next_is_digit) && malformed_separator.is_none() {
                    malformed_separator = Some(index);
                }
            } else {
                if first_digit_index.is_none() {
                    first_digit_index = Some(index);
                    first_digit = byte - b'0';
                }
                digit_count += 1;
            }
        }

        if let Some(index) = malformed_separator {
            return Err(Error::Separator { index });
        }

        let first_index = first_digit_index.expect("nonempty magnitude after sign");
        if digit_count > 1 && first_digit == 0 {
            return Err(Error::LeadingZero { index: first_index });
        }

        if digit_count > max_digits {
            let excess_number = max_digits + 1;
            let mut seen = 0usize;
            for index in start..bytes.len() {
                if bytes[index].is_ascii_digit() {
                    seen += 1;
                    if seen == excess_number {
                        return Err(Error::TooManyDigits {
                            limit: max_digits,
                            index,
                        });
                    }
                }
            }
        }

        let mut digits = Vec::with_capacity(digit_count);
        for &byte in bytes[start..].iter().rev() {
            if byte != b'_' {
                digits.push(byte - b'0');
            }
        }
        let is_zero = digits.iter().all(|&digit| digit == 0);
        Ok(Self {
            negative: negative && !is_zero,
            digits,
        })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let actual_len = self.digits.len().max(1);
        let width = actual_len.max(min_digits);
        let padding = width - actual_len;
        let separators = if group == 0 { 0 } else { (width - 1) / group };
        let mut result = String::with_capacity(width + separators + usize::from(self.negative));
        if self.negative {
            result.push('-');
        }

        for position in 0..width {
            if group > 0 && position > 0 && (width - position) % group == 0 {
                result.push('_');
            }
            if position < padding {
                result.push('0');
            } else {
                let from_left = position - padding;
                let digit_index = actual_len - 1 - from_left;
                result.push(char::from(b'0' + self.digits[digit_index]));
            }
        }
        result
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.negative, rhs.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => cmp_magnitude(&self.digits, &rhs.digits),
            (true, true) => cmp_magnitude(&rhs.digits, &self.digits),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            let digits = add_magnitudes(&self.digits, &rhs.digits);
            Self::from_parts(self.negative, digits)
        } else {
            match cmp_magnitude(&self.digits, &rhs.digits) {
                Ordering::Greater => Self::from_parts(
                    self.negative,
                    subtract_magnitudes(&self.digits, &rhs.digits),
                ),
                Ordering::Less => Self::from_parts(
                    rhs.negative,
                    subtract_magnitudes(&rhs.digits, &self.digits),
                ),
                Ordering::Equal => Self::zero(),
            }
        }
    }

    pub fn sub(&self, rhs: &Self) -> Self {
        self.add(&rhs.neg())
    }

    pub fn neg(&self) -> Self {
        let mut result = self.clone();
        if !result.is_zero() {
            result.negative = !result.negative;
        }
        result
    }

    pub fn signum(&self) -> i8 {
        if self.is_zero() {
            0
        } else if self.negative {
            -1
        } else {
            1
        }
    }

    pub fn mul(&self, rhs: &Self) -> Self {
        if self.is_zero() || rhs.is_zero() {
            return Self::zero();
        }

        let mut result = vec![0u8; self.digits.len() + rhs.digits.len()];
        for (i, &left) in self.digits.iter().enumerate() {
            let mut carry = 0u16;
            for (j, &right) in rhs.digits.iter().enumerate() {
                let index = i + j;
                let value = result[index] as u16 + left as u16 * right as u16 + carry;
                result[index] = (value % 10) as u8;
                carry = value / 10;
            }

            let mut index = i + rhs.digits.len();
            while carry > 0 {
                if index == result.len() {
                    result.push(0);
                }
                let value = result[index] as u16 + carry;
                result[index] = (value % 10) as u8;
                carry = value / 10;
                index += 1;
            }
        }

        Self::from_parts(self.negative ^ rhs.negative, result)
    }

    fn zero() -> Self {
        Self {
            negative: false,
            digits: vec![0],
        }
    }

    fn from_parts(negative: bool, mut digits: Vec<u8>) -> Self {
        while digits.len() > 1 && digits.last() == Some(&0) {
            digits.pop();
        }
        let is_zero = digits.len() == 1 && digits[0] == 0;
        Self {
            negative: negative && !is_zero,
            digits,
        }
    }

    fn is_zero(&self) -> bool {
        self.digits.len() == 1 && self.digits[0] == 0
    }
}

fn cmp_magnitude(left: &[u8], right: &[u8]) -> Ordering {
    left.len()
        .cmp(&right.len())
        .then_with(|| left.iter().rev().cmp(right.iter().rev()))
}

fn add_magnitudes(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(left.len().max(right.len()) + 1);
    let mut carry = 0u8;
    for index in 0..left.len().max(right.len()) {
        let sum = left.get(index).copied().unwrap_or(0)
            + right.get(index).copied().unwrap_or(0)
            + carry;
        result.push(sum % 10);
        carry = sum / 10;
    }
    if carry != 0 {
        result.push(carry);
    }
    result
}

fn subtract_magnitudes(larger: &[u8], smaller: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(larger.len());
    let mut borrow = 0i8;
    for index in 0..larger.len() {
        let mut digit = larger[index] as i8 - borrow
            - smaller.get(index).copied().unwrap_or(0) as i8;
        if digit < 0 {
            digit += 10;
            borrow = 1;
        } else {
            borrow = 0;
        }
        result.push(digit as u8);
    }
    while result.len() > 1 && result.last() == Some(&0) {
        result.pop();
    }
    result
}

fn main() {}