#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Empty,
    SignOnly,
    InvalidDigit { index: usize },
    Separator { index: usize },
    LeadingZero { index: usize },
    TooManyDigits { limit: usize, index: usize },
    Underflow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    // Base 10^9, least-significant limb first.
    limbs: Vec<u32>,
}

impl Big {
    pub fn parse(text: &str, max_digits: usize) -> Result<Self, Error> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(Error::Empty);
        }

        let start = if bytes[0] == b'+' { 1 } else { 0 };
        if start == bytes.len() {
            return Err(Error::SignOnly);
        }

        // First scan all magnitude bytes for invalid characters. Underscores
        // are intentionally ignored here so invalid digits take precedence.
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' && !byte.is_ascii_digit() {
                return Err(Error::InvalidDigit { index });
            }
        }

        // Then validate that each underscore has a digit on either side.
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
        let mut first_excess = None;
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte.is_ascii_digit() {
                digit_count += 1;
                first_digit.get_or_insert((byte, index));
                if digit_count == max_digits.saturating_add(1) {
                    first_excess = Some(index);
                }
            }
        }

        let (first_byte, first_index) = first_digit.expect("nonempty magnitude");
        if digit_count > 1 && first_byte == b'0' {
            return Err(Error::LeadingZero { index: first_index });
        }

        if digit_count > max_digits {
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: first_excess.unwrap_or_else(|| {
                    bytes
                        .iter()
                        .enumerate()
                        .skip(start)
                        .filter(|(_, byte)| byte.is_ascii_digit())
                        .nth(max_digits)
                        .map(|(index, _)| index)
                        .unwrap_or(first_index)
                }),
            });
        }

        let mut limbs = Vec::new();
        let mut limb = 0u32;
        let mut limb_digits = 0u32;

        for &byte in bytes.iter().skip(start) {
            if byte.is_ascii_digit() {
                limb = limb * 10 + u32::from(byte - b'0');
                limb_digits += 1;
                if limb_digits == 9 {
                    limbs.push(limb);
                    limb = 0;
                    limb_digits = 0;
                }
            }
        }
        if limb_digits != 0 {
            limbs.push(limb);
        }

        let mut result = Self { limbs };
        result.normalize();
        Ok(result)
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let mut digits = self.to_decimal_digits();
        let target = min_digits.max(1);
        if digits.len() < target {
            let mut padded = String::with_capacity(target);
            padded.extend(std::iter::repeat('0').take(target - digits.len()));
            padded.push_str(&digits);
            digits = padded;
        }

        if group == 0 || digits.len() <= group {
            return digits;
        }

        let first_group = {
            let rem = digits.len() % group;
            if rem == 0 { group } else { rem }
        };
        let separators = (digits.len() - 1) / group;
        let mut output = String::with_capacity(digits.len() + separators);
        output.push_str(&digits[..first_group]);

        let mut position = first_group;
        while position < digits.len() {
            output.push('_');
            output.push_str(&digits[position..position + group]);
            position += group;
        }
        output
    }

    pub fn cmp_value(&self, rhs: &Self) -> std::cmp::Ordering {
        match self.limbs.len().cmp(&rhs.limbs.len()) {
            std::cmp::Ordering::Equal => {
                for (left, right) in self.limbs.iter().rev().zip(rhs.limbs.iter().rev()) {
                    match left.cmp(right) {
                        std::cmp::Ordering::Equal => {}
                        ordering => return ordering,
                    }
                }
                std::cmp::Ordering::Equal
            }
            ordering => ordering,
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        const BASE: u64 = 1_000_000_000;
        let len = self.limbs.len().max(rhs.limbs.len());
        let mut limbs = Vec::with_capacity(len + 1);
        let mut carry = 0u64;

        for index in 0..len {
            let left = u64::from(*self.limbs.get(index).unwrap_or(&0));
            let right = u64::from(*rhs.limbs.get(index).unwrap_or(&0));
            let sum = left + right + carry;
            limbs.push((sum % BASE) as u32);
            carry = sum / BASE;
        }
        if carry != 0 {
            limbs.push(carry as u32);
        }

        let mut result = Self { limbs };
        result.normalize();
        result
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, Error> {
        if self.cmp_value(rhs) == std::cmp::Ordering::Less {
            return Err(Error::Underflow);
        }

        const BASE: i64 = 1_000_000_000;
        let mut limbs = Vec::with_capacity(self.limbs.len());
        let mut borrow = 0i64;

        for index in 0..self.limbs.len() {
            let left = i64::from(self.limbs[index]) - borrow;
            let right = i64::from(*rhs.limbs.get(index).unwrap_or(&0));
            if left < right {
                limbs.push((left + BASE - right) as u32);
                borrow = 1;
            } else {
                limbs.push((left - right) as u32);
                borrow = 0;
            }
        }

        let mut result = Self { limbs };
        result.normalize();
        Ok(result)
    }

    fn normalize(&mut self) {
        while self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }
    }

    fn to_decimal_digits(&self) -> String {
        if self.limbs.is_empty() {
            return String::from("0");
        }

        let mut output = self.limbs.last().unwrap().to_string();
        for limb in self.limbs[..self.limbs.len() - 1].iter().rev() {
            use std::fmt::Write;
            write!(&mut output, "{limb:09}").expect("writing to String");
        }
        output
    }
}

fn main() {}