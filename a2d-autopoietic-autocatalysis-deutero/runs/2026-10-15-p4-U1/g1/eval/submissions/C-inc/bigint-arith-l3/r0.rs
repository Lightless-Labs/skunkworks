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
    // Little-endian base-10 digits. Zero is represented by [0].
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

        // Check each underscore against its immediate neighbors in the magnitude.
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

        let first_digit_index = (start..bytes.len())
            .find(|&index| bytes[index] != b'_')
            .expect("nonempty magnitude has a digit");

        let digit_count = bytes[start..].iter().filter(|&&byte| byte != b'_').count();

        if digit_count > 1 && bytes[first_digit_index] == b'0' {
            return Err(Error::LeadingZero {
                index: first_digit_index,
            });
        }

        if digit_count > max_digits {
            let excess_digit = max_digits + 1;
            let mut seen = 0;
            let index = (start..bytes.len())
                .find(|&index| {
                    if bytes[index] == b'_' {
                        return false;
                    }
                    seen += 1;
                    seen == excess_digit
                })
                .expect("excess digit exists");
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

        let mut result = Self { negative, digits };
        result.normalize();
        Ok(result)
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let magnitude_len = self.digits.len().max(min_digits).max(1);
        let padding = magnitude_len - self.digits.len();
        let mut magnitude = String::with_capacity(magnitude_len + magnitude_len / group.max(1));
        for _ in 0..padding {
            magnitude.push('0');
        }
        for &digit in self.digits.iter().rev() {
            magnitude.push(char::from(b'0' + digit));
        }

        let grouped = if group == 0 || magnitude_len <= group {
            magnitude
        } else {
            let first_group = magnitude_len % group;
            let first_group = if first_group == 0 { group } else { first_group };
            let mut out = String::with_capacity(magnitude_len + (magnitude_len - 1) / group);
            out.push_str(&magnitude[..first_group]);
            let mut index = first_group;
            while index < magnitude_len {
                out.push('_');
                out.push_str(&magnitude[index..index + group]);
                index += group;
            }
            out
        };

        if self.negative {
            let mut out = String::with_capacity(grouped.len() + 1);
            out.push('-');
            out.push_str(&grouped);
            out
        } else {
            grouped
        }
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.negative, rhs.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => Self::cmp_magnitude(self, rhs),
            (true, true) => Self::cmp_magnitude(rhs, self),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            let mut result = Self {
                negative: self.negative,
                digits: Self::add_magnitudes(&self.digits, &rhs.digits),
            };
            result.normalize();
            result
        } else {
            match Self::cmp_magnitude(self, rhs) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => {
                    let mut result = Self {
                        negative: self.negative,
                        digits: Self::sub_magnitudes(&self.digits, &rhs.digits),
                    };
                    result.normalize();
                    result
                }
                Ordering::Less => {
                    let mut result = Self {
                        negative: rhs.negative,
                        digits: Self::sub_magnitudes(&rhs.digits, &self.digits),
                    };
                    result.normalize();
                    result
                }
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

        let mut digits = vec![0u8; self.digits.len() + rhs.digits.len()];
        for (i, &left) in self.digits.iter().enumerate() {
            let mut carry = 0u16;
            for (j, &right) in rhs.digits.iter().enumerate() {
                let index = i + j;
                let product = digits[index] as u16 + left as u16 * right as u16 + carry;
                digits[index] = (product % 10) as u8;
                carry = product / 10;
            }

            let mut index = i + rhs.digits.len();
            while carry != 0 {
                let sum = digits[index] as u16 + carry;
                digits[index] = (sum % 10) as u8;
                carry = sum / 10;
                index += 1;
                if index == digits.len() && carry != 0 {
                    digits.push(0);
                }
            }
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
            Self::div_rem_magnitudes(&self.digits, &rhs.digits);

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
        if self.is_zero() {
            self.negative = false;
        }
    }

    fn cmp_magnitude(lhs: &Self, rhs: &Self) -> Ordering {
        match lhs.digits.len().cmp(&rhs.digits.len()) {
            Ordering::Equal => lhs.digits.iter().rev().cmp(rhs.digits.iter().rev()),
            order => order,
        }
    }

    fn add_magnitudes(lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
        let mut result = Vec::with_capacity(lhs.len().max(rhs.len()) + 1);
        let mut carry = 0u8;
        for index in 0..lhs.len().max(rhs.len()) {
            let sum = lhs.get(index).copied().unwrap_or(0)
                + rhs.get(index).copied().unwrap_or(0)
                + carry;
            result.push(sum % 10);
            carry = sum / 10;
        }
        if carry != 0 {
            result.push(carry);
        }
        result
    }

    // Requires lhs >= rhs.
    fn sub_magnitudes(lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
        let mut result = Vec::with_capacity(lhs.len());
        let mut borrow = 0i16;
        for index in 0..lhs.len() {
            let mut digit = lhs[index] as i16 - borrow - rhs.get(index).copied().unwrap_or(0) as i16;
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

    fn cmp_digit_vectors(lhs: &[u8], rhs: &[u8]) -> Ordering {
        match lhs.len().cmp(&rhs.len()) {
            Ordering::Equal => lhs.iter().rev().cmp(rhs.iter().rev()),
            order => order,
        }
    }

    fn sub_digit_vectors(lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
        Self::sub_magnitudes(lhs, rhs)
    }

    fn mul_digit_vector_by_digit(value: &[u8], multiplier: u8) -> Vec<u8> {
        if multiplier == 0 {
            return vec![0];
        }
        let mut result = Vec::with_capacity(value.len() + 1);
        let mut carry = 0u16;
        for &digit in value {
            let product = digit as u16 * multiplier as u16 + carry;
            result.push((product % 10) as u8);
            carry = product / 10;
        }
        while carry != 0 {
            result.push((carry % 10) as u8);
            carry /= 10;
        }
        result
    }

    fn div_rem_magnitudes(dividend: &[u8], divisor: &[u8]) -> (Vec<u8>, Vec<u8>) {
        if Self::cmp_digit_vectors(dividend, divisor) == Ordering::Less {
            return (vec![0], dividend.to_vec());
        }

        // Long division, processing dividend digits from most significant to least.
        let mut quotient_be = Vec::with_capacity(dividend.len());
        let mut remainder = vec![0];

        for &digit in dividend.iter().rev() {
            if remainder.len() == 1 && remainder[0] == 0 {
                remainder[0] = digit;
            } else {
                remainder.insert(0, digit);
            }
            while remainder.len() > 1 && remainder.last() == Some(&0) {
                remainder.pop();
            }

            let mut quotient_digit = 0u8;
            for candidate in (1..=9).rev() {
                let product = Self::mul_digit_vector_by_digit(divisor, candidate);
                if Self::cmp_digit_vectors(&product, &remainder) != Ordering::Greater {
                    quotient_digit = candidate;
                    remainder = Self::sub_digit_vectors(&remainder, &product);
                    break;
                }
            }
            quotient_be.push(quotient_digit);
        }

        while quotient_be.len() > 1 && quotient_be.first() == Some(&0) {
            quotient_be.remove(0);
        }
        quotient_be.reverse();

        while remainder.len() > 1 && remainder.last() == Some(&0) {
            remainder.pop();
        }

        (quotient_be, remainder)
    }
}

fn main() {}