use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    negative: bool,
    // Little-endian base-10 digits. Zero is represented as [0] with negative=false.
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

        let mut invalid = None;
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' && !byte.is_ascii_digit() {
                invalid = Some(index);
                break;
            }
        }
        if let Some(index) = invalid {
            return Err(Error::InvalidDigit { index });
        }

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

        let mut digit_count = 0usize;
        let mut first_digit = None;
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte.is_ascii_digit() {
                if first_digit.is_none() {
                    first_digit = Some((index, byte));
                }
                digit_count += 1;
            }
        }

        let (first_index, first_byte) = first_digit.expect("validated magnitude has a digit");
        if digit_count > 1 && first_byte == b'0' {
            return Err(Error::LeadingZero { index: first_index });
        }

        if digit_count > max_digits {
            let excess_number = max_digits + 1;
            let mut seen = 0usize;
            for (index, &byte) in bytes.iter().enumerate().skip(start) {
                if byte.is_ascii_digit() {
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
        let mut result = Self { negative, digits };
        result.normalize();
        Ok(result)
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let actual = self.digits.len().max(1);
        let width = actual.max(min_digits);
        let mut out = String::with_capacity(width + width / group.max(1) + usize::from(self.negative));
        if self.negative {
            out.push('-');
        }

        for position in 0..width {
            if group > 0 && position > 0 && (width - position) % group == 0 {
                out.push('_');
            }
            let from_right = width - 1 - position;
            let digit = if from_right < self.digits.len() {
                self.digits[from_right]
            } else {
                0
            };
            out.push((b'0' + digit) as char);
        }
        out
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.is_negative(), rhs.is_negative()) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => Self::cmp_magnitude(self, rhs),
            (true, true) => Self::cmp_magnitude(rhs, self),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        match (self.is_negative(), rhs.is_negative()) {
            (false, false) => Self::from_magnitude(Self::add_magnitudes(&self.digits, &rhs.digits), false),
            (true, true) => Self::from_magnitude(Self::add_magnitudes(&self.digits, &rhs.digits), true),
            (false, true) => match Self::cmp_magnitude(self, rhs) {
                Ordering::Greater | Ordering::Equal => {
                    Self::from_magnitude(Self::sub_magnitudes(&self.digits, &rhs.digits), false)
                }
                Ordering::Less => {
                    Self::from_magnitude(Self::sub_magnitudes(&rhs.digits, &self.digits), true)
                }
            },
            (true, false) => match Self::cmp_magnitude(self, rhs) {
                Ordering::Greater => {
                    Self::from_magnitude(Self::sub_magnitudes(&self.digits, &rhs.digits), true)
                }
                Ordering::Equal => Self::zero(),
                Ordering::Less => {
                    Self::from_magnitude(Self::sub_magnitudes(&rhs.digits, &self.digits), false)
                }
            },
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

        let mut product = vec![0u8; self.digits.len() + rhs.digits.len()];
        for (i, &a) in self.digits.iter().enumerate() {
            let mut carry = 0u16;
            for (j, &b) in rhs.digits.iter().enumerate() {
                let index = i + j;
                let value = product[index] as u16 + (a as u16) * (b as u16) + carry;
                product[index] = (value % 10) as u8;
                carry = value / 10;
            }
            let mut index = i + rhs.digits.len();
            while carry > 0 {
                let value = product[index] as u16 + carry;
                product[index] = (value % 10) as u8;
                carry = value / 10;
                index += 1;
                if index == product.len() && carry > 0 {
                    product.push(0);
                }
            }
        }

        Self::from_magnitude(product, self.negative ^ rhs.negative)
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }

        let dividend = self.abs_digits();
        let divisor = rhs.abs_digits();
        if Self::cmp_digits(&dividend, &divisor) == Ordering::Less {
            return Ok((Self::zero(), self.clone()));
        }

        let mut quotient_be = Vec::with_capacity(dividend.len());
        let mut remainder = vec![0u8];

        for &digit in dividend.iter().rev() {
            if remainder.len() == 1 && remainder[0] == 0 {
                remainder[0] = digit;
            } else {
                remainder.insert(0, digit);
            }
            Self::trim_digits(&mut remainder);

            let mut qdigit = 0u8;
            while Self::cmp_digits(&remainder, &divisor) != Ordering::Less {
                remainder = Self::sub_digits(&remainder, &divisor);
                qdigit += 1;
            }
            quotient_be.push(qdigit);
        }

        quotient_be.reverse();
        let quotient = Self::from_magnitude(quotient_be, self.negative ^ rhs.negative);
        let remainder = Self::from_magnitude(remainder, self.negative);
        Ok((quotient, remainder))
    }

    fn zero() -> Self {
        Self {
            negative: false,
            digits: vec![0],
        }
    }

    fn from_magnitude(mut digits: Vec<u8>, negative: bool) -> Self {
        Self::trim_digits(&mut digits);
        let is_zero = digits.len() == 1 && digits[0] == 0;
        Self {
            negative: negative && !is_zero,
            digits,
        }
    }

    fn normalize(&mut self) {
        Self::trim_digits(&mut self.digits);
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

    fn abs_digits(&self) -> Vec<u8> {
        self.digits.clone()
    }

    fn trim_digits(digits: &mut Vec<u8>) {
        while digits.len() > 1 && digits.last() == Some(&0) {
            digits.pop();
        }
        if digits.is_empty() {
            digits.push(0);
        }
    }

    fn cmp_magnitude(lhs: &Self, rhs: &Self) -> Ordering {
        Self::cmp_digits(&lhs.digits, &rhs.digits)
    }

    fn cmp_digits(lhs: &[u8], rhs: &[u8]) -> Ordering {
        match lhs.len().cmp(&rhs.len()) {
            Ordering::Equal => {
                for i in (0..lhs.len()).rev() {
                    match lhs[i].cmp(&rhs[i]) {
                        Ordering::Equal => {}
                        other => return other,
                    }
                }
                Ordering::Equal
            }
            other => other,
        }
    }

    fn add_magnitudes(lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
        let length = lhs.len().max(rhs.len());
        let mut result = Vec::with_capacity(length + 1);
        let mut carry = 0u8;
        for i in 0..length {
            let sum = lhs.get(i).copied().unwrap_or(0)
                + rhs.get(i).copied().unwrap_or(0)
                + carry;
            result.push(sum % 10);
            carry = sum / 10;
        }
        if carry > 0 {
            result.push(carry);
        }
        result
    }

    // Requires lhs >= rhs.
    fn sub_magnitudes(lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
        Self::sub_digits(lhs, rhs)
    }

    // Little-endian decimal digits; requires lhs >= rhs.
    fn sub_digits(lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
        let mut result = Vec::with_capacity(lhs.len());
        let mut borrow = 0i8;
        for i in 0..lhs.len() {
            let mut digit = lhs[i] as i8 - borrow - rhs.get(i).copied().unwrap_or(0) as i8;
            if digit < 0 {
                digit += 10;
                borrow = 1;
            } else {
                borrow = 0;
            }
            result.push(digit as u8);
        }
        Self::trim_digits(&mut result);
        result
    }
}