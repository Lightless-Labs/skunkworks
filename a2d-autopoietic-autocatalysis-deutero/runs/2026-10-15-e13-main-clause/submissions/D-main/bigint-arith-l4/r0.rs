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
    InvalidRadix { radix: u32 },
    ExponentTooLarge { exponent: u32, limit: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    negative: bool,
    // Little-endian base-1,000,000,000 limbs.
    limbs: Vec<u32>,
}

impl Big {
    const BASE: u64 = 1_000_000_000;

    fn zero() -> Self {
        Self {
            negative: false,
            limbs: Vec::new(),
        }
    }

    fn one() -> Self {
        Self {
            negative: false,
            limbs: vec![1],
        }
    }

    fn is_zero(&self) -> bool {
        self.limbs.is_empty()
    }

    fn normalize(&mut self) {
        while self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }
        if self.is_zero() {
            self.negative = false;
        }
    }

    fn abs(&self) -> Self {
        let mut result = self.clone();
        result.negative = false;
        result
    }

    fn cmp_abs(&self, rhs: &Self) -> Ordering {
        match self.limbs.len().cmp(&rhs.limbs.len()) {
            Ordering::Equal => {
                for (a, b) in self.limbs.iter().rev().zip(rhs.limbs.iter().rev()) {
                    match a.cmp(b) {
                        Ordering::Equal => {}
                        other => return other,
                    }
                }
                Ordering::Equal
            }
            other => other,
        }
    }

    fn add_abs(&self, rhs: &Self) -> Self {
        let n = self.limbs.len().max(rhs.limbs.len());
        let mut out = Vec::with_capacity(n + 1);
        let mut carry = 0u64;

        for i in 0..n {
            let a = self.limbs.get(i).copied().unwrap_or(0) as u64;
            let b = rhs.limbs.get(i).copied().unwrap_or(0) as u64;
            let sum = a + b + carry;
            out.push((sum % Self::BASE) as u32);
            carry = sum / Self::BASE;
        }
        if carry != 0 {
            out.push(carry as u32);
        }
        Self {
            negative: false,
            limbs: out,
        }
    }

    // Requires |self| >= |rhs|.
    fn sub_abs(&self, rhs: &Self) -> Self {
        let mut out = Vec::with_capacity(self.limbs.len());
        let mut borrow = 0i64;

        for i in 0..self.limbs.len() {
            let a = self.limbs[i] as i64;
            let b = rhs.limbs.get(i).copied().unwrap_or(0) as i64;
            let mut digit = a - b - borrow;
            if digit < 0 {
                digit += Self::BASE as i64;
                borrow = 1;
            } else {
                borrow = 0;
            }
            out.push(digit as u32);
        }
        let mut result = Self {
            negative: false,
            limbs: out,
        };
        result.normalize();
        result
    }

    fn mul_small(&mut self, multiplier: u32) {
        if multiplier == 0 || self.is_zero() {
            self.limbs.clear();
            self.negative = false;
            return;
        }
        let mut carry = 0u64;
        for digit in &mut self.limbs {
            let value = *digit as u64 * multiplier as u64 + carry;
            *digit = (value % Self::BASE) as u32;
            carry = value / Self::BASE;
        }
        while carry != 0 {
            self.limbs.push((carry % Self::BASE) as u32);
            carry /= Self::BASE;
        }
    }

    fn add_small(&mut self, addend: u32) {
        let mut carry = addend as u64;
        let mut i = 0;
        while carry != 0 {
            if i == self.limbs.len() {
                self.limbs.push(0);
            }
            let value = self.limbs[i] as u64 + carry;
            self.limbs[i] = (value % Self::BASE) as u32;
            carry = value / Self::BASE;
            i += 1;
        }
    }

    fn div_rem_small(&self, divisor: u32) -> (Self, u32) {
        let mut quotient = vec![0; self.limbs.len()];
        let mut remainder = 0u64;
        for i in (0..self.limbs.len()).rev() {
            let value = remainder * Self::BASE + self.limbs[i] as u64;
            quotient[i] = (value / divisor as u64) as u32;
            remainder = value % divisor as u64;
        }
        let mut result = Self {
            negative: false,
            limbs: quotient,
        };
        result.normalize();
        (result, remainder as u32)
    }

    fn mul_abs(&self, rhs: &Self) -> Self {
        if self.is_zero() || rhs.is_zero() {
            return Self::zero();
        }

        let mut out = vec![0u32; self.limbs.len() + rhs.limbs.len()];
        for (i, &a) in self.limbs.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &b) in rhs.limbs.iter().enumerate() {
                let k = i + j;
                let value = out[k] as u64 + a as u64 * b as u64 + carry;
                out[k] = (value % Self::BASE) as u32;
                carry = value / Self::BASE;
            }
            let mut k = i + rhs.limbs.len();
            while carry != 0 {
                if k == out.len() {
                    out.push(0);
                }
                let value = out[k] as u64 + carry;
                out[k] = (value % Self::BASE) as u32;
                carry = value / Self::BASE;
                k += 1;
            }
        }

        let mut result = Self {
            negative: false,
            limbs: out,
        };
        result.normalize();
        result
    }

    fn mul_small_value(&self, multiplier: u32) -> Self {
        let mut result = self.clone();
        result.negative = false;
        result.mul_small(multiplier);
        result
    }

    fn div_rem_abs(&self, divisor: &Self) -> (Self, Self) {
        debug_assert!(!divisor.is_zero());

        if self.cmp_abs(divisor) == Ordering::Less {
            return (Self::zero(), self.abs());
        }

        let mut quotient_be = Vec::with_capacity(self.limbs.len());
        let mut remainder = Self::zero();

        for &digit in self.limbs.iter().rev() {
            if !remainder.is_zero() {
                remainder.limbs.insert(0, digit);
            } else if digit != 0 {
                remainder.limbs.push(digit);
            }
            remainder.normalize();

            let mut low = 0u32;
            let mut high = (Self::BASE - 1) as u32;
            while low < high {
                let mid = low + ((high - low) / 2) + 1;
                let product = divisor.mul_small_value(mid);
                if product.cmp_abs(&remainder) != Ordering::Greater {
                    low = mid;
                } else {
                    high = mid - 1;
                }
            }
            quotient_be.push(low);
            if low != 0 {
                let product = divisor.mul_small_value(low);
                remainder = remainder.sub_abs(&product);
            }
        }

        quotient_be.reverse();
        let mut quotient = Self {
            negative: false,
            limbs: quotient_be,
        };
        quotient.normalize();
        (quotient, remainder)
    }

    pub fn parse(text: &str, radix: u32, max_digits: usize) -> Result<Self, Error> {
        if !(2..=36).contains(&radix) {
            return Err(Error::InvalidRadix { radix });
        }

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
            if byte == b'_' {
                continue;
            }
            let valid = match byte {
                b'0'..=b'9' => (byte - b'0') as u32,
                b'a'..=b'z' => (byte - b'a' + 10) as u32,
                b'A'..=b'Z' => (byte - b'A' + 10) as u32,
                _ => {
                    invalid.get_or_insert(index);
                    continue;
                }
            };
            if valid >= radix {
                invalid.get_or_insert(index);
            }
        }
        if let Some(index) = invalid {
            return Err(Error::InvalidDigit { index });
        }

        let mut separator_error = None;
        for index in start..bytes.len() {
            if bytes[index] == b'_' {
                let prev_valid = index > start && bytes[index - 1] != b'_';
                let next_valid = index + 1 < bytes.len() && bytes[index + 1] != b'_';
                if !prev_valid || !next_valid {
                    separator_error.get_or_insert(index);
                }
            }
        }
        if let Some(index) = separator_error {
            return Err(Error::Separator { index });
        }

        let digit_count = bytes[start..].iter().filter(|&&b| b != b'_').count();
        let first_digit_index = (start..bytes.len()).find(|&i| bytes[i] != b'_').unwrap();
        if digit_count > 1 && bytes[first_digit_index] == b'0' {
            return Err(Error::LeadingZero {
                index: first_digit_index,
            });
        }

        if digit_count > max_digits {
            let excess_number = max_digits;
            let mut seen = 0usize;
            let mut index = first_digit_index;
            for i in start..bytes.len() {
                if bytes[i] != b'_' {
                    if seen == excess_number {
                        index = i;
                        break;
                    }
                    seen += 1;
                }
            }
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index,
            });
        }

        let mut value = Self::zero();
        for &byte in &bytes[start..] {
            if byte == b'_' {
                continue;
            }
            let digit = match byte {
                b'0'..=b'9' => (byte - b'0') as u32,
                b'a'..=b'z' => (byte - b'a' + 10) as u32,
                b'A'..=b'Z' => (byte - b'A' + 10) as u32,
                _ => unreachable!(),
            };
            value.mul_small(radix);
            value.add_small(digit);
        }
        value.negative = negative && !value.is_zero();
        Ok(value)
    }

    pub fn format(&self, radix: u32, min_digits: usize, group: usize) -> Result<String, Error> {
        if !(2..=36).contains(&radix) {
            return Err(Error::InvalidRadix { radix });
        }

        let mut value = self.abs();
        let mut digits = Vec::new();
        if value.is_zero() {
            digits.push(0);
        } else {
            while !value.is_zero() {
                let (quotient, remainder) = value.div_rem_small(radix);
                digits.push(remainder);
                value = quotient;
            }
            digits.reverse();
        }

        if digits.len() < min_digits {
            let mut padded = vec![0; min_digits - digits.len()];
            padded.extend(digits);
            digits = padded;
        }

        let mut out = String::new();
        if self.negative && !self.is_zero() {
            out.push('-');
        }
        for (i, digit) in digits.iter().enumerate() {
            if group > 0 && i > 0 && (digits.len() - i) % group == 0 {
                out.push('_');
            }
            out.push(if *digit < 10 {
                (b'0' + *digit as u8) as char
            } else {
                (b'a' + (*digit as u8 - 10)) as char
            });
        }
        Ok(out)
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.signum(), rhs.signum()) {
            (a, b) if a < b => Ordering::Less,
            (a, b) if a > b => Ordering::Greater,
            (0, 0) => Ordering::Equal,
            (-1, -1) => self.cmp_abs(rhs).reverse(),
            _ => self.cmp_abs(rhs),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            let mut result = self.add_abs(rhs);
            result.negative = self.negative && !result.is_zero();
            result
        } else {
            match self.cmp_abs(rhs) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => {
                    let mut result = self.sub_abs(rhs);
                    result.negative = self.negative;
                    result
                }
                Ordering::Less => {
                    let mut result = rhs.sub_abs(self);
                    result.negative = rhs.negative;
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
        let mut result = self.mul_abs(rhs);
        result.negative = !result.is_zero() && (self.negative != rhs.negative);
        result
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }

        let (mut quotient, mut remainder) = self.abs().div_rem_abs(&rhs.abs());
        quotient.negative =
            !quotient.is_zero() && (self.negative != rhs.negative);
        remainder.negative = !remainder.is_zero() && self.negative;
        Ok((quotient, remainder))
    }

    pub fn pow(&self, exponent: u32) -> Result<Self, Error> {
        const LIMIT: u32 = 1024;
        if exponent > LIMIT {
            return Err(Error::ExponentTooLarge {
                exponent,
                limit: LIMIT,
            });
        }

        let mut exp = exponent;
        let mut base = self.clone();
        let mut result = Self::one();
        while exp != 0 {
            if exp & 1 != 0 {
                result = result.mul(&base);
            }
            exp >>= 1;
            if exp != 0 {
                base = base.mul(&base);
            }
        }
        Ok(result)
    }

    pub fn gcd(&self, rhs: &Self) -> Self {
        let mut a = self.abs();
        let mut b = rhs.abs();
        while !b.is_zero() {
            let remainder = a.div_rem(&b).expect("nonzero divisor").1;
            a = b;
            b = remainder;
        }
        a
    }
}

fn main() {}