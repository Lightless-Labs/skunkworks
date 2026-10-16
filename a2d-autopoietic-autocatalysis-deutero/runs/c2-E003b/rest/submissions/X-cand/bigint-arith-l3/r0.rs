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
    // Little-endian base-10 digits. Zero is represented by [0] and is never negative.
    digits: Vec<u8>,
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

        for i in start..bytes.len() {
            if bytes[i] == b'_'
                && (i == start
                    || i + 1 == bytes.len()
                    || !bytes[i - 1].is_ascii_digit()
                    || !bytes[i + 1].is_ascii_digit())
            {
                return Err(Error::Separator { index: i });
            }
        }

        let first_digit = (start..bytes.len()).find(|&i| bytes[i] != b'_').unwrap();
        let digit_count = bytes[start..].iter().filter(|&&b| b != b'_').count();

        if digit_count > 1 && bytes[first_digit] == b'0' {
            return Err(Error::LeadingZero { index: first_digit });
        }

        if digit_count > max_digits {
            let excess = max_digits;
            let index = bytes[start..]
                .iter()
                .enumerate()
                .filter(|(_, b)| **b != b'_')
                .nth(excess)
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
        let is_zero = digits.iter().all(|&d| d == 0);
        Ok(Self {
            negative: negative && !is_zero,
            digits,
        })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let actual = self.digits.len().max(1);
        let width = actual.max(min_digits);
        let mut magnitude = String::with_capacity(width + width / group.max(1));
        for _ in actual..width {
            magnitude.push('0');
        }
        for &digit in self.digits.iter().rev() {
            magnitude.push((b'0' + digit) as char);
        }

        if group > 0 {
            let mut grouped = String::with_capacity(magnitude.len() + magnitude.len() / group);
            for (i, ch) in magnitude.chars().enumerate() {
                if i > 0 && (magnitude.len() - i) % group == 0 {
                    grouped.push('_');
                }
                grouped.push(ch);
            }
            magnitude = grouped;
        }

        if self.negative {
            magnitude.insert(0, '-');
        }
        magnitude
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.negative, rhs.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => cmp_mag(&self.digits, &rhs.digits),
            (true, true) => cmp_mag(&rhs.digits, &self.digits),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            Self::from_mag(add_mag(&self.digits, &rhs.digits), self.negative)
        } else {
            match cmp_mag(&self.digits, &rhs.digits) {
                Ordering::Greater => Self::from_mag(sub_mag(&self.digits, &rhs.digits), self.negative),
                Ordering::Less => Self::from_mag(sub_mag(&rhs.digits, &self.digits), rhs.negative),
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
        Self::from_mag(mul_mag(&self.digits, &rhs.digits), self.negative ^ rhs.negative)
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }
        let (quotient, remainder) = div_rem_mag(&self.digits, &rhs.digits);
        Ok((
            Self::from_mag(quotient, self.negative ^ rhs.negative),
            Self::from_mag(remainder, self.negative),
        ))
    }

    fn zero() -> Self {
        Self {
            negative: false,
            digits: vec![0],
        }
    }

    fn is_zero(&self) -> bool {
        self.digits.iter().all(|&d| d == 0)
    }

    fn from_mag(digits: Vec<u8>, negative: bool) -> Self {
        let digits = normalize(digits);
        let zero = digits.len() == 1 && digits[0] == 0;
        Self {
            negative: negative && !zero,
            digits,
        }
    }
}

fn normalize(mut digits: Vec<u8>) -> Vec<u8> {
    while digits.len() > 1 && digits.last() == Some(&0) {
        digits.pop();
    }
    if digits.is_empty() {
        digits.push(0);
    }
    digits
}

fn cmp_mag(a: &[u8], b: &[u8]) -> Ordering {
    let a = trim_mag(a);
    let b = trim_mag(b);
    match a.len().cmp(&b.len()) {
        Ordering::Equal => {
            for i in (0..a.len()).rev() {
                match a[i].cmp(&b[i]) {
                    Ordering::Equal => {}
                    other => return other,
                }
            }
            Ordering::Equal
        }
        other => other,
    }
}

fn trim_mag(mut a: &[u8]) -> &[u8] {
    while a.len() > 1 && a.last() == Some(&0) {
        a = &a[..a.len() - 1];
    }
    a
}

fn add_mag(a: &[u8], b: &[u8]) -> Vec<u8> {
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

fn sub_mag(a: &[u8], b: &[u8]) -> Vec<u8> {
    // Requires a >= b.
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
    normalize(result)
}

fn mul_mag(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut result = vec![0u8; a.len() + b.len()];
    for (i, &x) in a.iter().enumerate() {
        let mut carry = 0u16;
        for (j, &y) in b.iter().enumerate() {
            let index = i + j;
            let value = result[index] as u16 + x as u16 * y as u16 + carry;
            result[index] = (value % 10) as u8;
            carry = value / 10;
        }
        let mut index = i + b.len();
        while carry > 0 {
            let value = result[index] as u16 + carry;
            result[index] = (value % 10) as u8;
            carry = value / 10;
            index += 1;
            if index == result.len() && carry > 0 {
                result.push(0);
            }
        }
    }
    normalize(result)
}

fn div_rem_mag(dividend: &[u8], divisor: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let dividend = trim_mag(dividend);
    let divisor = trim_mag(divisor);
    if cmp_mag(dividend, divisor) == Ordering::Less {
        return (vec![0], dividend.to_vec());
    }

    let mut quotient_be = Vec::with_capacity(dividend.len());
    let mut remainder = vec![0];
    for &digit in dividend.iter().rev() {
        remainder.insert(0, digit);
        remainder = normalize(remainder);

        let mut qdigit = 0u8;
        while cmp_mag(&remainder, divisor) != Ordering::Less {
            remainder = sub_mag(&remainder, divisor);
            qdigit += 1;
        }
        quotient_be.push(qdigit);
    }
    quotient_be.reverse();
    (normalize(quotient_be), normalize(remainder))
}