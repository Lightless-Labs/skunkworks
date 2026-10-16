use std::cmp::Ordering;
use std::fmt;

#[derive(Clone, PartialEq, Eq)]
pub enum Error {
    Empty,
    SignOnly,
    InvalidDigit { index: usize },
    Separator { index: usize },
    LeadingZero { index: usize },
    TooManyDigits { limit: usize, index: usize },
    DivisionByZero,
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("Empty"),
            Self::SignOnly => f.write_str("SignOnly"),
            Self::InvalidDigit { index } => f
                .debug_struct("InvalidDigit")
                .field("index", index)
                .finish(),
            Self::Separator { index } => f
                .debug_struct("Separator")
                .field("index", index)
                .finish(),
            Self::LeadingZero { index } => f
                .debug_struct("LeadingZero")
                .field("index", index)
                .finish(),
            Self::TooManyDigits { limit, index } => f
                .debug_struct("TooManyDigits")
                .field("limit", limit)
                .field("index", index)
                .finish(),
            Self::DivisionByZero => f.write_str("DivisionByZero"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    // Little-endian base-10 digits. Zero is represented by [0] with negative=false.
    digits: Vec<u8>,
    negative: bool,
}

impl Big {
    fn zero() -> Self {
        Self {
            digits: vec![0],
            negative: false,
        }
    }

    fn from_parts(digits: Vec<u8>, negative: bool) -> Self {
        let mut result = Self { digits, negative };
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

        // Invalid bytes have precedence over malformed separators.
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' && !byte.is_ascii_digit() {
                return Err(Error::InvalidDigit { index });
            }
        }

        let mut digit_count = 0usize;
        let mut first_digit_index = None;
        let mut first_digit = 0u8;
        let mut malformed_separator = None;
        let mut previous_was_digit = false;

        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte == b'_' {
                let next_is_digit = bytes
                    .get(index + 1)
                    .is_some_and(|next| next.is_ascii_digit());
                if !previous_was_digit || !next_is_digit {
                    malformed_separator.get_or_insert(index);
                }
                previous_was_digit = false;
            } else {
                if first_digit_index.is_none() {
                    first_digit_index = Some(index);
                    first_digit = byte - b'0';
                }
                digit_count += 1;
                previous_was_digit = true;
            }
        }

        if let Some(index) = malformed_separator {
            return Err(Error::Separator { index });
        }

        let first_index = first_digit_index.expect("nonempty magnitude after sign check");
        if digit_count > 1 && first_digit == 0 {
            return Err(Error::LeadingZero { index: first_index });
        }

        if digit_count > max_digits {
            let excess_number = max_digits + 1;
            let mut seen = 0usize;
            for (index, &byte) in bytes.iter().enumerate().skip(start) {
                if byte != b'_' {
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
            if byte != b'_' {
                digits.push(byte - b'0');
            }
        }

        Ok(Self::from_parts(digits, negative))
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let magnitude_len = self.digits.len().max(min_digits).max(1);
        let padding = magnitude_len - self.digits.len();
        let mut out = String::new();

        if self.negative {
            out.push('-');
        }

        for position in (0..magnitude_len).rev() {
            if group > 0 && position + 1 < magnitude_len && (position + 1) % group == 0 {
                out.push('_');
            }

            if position >= self.digits.len() {
                out.push('0');
            } else {
                out.push((b'0' + self.digits[position]) as char);
            }
        }

        let _ = padding;
        out
    }

    fn cmp_magnitude(&self, rhs: &Self) -> Ordering {
        self.digits
            .len()
            .cmp(&rhs.digits.len())
            .then_with(|| self.digits.iter().rev().cmp(rhs.digits.iter().rev()))
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.negative, rhs.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => self.cmp_magnitude(rhs),
            (true, true) => rhs.cmp_magnitude(self),
        }
    }

    fn add_magnitude(lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
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
        if carry != 0 {
            result.push(carry);
        }
        result
    }

    // Assumes lhs >= rhs.
    fn sub_magnitude(lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
        let mut result = Vec::with_capacity(lhs.len());
        let mut borrow = 0i8;

        for (i, &digit) in lhs.iter().enumerate() {
            let mut value = digit as i8 - borrow - rhs.get(i).copied().unwrap_or(0) as i8;
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

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            return Self::from_parts(
                Self::add_magnitude(&self.digits, &rhs.digits),
                self.negative,
            );
        }

        match self.cmp_magnitude(rhs) {
            Ordering::Greater => Self::from_parts(
                Self::sub_magnitude(&self.digits, &rhs.digits),
                self.negative,
            ),
            Ordering::Less => Self::from_parts(
                Self::sub_magnitude(&rhs.digits, &self.digits),
                rhs.negative,
            ),
            Ordering::Equal => Self::zero(),
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
        for (i, &left) in self.digits.iter().enumerate() {
            let mut carry = 0u16;
            for (j, &right) in rhs.digits.iter().enumerate() {
                let index = i + j;
                let product = result[index] as u16 + left as u16 * right as u16 + carry;
                result[index] = (product % 10) as u8;
                carry = product / 10;
            }

            let mut index = i + rhs.digits.len();
            while carry != 0 {
                if index == result.len() {
                    result.push(0);
                }
                let sum = result[index] as u16 + carry;
                result[index] = (sum % 10) as u8;
                carry = sum / 10;
                index += 1;
            }
        }

        Self::from_parts(result, self.negative != rhs.negative)
    }

    fn mul_magnitude_digit(digits: &[u8], factor: u8) -> Vec<u8> {
        if factor == 0 {
            return vec![0];
        }

        let mut result = Vec::with_capacity(digits.len() + 1);
        let mut carry = 0u16;
        for &digit in digits {
            let product = digit as u16 * factor as u16 + carry;
            result.push((product % 10) as u8);
            carry = product / 10;
        }
        while carry != 0 {
            result.push((carry % 10) as u8);
            carry /= 10;
        }
        result
    }

    fn cmp_digit_vectors(lhs: &[u8], rhs: &[u8]) -> Ordering {
        lhs.len()
            .cmp(&rhs.len())
            .then_with(|| lhs.iter().rev().cmp(rhs.iter().rev()))
    }

    fn sub_digit_vectors(lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
        Self::sub_magnitude(lhs, rhs)
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }

        if self.is_zero() {
            return Ok((Self::zero(), Self::zero()));
        }

        match self.cmp_magnitude(rhs) {
            Ordering::Less => return Ok((Self::zero(), self.clone())),
            Ordering::Equal => {
                let quotient = Self::from_parts(vec![1], self.negative != rhs.negative);
                return Ok((quotient, Self::zero()));
            }
            Ordering::Greater => {}
        }

        // Long division, consuming the dividend's digits from most to least significant.
        let mut quotient_msd = Vec::with_capacity(self.digits.len());
        let mut remainder = vec![0u8];

        for &digit in self.digits.iter().rev() {
            remainder.insert(0, digit);
            while remainder.len() > 1 && remainder.last() == Some(&0) {
                remainder.pop();
            }

            let mut quotient_digit = 0u8;
            for candidate in (1..=9u8).rev() {
                let product = Self::mul_magnitude_digit(&rhs.digits, candidate);
                if Self::cmp_digit_vectors(&product, &remainder) != Ordering::Greater {
                    quotient_digit = candidate;
                    remainder = Self::sub_digit_vectors(&remainder, &product);
                    break;
                }
            }
            quotient_msd.push(quotient_digit);
        }

        quotient_msd.reverse();
        let quotient = Self::from_parts(quotient_msd, self.negative != rhs.negative);
        let remainder = Self::from_parts(remainder, self.negative);
        Ok((quotient, remainder))
    }
}