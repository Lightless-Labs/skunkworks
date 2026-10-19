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
    // Little-endian base-10 digits. Zero is represented as [0] and is never negative.
    digits: Vec<u8>,
}

impl Big {
    pub fn parse(text: &str, max_digits: usize) -> Result<Self, Error> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(Error::Empty);
        }

        let start = match bytes[0] {
            b'+' | b'-' => 1,
            _ => 0,
        };
        let negative = bytes.first() == Some(&b'-');

        if start == bytes.len() {
            return Err(Error::SignOnly);
        }

        // Scan all bytes first so invalid bytes take precedence over separators.
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' && !byte.is_ascii_digit() {
                return Err(Error::InvalidDigit { index });
            }
        }

        // Validate separators after the invalid-byte scan has succeeded.
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

        let mut first_digit_index = None;
        let mut digit_count = 0usize;
        let mut excess_index = None;
        let mut digits = Vec::new();

        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte.is_ascii_digit() {
                if first_digit_index.is_none() {
                    first_digit_index = Some(index);
                }
                digit_count = digit_count.saturating_add(1);
                if digit_count > max_digits && excess_index.is_none() {
                    excess_index = Some(index);
                }
                digits.push(byte - b'0');
            }
        }

        if digit_count > 1 && digits.first() == Some(&0) {
            return Err(Error::LeadingZero {
                index: first_digit_index.unwrap(),
            });
        }

        if let Some(index) = excess_index {
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index,
            });
        }

        digits.reverse();
        let mut result = Self { negative, digits };
        result.normalize();
        Ok(result)
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let magnitude_len = self.digits.len().max(1);
        let width = magnitude_len.max(min_digits);
        let mut out = String::new();

        if self.negative {
            out.push('-');
        }

        let padding = width - magnitude_len;
        for position in 0..width {
            if group > 0 && position > 0 && (width - position) % group == 0 {
                out.push('_');
            }

            let digit = if position < padding {
                0
            } else {
                self.digits[magnitude_len - 1 - (position - padding)]
            };
            out.push((b'0' + digit) as char);
        }

        out
    }

    pub fn cmp_value(&self, rhs: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;

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
                std::cmp::Ordering::Greater => {
                    let mut result = Self {
                        negative: self.negative,
                        digits: sub_magnitude(&self.digits, &rhs.digits),
                    };
                    result.normalize();
                    result
                }
                std::cmp::Ordering::Less => {
                    let mut result = Self {
                        negative: rhs.negative,
                        digits: sub_magnitude(&rhs.digits, &self.digits),
                    };
                    result.normalize();
                    result
                }
                std::cmp::Ordering::Equal => Self::zero(),
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

        let mut accum = vec![0u32; self.digits.len() + rhs.digits.len()];
        for (i, &a) in self.digits.iter().enumerate() {
            for (j, &b) in rhs.digits.iter().enumerate() {
                accum[i + j] += u32::from(a) * u32::from(b);
            }
        }

        for i in 0..accum.len() - 1 {
            let carry = accum[i] / 10;
            accum[i] %= 10;
            accum[i + 1] += carry;
        }

        let mut digits: Vec<u8> = accum.into_iter().map(|n| (n % 10) as u8).collect();
        while digits.len() > 1 && digits.last() == Some(&0) {
            digits.pop();
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

        match cmp_magnitude(&self.digits, &rhs.digits) {
            std::cmp::Ordering::Less => return Ok((Self::zero(), self.clone())),
            std::cmp::Ordering::Equal => {
                let quotient = Self {
                    negative: self.negative != rhs.negative,
                    digits: vec![1],
                };
                return Ok((quotient, Self::zero()));
            }
            std::cmp::Ordering::Greater => {}
        }

        let mut quotient_be = Vec::with_capacity(self.digits.len());
        let mut remainder = vec![0u8];

        for &digit in self.digits.iter().rev() {
            remainder.insert(0, digit);
            trim_magnitude(&mut remainder);

            let mut qdigit = 0u8;
            while cmp_magnitude(&remainder, &rhs.digits) != std::cmp::Ordering::Less {
                remainder = sub_magnitude(&remainder, &rhs.digits);
                qdigit += 1;
            }
            quotient_be.push(qdigit);
        }

        quotient_be.reverse();
        trim_magnitude(&mut quotient_be);
        trim_magnitude(&mut remainder);

        let mut quotient = Self {
            negative: self.negative != rhs.negative,
            digits: quotient_be,
        };
        quotient.normalize();

        let mut rem = Self {
            negative: self.negative,
            digits: remainder,
        };
        rem.normalize();

        Ok((quotient, rem))
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
        trim_magnitude(&mut self.digits);
        if self.is_zero() {
            self.negative = false;
        }
    }
}

fn trim_magnitude(digits: &mut Vec<u8>) {
    while digits.len() > 1 && digits.last() == Some(&0) {
        digits.pop();
    }
    if digits.is_empty() {
        digits.push(0);
    }
}

fn cmp_magnitude(a: &[u8], b: &[u8]) -> std::cmp::Ordering {
    match a.len().cmp(&b.len()) {
        std::cmp::Ordering::Equal => {
            for i in (0..a.len()).rev() {
                match a[i].cmp(&b[i]) {
                    std::cmp::Ordering::Equal => {}
                    other => return other,
                }
            }
            std::cmp::Ordering::Equal
        }
        other => other,
    }
}

fn add_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(a.len().max(b.len()) + 1);
    let mut carry = 0u16;
    for i in 0..a.len().max(b.len()) {
        let sum = u16::from(*a.get(i).unwrap_or(&0))
            + u16::from(*b.get(i).unwrap_or(&0))
            + carry;
        result.push((sum % 10) as u8);
        carry = sum / 10;
    }
    if carry > 0 {
        result.push(carry as u8);
    }
    result
}

fn sub_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(a.len());
    let mut borrow = 0i16;
    for i in 0..a.len() {
        let mut digit = i16::from(a[i]) - i16::from(*b.get(i).unwrap_or(&0)) - borrow;
        if digit < 0 {
            digit += 10;
            borrow = 1;
        } else {
            borrow = 0;
        }
        result.push(digit as u8);
    }
    trim_magnitude(&mut result);
    result
}