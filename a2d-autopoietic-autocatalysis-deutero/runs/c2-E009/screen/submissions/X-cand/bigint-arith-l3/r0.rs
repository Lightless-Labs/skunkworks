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

        let (negative, start) = match bytes[0] {
            b'+' => (false, 1),
            b'-' => (true, 1),
            _ => (false, 0),
        };

        if start == bytes.len() {
            return Err(Error::SignOnly);
        }

        // Invalid bytes anywhere take precedence over malformed separators.
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

        let first_digit = (start..bytes.len())
            .find(|&index| bytes[index] != b'_')
            .expect("a nonempty magnitude has a digit");

        let digit_count = bytes[start..].iter().filter(|&&b| b != b'_').count();

        if digit_count > 1 && bytes[first_digit] == b'0' {
            return Err(Error::LeadingZero { index: first_digit });
        }

        if digit_count > max_digits {
            let excess_number = max_digits + 1;
            let mut seen = 0usize;
            for index in start..bytes.len() {
                if bytes[index] != b'_' {
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

        let mut digits = Vec::new();
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
        let magnitude_len = self.digits.len().max(min_digits).max(1);
        let mut out = String::new();

        if self.negative {
            out.push('-');
        }

        let padding = magnitude_len.saturating_sub(self.digits.len());
        for position in (0..magnitude_len).rev() {
            if group > 0 && position + 1 < magnitude_len && (position + 1) % group == 0 {
                out.push('_');
            }

            if position >= self.digits.len() {
                out.push('0');
            } else {
                out.push(char::from(b'0' + self.digits[position]));
            }
        }

        let _ = padding;
        out
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.signum(), rhs.signum()) {
            (a, b) if a != b => a.cmp(&b),
            (0, 0) => Ordering::Equal,
            (-1, -1) => cmp_magnitude(&rhs.digits, &self.digits),
            _ => cmp_magnitude(&self.digits, &rhs.digits),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            let digits = add_magnitudes(&self.digits, &rhs.digits);
            Self {
                negative: self.negative,
                digits,
            }
            .normalized()
        } else {
            match cmp_magnitude(&self.digits, &rhs.digits) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => Self {
                    negative: self.negative,
                    digits: sub_magnitudes(&self.digits, &rhs.digits),
                }
                .normalized(),
                Ordering::Less => Self {
                    negative: rhs.negative,
                    digits: sub_magnitudes(&rhs.digits, &self.digits),
                }
                .normalized(),
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
            while carry != 0 {
                let total = result[index] as u16 + carry;
                result[index] = (total % 10) as u8;
                carry = total / 10;
                index += 1;
            }
        }

        Self {
            negative: self.negative != rhs.negative,
            digits: result,
        }
        .normalized()
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }

        match cmp_magnitude(&self.digits, &rhs.digits) {
            Ordering::Less => return Ok((Self::zero(), self.clone())),
            Ordering::Equal => {
                return Ok((
                    Self {
                        negative: self.negative != rhs.negative,
                        digits: vec![1],
                    },
                    Self::zero(),
                ));
            }
            Ordering::Greater => {}
        }

        let mut quotient = vec![0u8; self.digits.len()];
        let mut remainder = Self::zero();

        for position in (0..self.digits.len()).rev() {
            remainder.shift_base_add(self.digits[position]);

            let mut quotient_digit = 0u8;
            while cmp_magnitude(&remainder.digits, &rhs.digits) != Ordering::Less {
                remainder.digits = sub_magnitudes(&remainder.digits, &rhs.digits);
                remainder.normalize();
                quotient_digit += 1;
            }
            quotient[position] = quotient_digit;
        }

        let quotient = Self {
            negative: self.negative != rhs.negative,
            digits: quotient,
        }
        .normalized();

        remainder.negative = self.negative && !remainder.is_zero();
        Ok((quotient, remainder.normalized()))
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
        if self.is_zero() {
            self.negative = false;
        }
    }

    fn normalized(mut self) -> Self {
        self.normalize();
        self
    }

    fn shift_base_add(&mut self, digit: u8) {
        if self.is_zero() {
            self.digits[0] = digit;
        } else {
            self.digits.insert(0, digit);
        }
        self.normalize();
    }
}

fn cmp_magnitude(lhs: &[u8], rhs: &[u8]) -> Ordering {
    match lhs.len().cmp(&rhs.len()) {
        Ordering::Equal => {
            for (a, b) in lhs.iter().rev().zip(rhs.iter().rev()) {
                match a.cmp(b) {
                    Ordering::Equal => {}
                    ordering => return ordering,
                }
            }
            Ordering::Equal
        }
        ordering => ordering,
    }
}

fn add_magnitudes(lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut carry = 0u8;
    let length = lhs.len().max(rhs.len());

    for index in 0..length {
        let a = lhs.get(index).copied().unwrap_or(0);
        let b = rhs.get(index).copied().unwrap_or(0);
        let sum = a + b + carry;
        result.push(sum % 10);
        carry = sum / 10;
    }

    if carry != 0 {
        result.push(carry);
    }
    result
}

fn sub_magnitudes(lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut borrow = 0i8;

    for (index, &a) in lhs.iter().enumerate() {
        let b = rhs.get(index).copied().unwrap_or(0) as i8;
        let mut digit = a as i8 - borrow - b;
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