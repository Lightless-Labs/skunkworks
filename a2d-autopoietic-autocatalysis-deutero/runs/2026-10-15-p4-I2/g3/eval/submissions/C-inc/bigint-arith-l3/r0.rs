use std::cmp::Ordering;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    negative: bool,
    // Little-endian decimal digits. Zero is represented by [0] with negative=false.
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

        let mut digit_count = 0usize;
        let mut first_digit = None;
        let mut first_bad_separator = None;

        for index in start..bytes.len() {
            match bytes[index] {
                b'_' => {
                    let before_is_digit = index > start && bytes[index - 1].is_ascii_digit();
                    let after_is_digit =
                        index + 1 < bytes.len() && bytes[index + 1].is_ascii_digit();
                    if (!before_is_digit || !after_is_digit) && first_bad_separator.is_none() {
                        first_bad_separator = Some(index);
                    }
                }
                _ => {
                    first_digit.get_or_insert(index);
                    digit_count += 1;
                }
            }
        }

        if let Some(index) = first_bad_separator {
            return Err(Error::Separator { index });
        }

        if digit_count > 1 {
            let first = first_digit.expect("nonempty magnitude");
            if bytes[first] == b'0' {
                return Err(Error::LeadingZero { index: first });
            }
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

        let mut digits = Vec::with_capacity(digit_count.max(1));
        for &byte in bytes[start..].iter().rev() {
            if byte != b'_' {
                digits.push(byte - b'0');
            }
        }
        let mut result = Self { negative, digits };
        result.normalize();
        Ok(result)
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let magnitude_len = self.digits.len().max(1);
        let width = magnitude_len.max(min_digits);
        let padding = width - magnitude_len;
        let separator_count = if group == 0 { 0 } else { (width - 1) / group };
        let mut output = String::with_capacity(width + separator_count + usize::from(self.negative));

        if self.negative && !self.is_zero() {
            output.push('-');
        }

        for position in 0..width {
            if group > 0 && position > 0 && (width - position) % group == 0 {
                output.push('_');
            }
            let from_right = width - 1 - position;
            if from_right < padding {
                output.push('0');
            } else {
                let digit_index = from_right - padding;
                output.push(char::from(b'0' + self.digits[digit_index]));
            }
        }
        output
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.signum(), rhs.signum()) {
            (a, b) if a < b => Ordering::Less,
            (a, b) if a > b => Ordering::Greater,
            (0, 0) => Ordering::Equal,
            (-1, -1) => cmp_magnitude(&rhs.digits, &self.digits),
            _ => cmp_magnitude(&self.digits, &rhs.digits),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            let mut result = Self {
                negative: self.negative,
                digits: add_magnitudes(&self.digits, &rhs.digits),
            };
            result.normalize();
            result
        } else {
            match cmp_magnitude(&self.digits, &rhs.digits) {
                Ordering::Greater => {
                    let mut result = Self {
                        negative: self.negative,
                        digits: sub_magnitudes(&self.digits, &rhs.digits),
                    };
                    result.normalize();
                    result
                }
                Ordering::Less => {
                    let mut result = Self {
                        negative: rhs.negative,
                        digits: sub_magnitudes(&rhs.digits, &self.digits),
                    };
                    result.normalize();
                    result
                }
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

        let mut accum = vec![0u64; self.digits.len() + rhs.digits.len()];
        for (i, &a) in self.digits.iter().enumerate() {
            for (j, &b) in rhs.digits.iter().enumerate() {
                accum[i + j] += u64::from(a) * u64::from(b);
            }
        }

        let mut digits = Vec::with_capacity(accum.len() + 1);
        let mut carry = 0u64;
        for value in accum {
            let total = value + carry;
            digits.push((total % 10) as u8);
            carry = total / 10;
        }
        while carry > 0 {
            digits.push((carry % 10) as u8);
            carry /= 10;
        }

        let mut result = Self {
            negative: self.negative != rhs.negative,
            digits,
        };
        result.normalize();
        result
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }

        let (quotient_digits, remainder_digits) =
            div_magnitudes(&self.digits, &rhs.digits);

        let mut quotient = Self {
            negative: self.negative != rhs.negative,
            digits: quotient_digits,
        };
        quotient.normalize();

        let mut remainder = Self {
            negative: self.negative,
            digits: remainder_digits,
        };
        remainder.normalize();

        Ok((quotient, remainder))
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
}

fn cmp_magnitude(a: &[u8], b: &[u8]) -> Ordering {
    match a.len().cmp(&b.len()) {
        Ordering::Equal => {
            for (&x, &y) in a.iter().rev().zip(b.iter().rev()) {
                match x.cmp(&y) {
                    Ordering::Equal => {}
                    other => return other,
                }
            }
            Ordering::Equal
        }
        other => other,
    }
}

fn add_magnitudes(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(a.len().max(b.len()) + 1);
    let mut carry = 0u8;
    for i in 0..a.len().max(b.len()) {
        let sum = a.get(i).copied().unwrap_or(0) + b.get(i).copied().unwrap_or(0) + carry;
        result.push(sum % 10);
        carry = sum / 10;
    }
    if carry != 0 {
        result.push(carry);
    }
    result
}

// Requires a >= b.
fn sub_magnitudes(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(a.len());
    let mut borrow = 0i16;
    for i in 0..a.len() {
        let mut digit = i16::from(a[i]) - i16::from(b.get(i).copied().unwrap_or(0)) - borrow;
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

fn mul_magnitude_digit(a: &[u8], digit: u8) -> Vec<u8> {
    if digit == 0 {
        return vec![0];
    }
    let mut result = Vec::with_capacity(a.len() + 1);
    let mut carry = 0u16;
    for &value in a {
        let product = u16::from(value) * u16::from(digit) + carry;
        result.push((product % 10) as u8);
        carry = product / 10;
    }
    while carry != 0 {
        result.push((carry % 10) as u8);
        carry /= 10;
    }
    result
}

fn div_magnitudes(dividend: &[u8], divisor: &[u8]) -> (Vec<u8>, Vec<u8>) {
    if cmp_magnitude(dividend, divisor) == Ordering::Less {
        return (vec![0], dividend.to_vec());
    }

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
                remainder = sub_magnitudes(&remainder, &product);
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
}

fn main() {}