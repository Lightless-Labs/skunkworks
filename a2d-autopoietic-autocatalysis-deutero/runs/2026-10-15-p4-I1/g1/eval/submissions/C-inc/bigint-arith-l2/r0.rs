use std::cmp::Ordering;
use std::fmt;

#[derive(Clone)]
pub struct Big {
    negative: bool,
    // Little-endian base-10 digits. Zero is represented as [0] with no sign.
    digits: Vec<u8>,
}

impl fmt::Debug for Big {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Big")
            .field("negative", &self.negative)
            .field("digits", &self.digits)
            .finish()
    }
}

impl PartialEq for Big {
    fn eq(&self, other: &Self) -> bool {
        self.negative == other.negative && self.digits == other.digits
    }
}

impl Eq for Big {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Empty,
    SignOnly,
    InvalidDigit { index: usize },
    Separator { index: usize },
    LeadingZero { index: usize },
    TooManyDigits { limit: usize, index: usize },
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

        // Invalid bytes take precedence over separator, leading-zero, and size
        // errors. Underscores are the sole bytes ignored by this scan.
        if let Some(index) = (start..bytes.len())
            .find(|&i| bytes[i] != b'_' && !bytes[i].is_ascii_digit())
        {
            return Err(Error::InvalidDigit { index });
        }

        // Check separators after the full invalid-byte scan.
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

        let digit_count = bytes[start..]
            .iter()
            .filter(|&&b| b != b'_')
            .count();
        let first_digit_index = (start..bytes.len())
            .find(|&i| bytes[i].is_ascii_digit())
            .unwrap();

        if digit_count > 1 && bytes[first_digit_index] == b'0' {
            return Err(Error::LeadingZero {
                index: first_digit_index,
            });
        }

        if digit_count > max_digits {
            let excess_number = max_digits;
            let excess_index = bytes[start..]
                .iter()
                .enumerate()
                .filter(|&(_, &b)| b != b'_')
                .nth(excess_number)
                .map(|(offset, _)| {
                    // Recover the original byte index, including separators.
                    let mut seen = 0usize;
                    let mut result = start;
                    for i in start..bytes.len() {
                        if bytes[i] != b'_' {
                            if seen == excess_number {
                                result = i;
                                break;
                            }
                            seen += 1;
                        }
                    }
                    let _ = offset;
                    result
                })
                .unwrap();
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: excess_index,
            });
        }

        let mut digits = Vec::with_capacity(digit_count);
        for &b in bytes[start..].iter().rev() {
            if b != b'_' {
                digits.push(b - b'0');
            }
        }
        if digits.is_empty() {
            digits.push(0);
        }

        let mut result = Self { negative, digits };
        result.normalize();
        Ok(result)
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let actual = self.digits.len().max(1);
        let width = actual.max(min_digits);
        let pad = width - actual;
        let mut out = String::with_capacity(width + width / 2 + usize::from(self.negative));
        if self.negative {
            out.push('-');
        }

        for i in 0..width {
            if group > 0 && i > 0 && (width - i) % group == 0 {
                out.push('_');
            }
            if i < pad {
                out.push('0');
            } else {
                out.push((b'0' + self.digits[width - 1 - i]) as char);
            }
        }
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
                        digits: subtract_magnitudes(&self.digits, &rhs.digits),
                    };
                    result.normalize();
                    result
                }
                Ordering::Less => {
                    let mut result = Self {
                        negative: rhs.negative,
                        digits: subtract_magnitudes(&rhs.digits, &self.digits),
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

        let mut digits: Vec<u8> = accum.into_iter().map(|n| n as u8).collect();
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
            for (x, y) in a.iter().rev().zip(b.iter().rev()) {
                match x.cmp(y) {
                    Ordering::Equal => {}
                    other => return other,
                }
            }
            Ordering::Equal
        }
        other => other,
    }
}

fn add_magnitudes(a: &[u8], b: &[u8]) -> Vec<u8> {
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

// Assumes a >= b in magnitude.
fn subtract_magnitudes(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(a.len());
    let mut borrow = 0i8;
    for i in 0..a.len() {
        let mut digit =
            i16::from(a[i]) - i16::from(b.get(i).copied().unwrap_or(0)) - i16::from(borrow);
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

fn main() {}