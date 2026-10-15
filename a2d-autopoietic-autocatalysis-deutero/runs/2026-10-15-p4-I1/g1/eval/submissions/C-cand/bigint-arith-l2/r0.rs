use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    negative: bool,
    // Little-endian base-10 digits. Zero is represented as [0] and is never negative.
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

        let mut start = 0;
        let negative = match bytes[0] {
            b'+' => {
                start = 1;
                false
            }
            b'-' => {
                start = 1;
                true
            }
            _ => false,
        };

        if start == bytes.len() {
            return Err(Error::SignOnly);
        }

        // Invalid bytes take precedence over malformed separators.
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' && !byte.is_ascii_digit() {
                return Err(Error::InvalidDigit { index });
            }
        }

        let mut separator_error = None;
        for index in start..bytes.len() {
            if bytes[index] == b'_'
                && (index == start
                    || index + 1 == bytes.len()
                    || !bytes[index - 1].is_ascii_digit()
                    || !bytes[index + 1].is_ascii_digit())
            {
                separator_error = Some(index);
                break;
            }
        }
        if let Some(index) = separator_error {
            return Err(Error::Separator { index });
        }

        let first_digit_index = (start..bytes.len())
            .find(|&index| bytes[index] != b'_')
            .expect("nonempty magnitude has at least one digit");

        let digit_count = bytes[start..].iter().filter(|&&b| b != b'_').count();

        if digit_count > 1 && bytes[first_digit_index] == b'0' {
            return Err(Error::LeadingZero {
                index: first_digit_index,
            });
        }

        if digit_count > max_digits {
            let excess_number = max_digits + 1;
            let mut seen = 0usize;
            let mut excess_index = first_digit_index;
            for index in start..bytes.len() {
                if bytes[index] != b'_' {
                    seen += 1;
                    if seen == excess_number {
                        excess_index = index;
                        break;
                    }
                }
            }
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

        let is_zero = digits.iter().all(|&digit| digit == 0);
        Ok(Self {
            negative: negative && !is_zero,
            digits,
        })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let natural_len = self.digits.len().max(1);
        let magnitude_len = natural_len.max(min_digits);
        let padding = magnitude_len - natural_len;
        let separator_count = if group > 0 {
            (magnitude_len - 1) / group
        } else {
            0
        };

        let mut result = String::with_capacity(
            magnitude_len + separator_count + usize::from(self.negative),
        );
        if self.negative {
            result.push('-');
        }

        for position in 0..magnitude_len {
            if group > 0 && position > 0 && (magnitude_len - position) % group == 0 {
                result.push('_');
            }

            if position < padding {
                result.push('0');
            } else {
                let from_left = position - padding;
                let digit_index = natural_len - 1 - from_left;
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
        match (self.negative, rhs.negative) {
            (false, false) => Self::from_magnitude(add_magnitude(&self.digits, &rhs.digits), false),
            (true, true) => Self::from_magnitude(add_magnitude(&self.digits, &rhs.digits), true),
            (false, true) => match cmp_magnitude(&self.digits, &rhs.digits) {
                Ordering::Greater => Self::from_magnitude(
                    subtract_magnitude(&self.digits, &rhs.digits),
                    false,
                ),
                Ordering::Less => Self::from_magnitude(
                    subtract_magnitude(&rhs.digits, &self.digits),
                    true,
                ),
                Ordering::Equal => Self::zero(),
            },
            (true, false) => match cmp_magnitude(&self.digits, &rhs.digits) {
                Ordering::Greater => Self::from_magnitude(
                    subtract_magnitude(&self.digits, &rhs.digits),
                    true,
                ),
                Ordering::Less => Self::from_magnitude(
                    subtract_magnitude(&rhs.digits, &self.digits),
                    false,
                ),
                Ordering::Equal => Self::zero(),
            },
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

        let mut product = vec![0u8; self.digits.len() + rhs.digits.len()];
        for (i, &left) in self.digits.iter().enumerate() {
            let mut carry = 0u16;
            for (j, &right) in rhs.digits.iter().enumerate() {
                let index = i + j;
                let value = u16::from(product[index])
                    + u16::from(left) * u16::from(right)
                    + carry;
                product[index] = (value % 10) as u8;
                carry = value / 10;
            }

            let mut index = i + rhs.digits.len();
            while carry > 0 {
                let value = u16::from(product[index]) + carry;
                product[index] = (value % 10) as u8;
                carry = value / 10;
                index += 1;
            }
        }

        Self::from_magnitude(product, self.negative != rhs.negative)
    }

    fn zero() -> Self {
        Self {
            negative: false,
            digits: vec![0],
        }
    }

    fn is_zero(&self) -> bool {
        self.digits.len() == 1 && self.digits[0] == 0
    }

    fn from_magnitude(mut digits: Vec<u8>, negative: bool) -> Self {
        while digits.len() > 1 && digits.last() == Some(&0) {
            digits.pop();
        }
        let is_zero = digits.len() == 1 && digits[0] == 0;
        Self {
            negative: negative && !is_zero,
            digits,
        }
    }
}

fn cmp_magnitude(left: &[u8], right: &[u8]) -> Ordering {
    match left.len().cmp(&right.len()) {
        Ordering::Equal => left.iter().rev().cmp(right.iter().rev()),
        other => other,
    }
}

fn add_magnitude(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(left.len().max(right.len()) + 1);
    let mut carry = 0u8;
    for index in 0..left.len().max(right.len()) {
        let sum = left.get(index).copied().unwrap_or(0)
            + right.get(index).copied().unwrap_or(0)
            + carry;
        result.push(sum % 10);
        carry = sum / 10;
    }
    if carry > 0 {
        result.push(carry);
    }
    result
}

// Requires left magnitude >= right magnitude.
fn subtract_magnitude(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(left.len());
    let mut borrow = 0i16;
    for index in 0..left.len() {
        let mut digit = i16::from(left[index])
            - i16::from(right.get(index).copied().unwrap_or(0))
            - borrow;
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