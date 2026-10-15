use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    negative: bool,
    // Little-endian base-10 digits. Zero is represented as [0] with negative=false.
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
        let mut negative = false;
        if bytes[0] == b'+' || bytes[0] == b'-' {
            negative = bytes[0] == b'-';
            start = 1;
            if start == bytes.len() {
                return Err(Error::SignOnly);
            }
        }

        // Scan all magnitude bytes before considering separators.
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' && !byte.is_ascii_digit() {
                return Err(Error::InvalidDigit { index });
            }
        }

        let mut separator_error = None;
        for index in start..bytes.len() {
            if bytes[index] == b'_' {
                let previous_is_digit = index > start && bytes[index - 1].is_ascii_digit();
                let next_is_digit = index + 1 < bytes.len() && bytes[index + 1].is_ascii_digit();
                if !(previous_is_digit && next_is_digit) && separator_error.is_none() {
                    separator_error = Some(index);
                }
            }
        }
        if let Some(index) = separator_error {
            return Err(Error::Separator { index });
        }

        let mut first_digit_index = None;
        let mut digit_count = 0usize;
        let mut first_digit = 0u8;
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte.is_ascii_digit() {
                if first_digit_index.is_none() {
                    first_digit_index = Some(index);
                    first_digit = byte - b'0';
                }
                digit_count += 1;
            }
        }

        let first_index = first_digit_index.expect("validated magnitude contains digits");
        if digit_count > 1 && first_digit == 0 {
            return Err(Error::LeadingZero { index: first_index });
        }

        if digit_count > max_digits {
            let excess_number = max_digits + 1;
            let mut seen = 0usize;
            for (index, &byte) in bytes.iter().enumerate().skip(start) {
                if byte.is_ascii_digit() {
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
        for &byte in bytes.iter().skip(start) {
            if byte.is_ascii_digit() {
                digits.push(byte - b'0');
            }
        }
        digits.reverse();

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
        let mut magnitude = String::with_capacity(magnitude_len + magnitude_len / 2);

        for position in 0..magnitude_len {
            if group > 0 && position > 0 && (magnitude_len - position) % group == 0 {
                magnitude.push('_');
            }

            if position < padding {
                magnitude.push('0');
            } else {
                let digit_index = magnitude_len - 1 - position;
                magnitude.push(char::from(b'0' + self.digits[digit_index]));
            }
        }

        if self.negative {
            let mut result = String::with_capacity(magnitude.len() + 1);
            result.push('-');
            result.push_str(&magnitude);
            result
        } else {
            magnitude
        }
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.negative, rhs.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => compare_magnitudes(&self.digits, &rhs.digits),
            (true, true) => compare_magnitudes(&rhs.digits, &self.digits),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            let digits = add_magnitudes(&self.digits, &rhs.digits);
            Self::from_parts(self.negative, digits)
        } else {
            match compare_magnitudes(&self.digits, &rhs.digits) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => Self::from_parts(
                    self.negative,
                    subtract_magnitudes(&self.digits, &rhs.digits),
                ),
                Ordering::Less => Self::from_parts(
                    rhs.negative,
                    subtract_magnitudes(&rhs.digits, &self.digits),
                ),
            }
        }
    }

    pub fn sub(&self, rhs: &Self) -> Self {
        self.add(&rhs.neg())
    }

    pub fn neg(&self) -> Self {
        if self.is_zero() {
            Self::zero()
        } else {
            Self {
                negative: !self.negative,
                digits: self.digits.clone(),
            }
        }
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
                let value = result[index] as u16 + (left as u16 * right as u16) + carry;
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

    fn is_zero(&self) -> bool {
        self.digits.iter().all(|&digit| digit == 0)
    }

    fn from_parts(negative: bool, mut digits: Vec<u8>) -> Self {
        while digits.len() > 1 && digits.last() == Some(&0) {
            digits.pop();
        }
        let is_zero = digits.iter().all(|&digit| digit == 0);
        Self {
            negative: negative && !is_zero,
            digits,
        }
    }
}

fn compare_magnitudes(left: &[u8], right: &[u8]) -> Ordering {
    match left.len().cmp(&right.len()) {
        Ordering::Equal => {
            for (&a, &b) in left.iter().rev().zip(right.iter().rev()) {
                match a.cmp(&b) {
                    Ordering::Equal => {}
                    ordering => return ordering,
                }
            }
            Ordering::Equal
        }
        ordering => ordering,
    }
}

fn add_magnitudes(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(left.len().max(right.len()) + 1);
    let mut carry = 0u8;
    for index in 0..left.len().max(right.len()) {
        let a = left.get(index).copied().unwrap_or(0);
        let b = right.get(index).copied().unwrap_or(0);
        let sum = a + b + carry;
        result.push(sum % 10);
        carry = sum / 10;
    }
    if carry != 0 {
        result.push(carry);
    }
    result
}

// Requires left >= right.
fn subtract_magnitudes(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(left.len());
    let mut borrow = 0i8;
    for (index, &a) in left.iter().enumerate() {
        let b = right.get(index).copied().unwrap_or(0) as i8;
        let mut difference = a as i8 - b - borrow;
        if difference < 0 {
            difference += 10;
            borrow = 1;
        } else {
            borrow = 0;
        }
        result.push(difference as u8);
    }
    while result.len() > 1 && result.last() == Some(&0) {
        result.pop();
    }
    result
}

fn main() {}