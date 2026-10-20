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
    // Little-endian base-10 digits; zero is represented by [0].
    digits: Vec<u8>,
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

        let mut invalid = None;
        for (i, &b) in bytes.iter().enumerate().skip(start) {
            if b != b'_' && !b.is_ascii_digit() {
                invalid = Some(i);
                break;
            }
        }
        if let Some(index) = invalid {
            return Err(Error::InvalidDigit { index });
        }

        let mut malformed_separator = None;
        for i in start..bytes.len() {
            if bytes[i] == b'_'
                && (i == start
                    || i + 1 == bytes.len()
                    || !bytes[i - 1].is_ascii_digit()
                    || !bytes[i + 1].is_ascii_digit())
            {
                malformed_separator = Some(i);
                break;
            }
        }
        if let Some(index) = malformed_separator {
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
                .map(|(offset, _)| start + offset)
                .unwrap();
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index,
            });
        }

        let mut digits = Vec::with_capacity(digit_count);
        for &b in bytes[start..].iter().rev() {
            if b != b'_' {
                digits.push(b - b'0');
            }
        }
        Ok(Self::from_parts(negative, digits))
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let magnitude_len = self.digits.len().max(1);
        let width = magnitude_len.max(min_digits);
        let mut magnitude = String::with_capacity(width + width / group.max(1));
        for _ in magnitude_len..width {
            magnitude.push('0');
        }
        for &digit in self.digits.iter().rev() {
            magnitude.push(char::from(b'0' + digit));
        }

        if group > 0 && magnitude.len() > group {
            let first_group = magnitude.len() % group;
            let mut grouped = String::with_capacity(magnitude.len() + magnitude.len() / group);
            for (i, ch) in magnitude.chars().enumerate() {
                if i > 0 && (i == first_group || (i > first_group && (i - first_group) % group == 0)) {
                    grouped.push('_');
                }
                grouped.push(ch);
            }
            magnitude = grouped;
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
            (true, true) => cmp_magnitude(&rhs.digits, &self.digits),
            (false, false) => cmp_magnitude(&self.digits, &rhs.digits),
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
                let k = i + j;
                let product = result[k] as u16 + (a as u16) * (b as u16) + carry;
                result[k] = (product % 10) as u8;
                carry = product / 10;
            }
            let mut k = i + rhs.digits.len();
            while carry > 0 {
                let sum = result[k] as u16 + carry;
                result[k] = (sum % 10) as u8;
                carry = sum / 10;
                k += 1;
                if k == result.len() && carry > 0 {
                    result.push(0);
                }
            }
        }
        Self::from_parts(self.negative != rhs.negative, result)
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }

        let (quotient_digits, remainder_digits) =
            div_magnitude(&self.digits, &rhs.digits);
        let quotient = Self::from_parts(
            self.negative != rhs.negative,
            quotient_digits,
        );
        let remainder = Self::from_parts(self.negative, remainder_digits);
        Ok((quotient, remainder))
    }

    fn from_parts(negative: bool, mut digits: Vec<u8>) -> Self {
        normalize(&mut digits);
        let is_zero = digits.len() == 1 && digits[0] == 0;
        Self {
            negative: negative && !is_zero,
            digits,
        }
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
}

fn normalize(digits: &mut Vec<u8>) {
    while digits.len() > 1 && digits.last() == Some(&0) {
        digits.pop();
    }
    if digits.is_empty() {
        digits.push(0);
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

fn add_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(a.len().max(b.len()) + 1);
    let mut carry = 0u8;
    for i in 0..a.len().max(b.len()) {
        let sum = a.get(i).copied().unwrap_or(0) + b.get(i).copied().unwrap_or(0) + carry;
        result.push(sum % 10);
        carry = sum / 10;
    }
    if carry > 0 {
        result.push(carry);
    }
    result
}

fn sub_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(a.len());
    let mut borrow = 0i16;
    for i in 0..a.len() {
        let mut digit = a[i] as i16 - borrow - b.get(i).copied().unwrap_or(0) as i16;
        if digit < 0 {
            digit += 10;
            borrow = 1;
        } else {
            borrow = 0;
        }
        result.push(digit as u8);
    }
    normalize(&mut result);
    result
}

fn mul_small(a: &[u8], multiplier: u8) -> Vec<u8> {
    if multiplier == 0 {
        return vec![0];
    }
    let mut result = Vec::with_capacity(a.len() + 1);
    let mut carry = 0u16;
    for &digit in a {
        let product = digit as u16 * multiplier as u16 + carry;
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
    let mut remainder = vec![0u8];

    for &digit in dividend.iter().rev() {
        remainder.insert(0, digit);
        normalize(&mut remainder);

        let mut q_digit = 0u8;
        for candidate in (1..=9).rev() {
            let product = mul_small(divisor, candidate);
            if cmp_magnitude(&product, &remainder) != Ordering::Greater {
                q_digit = candidate;
                remainder = sub_magnitude(&remainder, &product);
                break;
            }
        }
        quotient_be.push(q_digit);
    }

    quotient_be.reverse();
    normalize(&mut quotient_be);
    normalize(&mut remainder);
    (quotient_be, remainder)
}