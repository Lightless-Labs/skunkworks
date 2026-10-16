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
    // Little-endian base-10 digits. Zero is represented by [0] with negative=false.
    digits: Vec<u8>,
}

impl Big {
    pub fn parse(text: &str, max_digits: usize) -> Result<Self, Error> {
        if text.is_empty() {
            return Err(Error::Empty);
        }

        let bytes = text.as_bytes();
        let (negative, start) = match bytes[0] {
            b'-' => (true, 1),
            b'+' => (false, 1),
            _ => (false, 0),
        };
        if start == bytes.len() {
            return Err(Error::SignOnly);
        }

        let magnitude = &bytes[start..];

        if let Some(offset) = magnitude
            .iter()
            .position(|&b| b != b'_' && !b.is_ascii_digit())
        {
            return Err(Error::InvalidDigit {
                index: start + offset,
            });
        }

        let digit_count = magnitude.iter().filter(|&&b| b != b'_').count();
        let mut digit_number = 0usize;
        for (i, &b) in magnitude.iter().enumerate() {
            if b == b'_' {
                if i == 0
                    || i + 1 == magnitude.len()
                    || !magnitude[i - 1].is_ascii_digit()
                    || !magnitude[i + 1].is_ascii_digit()
                {
                    return Err(Error::Separator { index: start + i });
                }
            } else {
                digit_number += 1;
            }
        }

        if digit_count > 1 {
            let first_digit = magnitude.iter().position(|&b| b != b'_').unwrap();
            if magnitude[first_digit] == b'0' {
                return Err(Error::LeadingZero {
                    index: start + first_digit,
                });
            }
        }

        if digit_count > max_digits {
            let excess_number = max_digits + 1;
            let mut seen = 0usize;
            for (i, &b) in magnitude.iter().enumerate() {
                if b != b'_' {
                    seen += 1;
                    if seen == excess_number {
                        return Err(Error::TooManyDigits {
                            limit: max_digits,
                            index: start + i,
                        });
                    }
                }
            }
        }

        let mut digits = Vec::with_capacity(digit_count);
        for &b in magnitude.iter().rev() {
            if b != b'_' {
                digits.push(b - b'0');
            }
        }
        let mut value = Self { negative, digits };
        value.normalize();
        Ok(value)
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let actual_len = self.digits.len().max(1);
        let width = actual_len.max(min_digits);
        let padding = width - actual_len;

        let mut out = String::new();
        if self.negative {
            out.push('-');
        }

        for pos in (0..width).rev() {
            if group > 0 && pos < width - 1 && (pos + 1) % group == 0 {
                out.push('_');
            }

            if pos >= actual_len {
                out.push('0');
            } else {
                out.push((b'0' + self.digits[pos]) as char);
            }
        }
        let _ = padding;
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
            let mut result = Self {
                negative: self.negative,
                digits: add_magnitudes(&self.digits, &rhs.digits),
            };
            result.normalize();
            result
        } else {
            match cmp_magnitude(&self.digits, &rhs.digits) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => {
                    let mut result = Self {
                        negative: self.negative,
                        digits: sub_magnitudes(&self.digits, &rhs.digits),
                    };
                    result.normalize();
                    result
                }
                Ordering::Less => {
                    let mut result = Self {
                        negative: rhs.negative,
                        digits: sub_magnitudes(&rhs.digits, &self.digits),
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
        let mut result = Self {
            negative: self.negative ^ rhs.negative,
            digits: mul_magnitudes(&self.digits, &rhs.digits),
        };
        result.normalize();
        result
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }

        let (q_digits, r_digits) = div_rem_magnitudes(&self.digits, &rhs.digits);
        let mut quotient = Self {
            negative: self.negative ^ rhs.negative,
            digits: q_digits,
        };
        let mut remainder = Self {
            negative: self.negative,
            digits: r_digits,
        };
        quotient.normalize();
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
            for i in (0..a.len()).rev() {
                match a[i].cmp(&b[i]) {
                    Ordering::Equal => {}
                    ord => return ord,
                }
            }
            Ordering::Equal
        }
        ord => ord,
    }
}

fn add_magnitudes(a: &[u8], b: &[u8]) -> Vec<u8> {
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

fn sub_magnitudes(a: &[u8], b: &[u8]) -> Vec<u8> {
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
    trim_magnitude(&mut result);
    result
}

fn mul_magnitudes(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut result = vec![0u8; a.len() + b.len() + 1];
    for (i, &ad) in a.iter().enumerate() {
        let mut carry = 0u16;
        for (j, &bd) in b.iter().enumerate() {
            let index = i + j;
            let value = result[index] as u16 + ad as u16 * bd as u16 + carry;
            result[index] = (value % 10) as u8;
            carry = value / 10;
        }
        let mut index = i + b.len();
        while carry != 0 {
            let value = result[index] as u16 + carry;
            result[index] = (value % 10) as u8;
            carry = value / 10;
            index += 1;
        }
    }
    trim_magnitude(&mut result);
    result
}

fn div_rem_magnitudes(dividend: &[u8], divisor: &[u8]) -> (Vec<u8>, Vec<u8>) {
    if cmp_magnitude(dividend, divisor) == Ordering::Less {
        return (vec![0], dividend.to_vec());
    }

    let mut quotient_be = Vec::with_capacity(dividend.len());
    let mut remainder = vec![0u8];

    for &digit in dividend.iter().rev() {
        if remainder.len() == 1 && remainder[0] == 0 {
            remainder[0] = digit;
        } else {
            remainder.insert(0, digit);
        }
        trim_magnitude(&mut remainder);

        let mut qdigit = 0u8;
        while cmp_magnitude(&remainder, divisor) != Ordering::Less {
            remainder = sub_magnitudes(&remainder, divisor);
            qdigit += 1;
        }
        quotient_be.push(qdigit);
    }

    quotient_be.reverse();
    trim_magnitude(&mut quotient_be);
    trim_magnitude(&mut remainder);
    (quotient_be, remainder)
}

fn trim_magnitude(digits: &mut Vec<u8>) {
    while digits.len() > 1 && digits.last() == Some(&0) {
        digits.pop();
    }
    if digits.is_empty() {
        digits.push(0);
    }
}