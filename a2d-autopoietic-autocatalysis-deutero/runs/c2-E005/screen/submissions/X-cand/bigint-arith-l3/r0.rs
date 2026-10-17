use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    negative: bool,
    // Little-endian base-10 digits. Zero is represented by [0] and is never negative.
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

        let sign_len = usize::from(bytes[0] == b'+' || bytes[0] == b'-');
        if sign_len == bytes.len() {
            return Err(Error::SignOnly);
        }

        let negative = sign_len == 1 && bytes[0] == b'-';
        let mut invalid = None;
        for (i, &b) in bytes.iter().enumerate().skip(sign_len) {
            if b != b'_' && !b.is_ascii_digit() {
                invalid = Some(i);
                break;
            }
        }
        if let Some(index) = invalid {
            return Err(Error::InvalidDigit { index });
        }

        let mut separator = None;
        for i in sign_len..bytes.len() {
            if bytes[i] == b'_'
                && (i == sign_len
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

        let first_digit = (sign_len..bytes.len()).find(|&i| bytes[i] != b'_').unwrap();
        let digit_count = bytes[sign_len..].iter().filter(|&&b| b != b'_').count();

        if digit_count > 1 && bytes[first_digit] == b'0' {
            return Err(Error::LeadingZero { index: first_digit });
        }

        if digit_count > max_digits {
            let target = max_digits + 1;
            let mut seen = 0usize;
            for (i, &b) in bytes.iter().enumerate().skip(sign_len) {
                if b != b'_' {
                    seen += 1;
                    if seen == target {
                        return Err(Error::TooManyDigits { limit: max_digits, index: i });
                    }
                }
            }
        }

        let mut digits = Vec::with_capacity(digit_count);
        for &b in bytes[sign_len..].iter().rev() {
            if b != b'_' {
                digits.push(b - b'0');
            }
        }
        let mut value = Self { negative, digits };
        value.normalize();
        Ok(value)
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let width = min_digits.max(self.digits.len()).max(1);
        let pad = width - self.digits.len();
        let mut magnitude = String::with_capacity(width + width.saturating_sub(1) / group.max(1));
        for _ in 0..pad {
            magnitude.push('0');
        }
        for &d in self.digits.iter().rev() {
            magnitude.push(char::from(b'0' + d));
        }

        if group > 0 && magnitude.len() > group {
            let mut grouped = String::with_capacity(magnitude.len() + magnitude.len() / group);
            let first = magnitude.len() % group;
            let first = if first == 0 { group } else { first };
            grouped.push_str(&magnitude[..first]);
            for chunk in magnitude[first..].as_bytes().chunks(group) {
                grouped.push('_');
                grouped.push_str(std::str::from_utf8(chunk).expect("ASCII digits"));
            }
            magnitude = grouped;
        }

        if self.negative {
            let mut out = String::with_capacity(magnitude.len() + 1);
            out.push('-');
            out.push_str(&magnitude);
            out
        } else {
            magnitude
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
            let mut result = Self {
                negative: self.negative,
                digits: add_magnitude(&self.digits, &rhs.digits),
            };
            result.normalize();
            result
        } else {
            match cmp_magnitude(&self.digits, &rhs.digits) {
                Ordering::Greater => {
                    let mut result = Self {
                        negative: self.negative,
                        digits: sub_magnitude(&self.digits, &rhs.digits),
                    };
                    result.normalize();
                    result
                }
                Ordering::Less => {
                    let mut result = Self {
                        negative: rhs.negative,
                        digits: sub_magnitude(&rhs.digits, &self.digits),
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
        let mut result = Self {
            negative: self.negative ^ rhs.negative,
            digits: mul_magnitude(&self.digits, &rhs.digits),
        };
        result.normalize();
        result
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }

        let (quotient, remainder) = div_magnitude(&self.digits, &rhs.digits);
        let mut q = Self {
            negative: self.negative ^ rhs.negative,
            digits: quotient,
        };
        let mut r = Self {
            negative: self.negative,
            digits: remainder,
        };
        q.normalize();
        r.normalize();
        Ok((q, r))
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
        let mut digit = a[i] as i8 - b.get(i).copied().unwrap_or(0) as i8 - borrow;
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

fn mul_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; a.len() + b.len()];
    for (i, &x) in a.iter().enumerate() {
        let mut carry = 0u16;
        for (j, &y) in b.iter().enumerate() {
            let k = i + j;
            let value = out[k] as u16 + x as u16 * y as u16 + carry;
            out[k] = (value % 10) as u8;
            carry = value / 10;
        }
        let mut k = i + b.len();
        while carry != 0 {
            let value = out[k] as u16 + carry;
            out[k] = (value % 10) as u8;
            carry = value / 10;
            k += 1;
            if k == out.len() && carry != 0 {
                out.push(0);
            }
        }
    }
    while out.len() > 1 && out.last() == Some(&0) {
        out.pop();
    }
    out
}

// Long division on little-endian base-10 magnitudes.
fn div_magnitude(dividend: &[u8], divisor: &[u8]) -> (Vec<u8>, Vec<u8>) {
    if cmp_magnitude(dividend, divisor) == Ordering::Less {
        return (vec![0], dividend.to_vec());
    }

    let mut quotient_be = Vec::with_capacity(dividend.len());
    let mut remainder = vec![0u8];

    for &digit in dividend.iter().rev() {
        remainder.insert(0, digit);
        trim_magnitude(&mut remainder);

        let mut qdigit = 0u8;
        while cmp_magnitude(&remainder, divisor) != Ordering::Less {
            remainder = sub_magnitude(&remainder, divisor);
            qdigit += 1;
        }
        quotient_be.push(qdigit);
    }

    while quotient_be.len() > 1 && quotient_be.first() == Some(&0) {
        quotient_be.remove(0);
    }
    quotient_be.reverse();
    trim_magnitude(&mut remainder);
    (quotient_be, remainder)
}

fn trim_magnitude(value: &mut Vec<u8>) {
    while value.len() > 1 && value.last() == Some(&0) {
        value.pop();
    }
    if value.is_empty() {
        value.push(0);
    }
}