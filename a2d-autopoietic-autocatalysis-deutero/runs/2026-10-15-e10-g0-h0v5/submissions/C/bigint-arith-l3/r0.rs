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
    // Little-endian base-10 digits; zero is represented as [0].
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

        let mut separator = None;
        for i in start..bytes.len() {
            if bytes[i] == b'_'
                && (i == start
                    || i + 1 == bytes.len()
                    || !bytes[i - 1].is_ascii_digit()
                    || !bytes[i + 1].is_ascii_digit())
            {
                separator = Some(i);
                break;
            }
        }
        if let Some(index) = separator {
            return Err(Error::Separator { index });
        }

        let digit_count = bytes[start..].iter().filter(|&&b| b != b'_').count();
        let first_digit_index = (start..bytes.len()).find(|&i| bytes[i] != b'_').unwrap();

        if digit_count > 1 && bytes[first_digit_index] == b'0' {
            return Err(Error::LeadingZero {
                index: first_digit_index,
            });
        }

        if digit_count > max_digits {
            let excess_number = max_digits + 1;
            let mut seen = 0usize;
            let mut excess_index = first_digit_index;
            for (i, &b) in bytes.iter().enumerate().skip(start) {
                if b != b'_' {
                    seen += 1;
                    if seen == excess_number {
                        excess_index = i;
                        break;
                    }
                }
            }
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: excess_index,
            });
        }

        let digits = bytes[start..]
            .iter()
            .filter(|&&b| b != b'_')
            .rev()
            .map(|&b| b - b'0')
            .collect();

        Ok(Self::canonical(negative, digits))
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let raw: String = self.digits.iter().rev().map(|d| char::from(b'0' + *d)).collect();
        let width = min_digits.max(raw.len()).max(1);
        let mut magnitude = String::with_capacity(width + width / group.max(1));
        for _ in 0..width.saturating_sub(raw.len()) {
            magnitude.push('0');
        }
        magnitude.push_str(&raw);

        if group == 0 {
            return if self.negative {
                format!("-{magnitude}")
            } else {
                magnitude
            };
        }

        let mut grouped = String::with_capacity(magnitude.len() + magnitude.len() / group);
        for (i, ch) in magnitude.chars().enumerate() {
            if i > 0 && (magnitude.len() - i) % group == 0 {
                grouped.push('_');
            }
            grouped.push(ch);
        }

        if self.negative {
            format!("-{grouped}")
        } else {
            grouped
        }
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
            Self::canonical(
                self.negative,
                add_magnitude(&self.digits, &rhs.digits),
            )
        } else {
            match cmp_magnitude(&self.digits, &rhs.digits) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => Self::canonical(
                    self.negative,
                    sub_magnitude(&self.digits, &rhs.digits),
                ),
                Ordering::Less => Self::canonical(
                    rhs.negative,
                    sub_magnitude(&rhs.digits, &self.digits),
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
        for (i, &a) in self.digits.iter().enumerate() {
            let mut carry = 0u16;
            for (j, &b) in rhs.digits.iter().enumerate() {
                let k = i + j;
                let value = result[k] as u16 + a as u16 * b as u16 + carry;
                result[k] = (value % 10) as u8;
                carry = value / 10;
            }
            let mut k = i + rhs.digits.len();
            while carry > 0 {
                if k == result.len() {
                    result.push(0);
                }
                let value = result[k] as u16 + carry;
                result[k] = (value % 10) as u8;
                carry = value / 10;
                k += 1;
            }
        }
        Self::canonical(self.negative != rhs.negative, result)
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }
        if self.is_zero() {
            return Ok((Self::zero(), Self::zero()));
        }

        let (q_digits, r_digits) = div_magnitude(&self.digits, &rhs.digits);
        let quotient = Self::canonical(self.negative != rhs.negative, q_digits);
        let remainder = Self::canonical(self.negative, r_digits);
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

    fn canonical(negative: bool, mut digits: Vec<u8>) -> Self {
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

fn cmp_magnitude(a: &[u8], b: &[u8]) -> Ordering {
    match a.len().cmp(&b.len()) {
        Ordering::Equal => a.iter().rev().cmp(b.iter().rev()),
        other => other,
    }
}

fn add_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(a.len().max(b.len()) + 1);
    let mut carry = 0u8;
    for i in 0..a.len().max(b.len()) {
        let sum = a.get(i).copied().unwrap_or(0) + b.get(i).copied().unwrap_or(0) + carry;
        out.push(sum % 10);
        carry = sum / 10;
    }
    if carry != 0 {
        out.push(carry);
    }
    out
}

// Requires a >= b.
fn sub_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(a.len());
    let mut borrow = 0i8;
    for i in 0..a.len() {
        let mut digit = a[i] as i8 - borrow - b.get(i).copied().unwrap_or(0) as i8;
        if digit < 0 {
            digit += 10;
            borrow = 1;
        } else {
            borrow = 0;
        }
        out.push(digit as u8);
    }
    while out.len() > 1 && out.last() == Some(&0) {
        out.pop();
    }
    out
}

fn mul_magnitude_digit(a: &[u8], digit: u8) -> Vec<u8> {
    if digit == 0 {
        return vec![0];
    }
    let mut out = Vec::with_capacity(a.len() + 1);
    let mut carry = 0u16;
    for &value in a {
        let product = value as u16 * digit as u16 + carry;
        out.push((product % 10) as u8);
        carry = product / 10;
    }
    while carry > 0 {
        out.push((carry % 10) as u8);
        carry /= 10;
    }
    out
}

fn div_magnitude(a: &[u8], b: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut quotient_be = Vec::with_capacity(a.len());
    let mut remainder = vec![0u8];

    for &digit in a.iter().rev() {
        if remainder.len() == 1 && remainder[0] == 0 {
            remainder[0] = digit;
        } else {
            remainder.insert(0, digit);
        }

        let mut q_digit = 0u8;
        for candidate in (1..=9).rev() {
            if cmp_magnitude(&mul_magnitude_digit(b, candidate), &remainder) != Ordering::Greater {
                q_digit = candidate;
                break;
            }
        }
        if q_digit != 0 {
            remainder = sub_magnitude(&remainder, &mul_magnitude_digit(b, q_digit));
        }
        quotient_be.push(q_digit);
    }

    while quotient_be.len() > 1 && quotient_be.first() == Some(&0) {
        quotient_be.remove(0);
    }
    quotient_be.reverse();
    (quotient_be, remainder)
}

fn main() {}