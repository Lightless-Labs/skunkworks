use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    negative: bool,
    // Little-endian base-10 digits. Zero is represented by [0] with negative=false.
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

        let (negative, start) = match bytes[0] {
            b'+' => (false, 1),
            b'-' => (true, 1),
            _ => (false, 0),
        };

        if start == bytes.len() {
            return Err(Error::SignOnly);
        }

        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' && !byte.is_ascii_digit() {
                return Err(Error::InvalidDigit { index });
            }
        }

        let mut digit_count = 0usize;
        let mut first_digit = None;
        let mut first_separator_error = None;

        for index in start..bytes.len() {
            if bytes[index] == b'_' {
                let prev_is_digit = index > start && bytes[index - 1].is_ascii_digit();
                let next_is_digit =
                    index + 1 < bytes.len() && bytes[index + 1].is_ascii_digit();
                if (!prev_is_digit || !next_is_digit) && first_separator_error.is_none() {
                    first_separator_error = Some(index);
                }
            } else {
                if first_digit.is_none() {
                    first_digit = Some(index);
                }
                digit_count += 1;
            }
        }

        if let Some(index) = first_separator_error {
            return Err(Error::Separator { index });
        }

        let first_digit = first_digit.expect("nonempty magnitude after sign check");
        if digit_count > 1 && bytes[first_digit] == b'0' {
            return Err(Error::LeadingZero { index: first_digit });
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
            if byte.is_ascii_digit() {
                digits.push(byte - b'0');
            }
        }

        let mut value = Self { negative, digits };
        value.normalize();
        Ok(value)
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let magnitude = self.magnitude_string();
        let width = min_digits.max(magnitude.len());
        let padding = width - magnitude.len();

        let mut out = String::with_capacity(width + width / group.max(1) + 1);
        if self.negative {
            out.push('-');
        }

        for i in 0..width {
            if group > 0 && i > 0 && (width - i) % group == 0 {
                out.push('_');
            }
            if i < padding {
                out.push('0');
            } else {
                out.push(magnitude.as_bytes()[i - padding] as char);
            }
        }
        out
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
            Self::from_parts(
                self.negative,
                add_magnitude(&self.digits, &rhs.digits),
            )
        } else {
            match cmp_magnitude(&self.digits, &rhs.digits) {
                Ordering::Greater => Self::from_parts(
                    self.negative,
                    sub_magnitude(&self.digits, &rhs.digits),
                ),
                Ordering::Less => Self::from_parts(
                    rhs.negative,
                    sub_magnitude(&rhs.digits, &self.digits),
                ),
                Ordering::Equal => Self::zero(),
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
        for (i, &a) in self.digits.iter().enumerate() {
            let mut carry = 0u16;
            for (j, &b) in rhs.digits.iter().enumerate() {
                let index = i + j;
                let product = result[index] as u16 + a as u16 * b as u16 + carry;
                result[index] = (product % 10) as u8;
                carry = product / 10;
            }

            let mut index = i + rhs.digits.len();
            while carry > 0 {
                if index == result.len() {
                    result.push(0);
                }
                let sum = result[index] as u16 + carry;
                result[index] = (sum % 10) as u8;
                carry = sum / 10;
                index += 1;
            }
        }

        Self::from_parts(self.negative != rhs.negative, result)
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }
        if self.is_zero() {
            return Ok((Self::zero(), Self::zero()));
        }

        let (quotient_digits, remainder_digits) =
            div_rem_magnitude(&self.digits, &rhs.digits);

        let quotient = Self::from_parts(self.negative != rhs.negative, quotient_digits);
        let remainder = Self::from_parts(self.negative, remainder_digits);
        Ok((quotient, remainder))
    }

    fn zero() -> Self {
        Self {
            negative: false,
            digits: vec![0],
        }
    }

    fn from_parts(negative: bool, digits: Vec<u8>) -> Self {
        let mut value = Self { negative, digits };
        value.normalize();
        value
    }

    fn normalize(&mut self) {
        while self.digits.len() > 1 && self.digits.last() == Some(&0) {
            self.digits.pop();
        }
        if self.digits.is_empty() {
            self.digits.push(0);
        }
        if self.digits.len() == 1 && self.digits[0] == 0 {
            self.negative = false;
        }
    }

    fn is_zero(&self) -> bool {
        self.digits.len() == 1 && self.digits[0] == 0
    }

    fn magnitude_string(&self) -> String {
        self.digits
            .iter()
            .rev()
            .map(|digit| (b'0' + *digit) as char)
            .collect()
    }
}

fn cmp_magnitude(a: &[u8], b: &[u8]) -> Ordering {
    match a.len().cmp(&b.len()) {
        Ordering::Equal => {
            for (x, y) in a.iter().rev().zip(b.iter().rev()) {
                match x.cmp(y) {
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

    for i in 0..a.len().max(b.len()) {
        let sum = a.get(i).copied().unwrap_or(0)
            + b.get(i).copied().unwrap_or(0)
            + carry;
        result.push(sum % 10);
        carry = sum / 10;
    }
    if carry > 0 {
        result.push(carry);
    }
    result
}

fn sub_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
    debug_assert!(cmp_magnitude(a, b) != Ordering::Less);
    let mut result = Vec::with_capacity(a.len());
    let mut borrow = 0i8;

    for i in 0..a.len() {
        let mut digit = a[i] as i8 - borrow - b.get(i).copied().unwrap_or(0) as i8;
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

fn mul_magnitude_digit(value: &[u8], digit: u8) -> Vec<u8> {
    if digit == 0 {
        return vec![0];
    }
    let mut result = Vec::with_capacity(value.len() + 1);
    let mut carry = 0u16;
    for &d in value {
        let product = d as u16 * digit as u16 + carry;
        result.push((product % 10) as u8);
        carry = product / 10;
    }
    while carry > 0 {
        result.push((carry % 10) as u8);
        carry /= 10;
    }
    result
}

fn div_rem_magnitude(dividend: &[u8], divisor: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut quotient_be = Vec::with_capacity(dividend.len());
    let mut remainder = vec![0u8];

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

fn trim_magnitude(value: &mut Vec<u8>) {
    while value.len() > 1 && value.last() == Some(&0) {
        value.pop();
    }
}

fn main() {}