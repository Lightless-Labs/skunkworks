use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    negative: bool,
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
    DivisionByZero,
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

        // Invalid bytes take precedence over every underscore error.
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' && !byte.is_ascii_digit() {
                return Err(Error::InvalidDigit { index });
            }
        }

        // Validate separators only after the full invalid-byte scan.
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

        let digit_positions: Vec<usize> = (start..bytes.len())
            .filter(|&index| bytes[index].is_ascii_digit())
            .collect();

        if digit_positions.len() > 1 && bytes[digit_positions[0]] == b'0' {
            return Err(Error::LeadingZero {
                index: digit_positions[0],
            });
        }

        if digit_positions.len() > max_digits {
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: digit_positions[max_digits],
            });
        }

        let mut digits: Vec<u8> = digit_positions
            .iter()
            .rev()
            .map(|&index| bytes[index] - b'0')
            .collect();
        if digits.is_empty() {
            digits.push(0);
        }

        let is_zero = digits.len() == 1 && digits[0] == 0;
        Ok(Self {
            negative: negative && !is_zero,
            digits,
        })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let magnitude_len = self.digits.len().max(1).max(min_digits);
        let padding = magnitude_len.saturating_sub(self.digits.len());
        let mut out = String::new();

        if self.negative && !self.is_zero() {
            out.push('-');
        }

        for position in 0..magnitude_len {
            if group > 0 && position > 0 && (magnitude_len - position) % group == 0 {
                out.push('_');
            }
            let digit = if position < padding {
                0
            } else {
                self.digits[self.digits.len() - 1 - (position - padding)]
            };
            out.push((b'0' + digit) as char);
        }
        out
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.is_negative(), rhs.is_negative()) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => cmp_magnitude(&self.digits, &rhs.digits),
            (true, true) => cmp_magnitude(&rhs.digits, &self.digits),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        match (self.is_negative(), rhs.is_negative()) {
            (false, false) => Self::from_parts(false, add_magnitude(&self.digits, &rhs.digits)),
            (true, true) => Self::from_parts(true, add_magnitude(&self.digits, &rhs.digits)),
            (false, true) => match cmp_magnitude(&self.digits, &rhs.digits) {
                Ordering::Greater => {
                    Self::from_parts(false, sub_magnitude(&self.digits, &rhs.digits))
                }
                Ordering::Less => {
                    Self::from_parts(true, sub_magnitude(&rhs.digits, &self.digits))
                }
                Ordering::Equal => Self::zero(),
            },
            (true, false) => match cmp_magnitude(&self.digits, &rhs.digits) {
                Ordering::Greater => {
                    Self::from_parts(true, sub_magnitude(&self.digits, &rhs.digits))
                }
                Ordering::Less => {
                    Self::from_parts(false, sub_magnitude(&rhs.digits, &self.digits))
                }
                Ordering::Equal => Self::zero(),
            },
        }
    }

    pub fn sub(&self, rhs: &Self) -> Self {
        self.add(&rhs.neg())
    }

    pub fn neg(&self) -> Self {
        Self::from_parts(!self.is_negative(), self.digits.clone())
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
        for (i, &a) in self.digits.iter().enumerate() {
            let mut carry = 0u16;
            for (j, &b) in rhs.digits.iter().enumerate() {
                let index = i + j;
                let total = result[index] as u16 + a as u16 * b as u16 + carry;
                result[index] = (total % 10) as u8;
                carry = total / 10;
            }

            let mut index = i + rhs.digits.len();
            while carry > 0 {
                let total = result[index] as u16 + carry;
                result[index] = (total % 10) as u8;
                carry = total / 10;
                index += 1;
            }
        }

        Self::from_parts(self.is_negative() != rhs.is_negative(), result)
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }
        if self.is_zero() {
            return Ok((Self::zero(), Self::zero()));
        }

        let (quotient_digits, remainder_digits) =
            div_magnitude(&self.digits, &rhs.digits);

        let quotient = Self::from_parts(
            self.is_negative() != rhs.is_negative(),
            quotient_digits,
        );
        let remainder = Self::from_parts(self.is_negative(), remainder_digits);
        Ok((quotient, remainder))
    }

    fn zero() -> Self {
        Self {
            negative: false,
            digits: vec![0],
        }
    }

    fn from_parts(negative: bool, digits: Vec<u8>) -> Self {
        let mut result = Self { negative, digits };
        result.normalize();
        result
    }

    fn normalize(&mut self) {
        while self.digits.len() > 1 && self.digits.last() == Some(&0) {
            self.digits.pop();
        }
        if self.digits.is_empty() {
            self.digits.push(0);
        }
        if self.is_zero() {
            self.negative = false;
        }
    }

    fn is_zero(&self) -> bool {
        self.digits.len() == 1 && self.digits[0] == 0
    }

    fn is_negative(&self) -> bool {
        self.negative && !self.is_zero()
    }
}

fn cmp_magnitude(a: &[u8], b: &[u8]) -> Ordering {
    match a.len().cmp(&b.len()) {
        Ordering::Equal => {
            for (left, right) in a.iter().rev().zip(b.iter().rev()) {
                match left.cmp(right) {
                    Ordering::Equal => {}
                    other => return other,
                }
            }
            Ordering::Equal
        }
        other => other,
    }
}

fn add_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(a.len().max(b.len()) + 1);
    let mut carry = 0u8;
    for index in 0..a.len().max(b.len()) {
        let sum = a.get(index).copied().unwrap_or(0)
            + b.get(index).copied().unwrap_or(0)
            + carry;
        result.push(sum % 10);
        carry = sum / 10;
    }
    if carry > 0 {
        result.push(carry);
    }
    result
}

// Requires a >= b.
fn sub_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(a.len());
    let mut borrow = 0i8;
    for (index, &digit) in a.iter().enumerate() {
        let mut value = digit as i8 - borrow - b.get(index).copied().unwrap_or(0) as i8;
        if value < 0 {
            value += 10;
            borrow = 1;
        } else {
            borrow = 0;
        }
        result.push(value as u8);
    }
    while result.len() > 1 && result.last() == Some(&0) {
        result.pop();
    }
    result
}

fn mul_magnitude_digit(a: &[u8], digit: u8) -> Vec<u8> {
    if digit == 0 {
        return vec![0];
    }
    let mut result = Vec::with_capacity(a.len() + 1);
    let mut carry = 0u16;
    for &value in a {
        let product = value as u16 * digit as u16 + carry;
        result.push((product % 10) as u8);
        carry = product / 10;
    }
    while carry > 0 {
        result.push((carry % 10) as u8);
        carry /= 10;
    }
    result
}

fn div_magnitude(dividend: &[u8], divisor: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut quotient_be = Vec::with_capacity(dividend.len());
    let mut remainder = vec![0];

    for &digit in dividend.iter().rev() {
        if remainder.len() == 1 && remainder[0] == 0 {
            remainder[0] = digit;
        } else {
            remainder.insert(0, digit);
        }
        trim_magnitude(&mut remainder);

        let mut quotient_digit = 0u8;
        for candidate in (1..=9).rev() {
            let product = mul_magnitude_digit(divisor, candidate);
            if cmp_magnitude(&product, &remainder) != Ordering::Greater {
                quotient_digit = candidate;
                remainder = sub_magnitude(&remainder, &product);
                break;
            }
        }
        quotient_be.push(quotient_digit);
    }

    quotient_be.reverse();
    trim_magnitude(&mut quotient_be);
    trim_magnitude(&mut remainder);
    (quotient_be, remainder)
}

fn trim_magnitude(digits: &mut Vec<u8>) {
    while digits.len() > 1 && digits.last() == Some(&0) {
        digits.pop();
    }
    if digits.is_empty() {
        digits.push(0);
    }
}

fn main() {}