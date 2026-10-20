use std::cmp::Ordering;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Empty,
    SignOnly,
    InvalidDigit { index: usize },
    Separator { index: usize },
    LeadingZero { index: usize },
    TooManyDigits { limit: usize, index: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    negative: bool,
    // Little-endian decimal digits. Zero is represented as [0] with negative=false.
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

        // Invalid bytes take precedence over separator and digit-count errors.
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' && !byte.is_ascii_digit() {
                return Err(Error::InvalidDigit { index });
            }
        }

        let magnitude = &bytes[start..];
        for i in 0..magnitude.len() {
            if magnitude[i] == b'_'
                && (i == 0
                    || i + 1 == magnitude.len()
                    || magnitude[i - 1] == b'_'
                    || magnitude[i + 1] == b'_')
            {
                return Err(Error::Separator { index: start + i });
            }
        }

        let digit_count = magnitude.iter().filter(|&&b| b != b'_').count();
        let first_digit_index = magnitude
            .iter()
            .position(|&b| b != b'_')
            .expect("nonempty magnitude contains digits");

        if digit_count > 1 && magnitude[first_digit_index] == b'0' {
            return Err(Error::LeadingZero {
                index: start + first_digit_index,
            });
        }

        if digit_count > max_digits {
            let excess_digit = magnitude
                .iter()
                .enumerate()
                .filter(|(_, b)| **b != b'_')
                .nth(max_digits)
                .map(|(index, _)| index)
                .expect("digit count exceeds limit");
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: start + excess_digit,
            });
        }

        let mut digits = Vec::with_capacity(digit_count);
        for &byte in magnitude.iter().rev() {
            if byte != b'_' {
                digits.push(byte - b'0');
            }
        }

        let is_zero = digits.iter().all(|&digit| digit == 0);
        Ok(Self {
            negative: negative && !is_zero,
            digits,
        })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let digit_count = self.digits.len().max(min_digits).max(1);
        let padding = digit_count - self.digits.len();
        let mut out = String::with_capacity(digit_count + digit_count / 3 + 1);

        if self.negative {
            out.push('-');
        }

        for position in 0..digit_count {
            if group > 0 && position > 0 && (digit_count - position) % group == 0 {
                out.push('_');
            }

            let digit = if position < padding {
                0
            } else {
                self.digits[self.digits.len() - 1 - (position - padding)]
            };
            out.push((b'0' + digit) as char);
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
            let digits = add_magnitude(&self.digits, &rhs.digits);
            Self::from_parts(self.negative, digits)
        } else {
            match cmp_magnitude(&self.digits, &rhs.digits) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => Self::from_parts(
                    self.negative,
                    subtract_magnitude(&self.digits, &rhs.digits),
                ),
                Ordering::Less => Self::from_parts(
                    rhs.negative,
                    subtract_magnitude(&rhs.digits, &self.digits),
                ),
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
        for (i, &left) in self.digits.iter().enumerate() {
            let mut carry = 0u16;
            for (j, &right) in rhs.digits.iter().enumerate() {
                let index = i + j;
                let value = result[index] as u16 + left as u16 * right as u16 + carry;
                result[index] = (value % 10) as u8;
                carry = value / 10;
            }

            let mut index = i + rhs.digits.len();
            while carry != 0 {
                if index == result.len() {
                    result.push(0);
                }
                let value = result[index] as u16 + carry;
                result[index] = (value % 10) as u8;
                carry = value / 10;
                index += 1;
            }
        }

        Self::from_parts(self.negative != rhs.negative, result)
    }

    fn from_parts(negative: bool, digits: Vec<u8>) -> Self {
        let mut result = Self { negative, digits };
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

fn cmp_magnitude(left: &[u8], right: &[u8]) -> Ordering {
    match left.len().cmp(&right.len()) {
        Ordering::Equal => left.iter().rev().cmp(right.iter().rev()),
        ordering => ordering,
    }
}

fn add_magnitude(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(left.len().max(right.len()) + 1);
    let mut carry = 0u8;

    for i in 0..left.len().max(right.len()) {
        let sum = left.get(i).copied().unwrap_or(0)
            + right.get(i).copied().unwrap_or(0)
            + carry;
        result.push(sum % 10);
        carry = sum / 10;
    }

    if carry != 0 {
        result.push(carry);
    }
    result
}

// Requires left magnitude >= right magnitude.
fn subtract_magnitude(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(left.len());
    let mut borrow = 0i8;

    for i in 0..left.len() {
        let mut digit = left[i] as i8 - borrow - right.get(i).copied().unwrap_or(0) as i8;
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

impl fmt::Display for Big {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format(0, 0))
    }
}