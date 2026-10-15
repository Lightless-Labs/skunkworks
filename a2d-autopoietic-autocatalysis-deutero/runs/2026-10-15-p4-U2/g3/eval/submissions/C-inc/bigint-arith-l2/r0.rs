use std::cmp::Ordering;
use std::fmt;

#[derive(Clone, PartialEq, Eq)]
pub struct Big {
    negative: bool,
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

        let mut digit_count = 0usize;
        let mut first_digit_index = None;

        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte == b'_' {
                continue;
            }
            if !byte.is_ascii_digit() {
                return Err(Error::InvalidDigit { index });
            }
            if first_digit_index.is_none() {
                first_digit_index = Some(index);
            }
            digit_count += 1;
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

        let first = first_digit_index.expect("nonempty magnitude has a digit");
        if digit_count > 1 && bytes[first] == b'0' {
            return Err(Error::LeadingZero { index: first });
        }

        if digit_count > max_digits {
            let target = max_digits + 1;
            let mut seen = 0usize;
            for (index, &byte) in bytes.iter().enumerate().skip(start) {
                if byte.is_ascii_digit() {
                    seen += 1;
                    if seen == target {
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

        let is_zero = digits.iter().all(|&digit| digit == 0);
        Ok(Self {
            negative: negative && !is_zero,
            digits,
        })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let magnitude_len = self.digits.len().max(1);
        let total_len = magnitude_len.max(min_digits);
        let padding = total_len - magnitude_len;

        let mut out = String::with_capacity(total_len + total_len / group.max(1) + 1);
        if self.negative {
            out.push('-');
        }

        for position in 0..total_len {
            if group > 0 && position > 0 && (total_len - position) % group == 0 {
                out.push('_');
            }

            if position < padding {
                out.push('0');
            } else {
                let digit_index = total_len - 1 - position;
                let digit = self.digits.get(digit_index).copied().unwrap_or(0);
                out.push(char::from(b'0' + digit));
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
            Self::from_magnitude(
                self.negative,
                add_magnitudes(&self.digits, &rhs.digits),
            )
        } else {
            match cmp_magnitude(&self.digits, &rhs.digits) {
                Ordering::Greater => Self::from_magnitude(
                    self.negative,
                    sub_magnitudes(&self.digits, &rhs.digits),
                ),
                Ordering::Less => Self::from_magnitude(
                    rhs.negative,
                    sub_magnitudes(&rhs.digits, &self.digits),
                ),
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

        let mut product = vec![0u8; self.digits.len() + rhs.digits.len()];
        for (i, &left) in self.digits.iter().enumerate() {
            let mut carry = 0u16;
            for (j, &right) in rhs.digits.iter().enumerate() {
                let index = i + j;
                let value = product[index] as u16 + left as u16 * right as u16 + carry;
                product[index] = (value % 10) as u8;
                carry = value / 10;
            }

            let mut index = i + rhs.digits.len();
            while carry > 0 {
                if index == product.len() {
                    product.push(0);
                }
                let value = product[index] as u16 + carry;
                product[index] = (value % 10) as u8;
                carry = value / 10;
                index += 1;
            }
        }

        Self::from_magnitude(self.negative != rhs.negative, product)
    }

    fn from_magnitude(negative: bool, mut digits: Vec<u8>) -> Self {
        while digits.len() > 1 && digits.last() == Some(&0) {
            digits.pop();
        }
        let is_zero = digits.iter().all(|&digit| digit == 0);
        Self {
            negative: negative && !is_zero,
            digits,
        }
    }

    fn zero() -> Self {
        Self {
            negative: false,
            digits: vec![0],
        }
    }

    fn is_zero(&self) -> bool {
        self.digits.iter().all(|&digit| digit == 0)
    }
}

fn cmp_magnitude(left: &[u8], right: &[u8]) -> Ordering {
    let left_len = significant_len(left);
    let right_len = significant_len(right);

    match left_len.cmp(&right_len) {
        Ordering::Equal => {
            for index in (0..left_len).rev() {
                match left[index].cmp(&right[index]) {
                    Ordering::Equal => {}
                    ordering => return ordering,
                }
            }
            Ordering::Equal
        }
        ordering => ordering,
    }
}

fn significant_len(digits: &[u8]) -> usize {
    let mut len = digits.len();
    while len > 1 && digits[len - 1] == 0 {
        len -= 1;
    }
    len
}

fn add_magnitudes(left: &[u8], right: &[u8]) -> Vec<u8> {
    let max_len = left.len().max(right.len());
    let mut result = Vec::with_capacity(max_len + 1);
    let mut carry = 0u16;

    for index in 0..max_len {
        let a = left.get(index).copied().unwrap_or(0) as u16;
        let b = right.get(index).copied().unwrap_or(0) as u16;
        let sum = a + b + carry;
        result.push((sum % 10) as u8);
        carry = sum / 10;
    }

    if carry > 0 {
        result.push(carry as u8);
    }

    result
}

fn sub_magnitudes(larger: &[u8], smaller: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(larger.len());
    let mut borrow = 0i16;

    for index in 0..larger.len() {
        let a = larger[index] as i16 - borrow;
        let b = smaller.get(index).copied().unwrap_or(0) as i16;

        if a < b {
            result.push((a + 10 - b) as u8);
            borrow = 1;
        } else {
            result.push((a - b) as u8);
            borrow = 0;
        }
    }

    while result.len() > 1 && result.last() == Some(&0) {
        result.pop();
    }

    result
}

fn main() {}