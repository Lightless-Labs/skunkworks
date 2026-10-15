use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Empty,
    SignOnly,
    InvalidDigit { index: usize },
    Separator { index: usize },
    LeadingZero { index: usize },
    TooManyDigits { limit: usize, index: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    negative: bool,
    // Little-endian base-10 digits; zero is represented by [0].
    digits: Vec<u8>,
}

impl Big {
    pub fn parse(text: &str, max_digits: usize) -> Result<Self, Error> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(Error::Empty);
        }

        let (negative, start) = match bytes[0] {
            b'+' => (false, 1),
            b'-' => (true, 1),
            _ => (false, 0),
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

        let mut first_bad_separator = None;
        for index in start..bytes.len() {
            if bytes[index] == b'_'
                && (index == start
                    || index + 1 == bytes.len()
                    || !bytes[index - 1].is_ascii_digit()
                    || !bytes[index + 1].is_ascii_digit())
            {
                first_bad_separator = Some(index);
                break;
            }
        }
        if let Some(index) = first_bad_separator {
            return Err(Error::Separator { index });
        }

        let digit_count = bytes[start..].iter().filter(|&&b| b != b'_').count();
        let first_digit_index = (start..bytes.len())
            .find(|&i| bytes[i] != b'_')
            .expect("a nonempty magnitude contains a digit");

        if digit_count > 1 && bytes[first_digit_index] == b'0' {
            return Err(Error::LeadingZero {
                index: first_digit_index,
            });
        }

        if digit_count > max_digits {
            let excess_number = max_digits;
            let index = (start..bytes.len())
                .filter(|&i| bytes[i] != b'_')
                .nth(excess_number)
                .expect("digit count exceeds limit");
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index,
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
        let magnitude_len = self.digits.len().max(1);
        let output_len = magnitude_len.max(min_digits);
        let padding = output_len - magnitude_len;

        let mut result = String::new();
        if self.negative {
            result.push('-');
        }

        for position in 0..output_len {
            if group > 0 && position > 0 && (output_len - position) % group == 0 {
                result.push('_');
            }
            if position < padding {
                result.push('0');
            } else {
                let digit_index = output_len - 1 - position;
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
            let digits = add_magnitude(&self.digits, &rhs.digits);
            Self {
                negative: self.negative && !is_zero_digits(&digits),
                digits,
            }
        } else {
            match cmp_magnitude(&self.digits, &rhs.digits) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => {
                    let digits = sub_magnitude(&self.digits, &rhs.digits);
                    Self {
                        negative: self.negative && !is_zero_digits(&digits),
                        digits,
                    }
                }
                Ordering::Less => {
                    let digits = sub_magnitude(&rhs.digits, &self.digits);
                    Self {
                        negative: rhs.negative && !is_zero_digits(&digits),
                        digits,
                    }
                }
            }
        }
    }

    pub fn sub(&self, rhs: &Self) -> Self {
        self.add(&rhs.neg())
    }

    pub fn neg(&self) -> Self {
        let mut result = self.clone();
        if !is_zero_digits(&result.digits) {
            result.negative = !result.negative;
        }
        result
    }

    pub fn signum(&self) -> i8 {
        if is_zero_digits(&self.digits) {
            0
        } else if self.negative {
            -1
        } else {
            1
        }
    }

    pub fn mul(&self, rhs: &Self) -> Self {
        if is_zero_digits(&self.digits) || is_zero_digits(&rhs.digits) {
            return Self::zero();
        }

        let mut accum = vec![0u32; self.digits.len() + rhs.digits.len()];
        for (i, &a) in self.digits.iter().enumerate() {
            for (j, &b) in rhs.digits.iter().enumerate() {
                accum[i + j] += u32::from(a) * u32::from(b);
            }
        }

        for i in 0..accum.len() - 1 {
            let carry = accum[i] / 10;
            accum[i] %= 10;
            accum[i + 1] += carry;
        }

        let mut digits: Vec<u8> = accum.into_iter().map(|n| n as u8).collect();
        while digits.len() > 1 && digits.last() == Some(&0) {
            digits.pop();
        }

        Self {
            negative: self.negative != rhs.negative,
            digits,
        }
    }

    fn zero() -> Self {
        Self {
            negative: false,
            digits: vec![0],
        }
    }
}

fn is_zero_digits(digits: &[u8]) -> bool {
    digits.iter().all(|&digit| digit == 0)
}

fn cmp_magnitude(lhs: &[u8], rhs: &[u8]) -> Ordering {
    match lhs.len().cmp(&rhs.len()) {
        Ordering::Equal => {
            for (&a, &b) in lhs.iter().rev().zip(rhs.iter().rev()) {
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

fn add_magnitude(lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(lhs.len().max(rhs.len()) + 1);
    let mut carry = 0u16;
    for index in 0..lhs.len().max(rhs.len()) {
        let a = lhs.get(index).copied().unwrap_or(0) as u16;
        let b = rhs.get(index).copied().unwrap_or(0) as u16;
        let sum = a + b + carry;
        result.push((sum % 10) as u8);
        carry = sum / 10;
    }
    if carry != 0 {
        result.push(carry as u8);
    }
    result
}

fn sub_magnitude(lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
    // Requires lhs >= rhs.
    let mut result = Vec::with_capacity(lhs.len());
    let mut borrow = 0i16;
    for (index, &a) in lhs.iter().enumerate() {
        let b = rhs.get(index).copied().unwrap_or(0) as i16;
        let mut digit = a as i16 - b - borrow;
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