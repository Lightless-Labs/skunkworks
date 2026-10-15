use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    negative: bool,
    // Base 1,000,000,000, least-significant limb first.
    limbs: Vec<u32>,
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

        let mut start = 0;
        let mut negative = false;
        if bytes[0] == b'+' || bytes[0] == b'-' {
            negative = bytes[0] == b'-';
            start = 1;
            if start == bytes.len() {
                return Err(Error::SignOnly);
            }
        }

        // Scan every magnitude byte before checking separators.
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' && !byte.is_ascii_digit() {
                return Err(Error::InvalidDigit { index });
            }
        }

        let mut first_digit = None;
        let mut last_digit = None;
        let mut digit_count = 0usize;
        let mut first_bad_separator = None;

        for index in start..bytes.len() {
            match bytes[index] {
                b'0'..=b'9' => {
                    first_digit.get_or_insert(index);
                    last_digit = Some(index);
                    digit_count += 1;
                }
                b'_' => {
                    let prev_is_digit =
                        index > start && bytes[index - 1].is_ascii_digit();
                    let next_is_digit =
                        index + 1 < bytes.len() && bytes[index + 1].is_ascii_digit();
                    if !(prev_is_digit && next_is_digit) {
                        first_bad_separator.get_or_insert(index);
                    }
                }
                _ => unreachable!(),
            }
        }

        if let Some(index) = first_bad_separator {
            return Err(Error::Separator { index });
        }

        let first_digit = first_digit.expect("nonempty magnitude has a digit");
        if digit_count > 1 && bytes[first_digit] == b'0' {
            return Err(Error::LeadingZero { index: first_digit });
        }

        if digit_count > max_digits {
            let mut seen = 0usize;
            for (index, &byte) in bytes.iter().enumerate().skip(start) {
                if byte.is_ascii_digit() {
                    seen += 1;
                    if seen > max_digits {
                        return Err(Error::TooManyDigits {
                            limit: max_digits,
                            index,
                        });
                    }
                }
            }
        }

        let _last_digit = last_digit.expect("nonempty magnitude has a digit");
        let mut limbs = Vec::new();
        let mut chunk_value = 0u32;
        let mut chunk_len = 0u32;

        for &byte in bytes[start..].iter().rev() {
            if byte == b'_' {
                continue;
            }
            chunk_value += u32::from(byte - b'0') * 10u32.pow(chunk_len);
            chunk_len += 1;
            if chunk_len == 9 {
                limbs.push(chunk_value);
                chunk_value = 0;
                chunk_len = 0;
            }
        }
        if chunk_len > 0 {
            limbs.push(chunk_value);
        }

        let mut result = Self { negative, limbs };
        result.normalize();
        Ok(result)
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let mut magnitude = self.magnitude_string();
        let required = min_digits.max(1);
        if magnitude.len() < required {
            let mut padded = String::with_capacity(required);
            padded.extend(std::iter::repeat_n('0', required - magnitude.len()));
            padded.push_str(&magnitude);
            magnitude = padded;
        }

        let mut output = String::new();
        if self.negative {
            output.push('-');
        }

        if group == 0 {
            output.push_str(&magnitude);
        } else {
            for (index, ch) in magnitude.chars().enumerate() {
                if index > 0 && (magnitude.len() - index) % group == 0 {
                    output.push('_');
                }
                output.push(ch);
            }
        }
        output
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.negative, rhs.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => self.cmp_magnitude(rhs),
            (true, true) => self.cmp_magnitude(rhs).reverse(),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            let mut result = Self {
                negative: self.negative,
                limbs: add_magnitudes(&self.limbs, &rhs.limbs),
            };
            result.normalize();
            result
        } else {
            match self.cmp_magnitude(rhs) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => {
                    let mut result = Self {
                        negative: self.negative,
                        limbs: subtract_magnitudes(&self.limbs, &rhs.limbs),
                    };
                    result.normalize();
                    result
                }
                Ordering::Less => {
                    let mut result = Self {
                        negative: rhs.negative,
                        limbs: subtract_magnitudes(&rhs.limbs, &self.limbs),
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

        let base = 1_000_000_000u64;
        let mut limbs = vec![0u32; self.limbs.len() + rhs.limbs.len()];

        for (i, &left) in self.limbs.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &right) in rhs.limbs.iter().enumerate() {
                let index = i + j;
                let current = u64::from(limbs[index])
                    + u64::from(left) * u64::from(right)
                    + carry;
                limbs[index] = (current % base) as u32;
                carry = current / base;
            }

            let mut index = i + rhs.limbs.len();
            while carry > 0 {
                let current = u64::from(limbs[index]) + carry;
                limbs[index] = (current % base) as u32;
                carry = current / base;
                index += 1;
            }
        }

        let mut result = Self {
            negative: self.negative != rhs.negative,
            limbs,
        };
        result.normalize();
        result
    }

    fn zero() -> Self {
        Self {
            negative: false,
            limbs: vec![0],
        }
    }

    fn is_zero(&self) -> bool {
        self.limbs.len() == 1 && self.limbs[0] == 0
    }

    fn normalize(&mut self) {
        while self.limbs.len() > 1 && self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }
        if self.is_zero() {
            self.negative = false;
        }
    }

    fn cmp_magnitude(&self, rhs: &Self) -> Ordering {
        match self.limbs.len().cmp(&rhs.limbs.len()) {
            Ordering::Equal => {
                for (left, right) in self.limbs.iter().rev().zip(rhs.limbs.iter().rev()) {
                    match left.cmp(right) {
                        Ordering::Equal => {}
                        ordering => return ordering,
                    }
                }
                Ordering::Equal
            }
            ordering => ordering,
        }
    }

    fn magnitude_string(&self) -> String {
        let mut iter = self.limbs.iter().rev();
        let mut result = iter.next().copied().unwrap_or(0).to_string();
        for limb in iter {
            result.push_str(&format!("{limb:09}"));
        }
        result
    }
}

fn add_magnitudes(left: &[u32], right: &[u32]) -> Vec<u32> {
    const BASE: u64 = 1_000_000_000;
    let mut result = Vec::with_capacity(left.len().max(right.len()) + 1);
    let mut carry = 0u64;

    for index in 0..left.len().max(right.len()) {
        let sum = u64::from(*left.get(index).unwrap_or(&0))
            + u64::from(*right.get(index).unwrap_or(&0))
            + carry;
        result.push((sum % BASE) as u32);
        carry = sum / BASE;
    }

    if carry > 0 {
        result.push(carry as u32);
    }
    result
}

fn subtract_magnitudes(larger: &[u32], smaller: &[u32]) -> Vec<u32> {
    const BASE: i64 = 1_000_000_000;
    let mut result = Vec::with_capacity(larger.len());
    let mut borrow = 0i64;

    for (index, &limb) in larger.iter().enumerate() {
        let mut difference = i64::from(limb)
            - i64::from(*smaller.get(index).unwrap_or(&0))
            - borrow;
        if difference < 0 {
            difference += BASE;
            borrow = 1;
        } else {
            borrow = 0;
        }
        result.push(difference as u32);
    }

    while result.len() > 1 && result.last() == Some(&0) {
        result.pop();
    }
    result
}

fn main() {}