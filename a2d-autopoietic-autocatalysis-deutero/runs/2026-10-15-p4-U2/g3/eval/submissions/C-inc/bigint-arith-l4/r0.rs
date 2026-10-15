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
    // Little-endian base-10^9 limbs.
    limbs: Vec<u32>,
}

impl Big {
    const BASE: u64 = 1_000_000_000;
    const EXPONENT_LIMIT: u32 = 1024;

    fn zero() -> Self {
        Self {
            negative: false,
            limbs: Vec::new(),
        }
    }

    fn one() -> Self {
        Self { negative: false, limbs: vec![1] }
    }

    fn normalize(&mut self) {
        while self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }
        if self.limbs.is_empty() {
            self.negative = false;
        }
    }

    fn is_zero(&self) -> bool {
        self.limbs.is_empty()
    }

    fn abs_cmp(&self, rhs: &Self) -> Ordering {
        self.limbs
            .len()
            .cmp(&rhs.limbs.len())
            .then_with(|| self.limbs.iter().rev().cmp(rhs.limbs.iter().rev()))
    }

    fn abs_add(&self, rhs: &Self) -> Vec<u32> {
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
        out
    }

    // Requires |self| >= |rhs|.
    fn abs_sub(&self, rhs: &Self) -> Vec<u32> {
        let mut out = Vec::with_capacity(self.limbs.len());
        let mut borrow = 0i64;
        for i in 0..self.limbs.len() {
            let a = self.limbs[i] as i64;
            let b = rhs.limbs.get(i).copied().unwrap_or(0) as i64;
            let mut diff = a - b - borrow;
            if diff < 0 {
                diff += Self::BASE as i64;
                borrow = 1;
            } else {
                borrow = 0;
            }
            out.push(diff as u32);
        }
        while out.last() == Some(&0) {
            out.pop();
        }
        out
    }

    fn mul_small_abs(&mut self, multiplier: u32) {
        if multiplier == 0 || self.is_zero() {
            self.limbs.clear();
            self.negative = false;
            return;
        }
        let mut carry = 0u64;
        for limb in &mut self.limbs {
            let n = *limb as u64 * multiplier as u64 + carry;
            *limb = (n % Self::BASE) as u32;
            carry = n / Self::BASE;
        }
        while carry != 0 {
            self.limbs.push((carry % Self::BASE) as u32);
            carry /= Self::BASE;
        }
    }

    fn add_small_abs(&mut self, value: u32) {
        let mut carry = value as u64;
        let mut i = 0;
        while carry != 0 {
            if i == self.limbs.len() {
                self.limbs.push(0);
            }
            let n = self.limbs[i] as u64 + carry;
            self.limbs[i] = (n % Self::BASE) as u32;
            carry = n / Self::BASE;
            i += 1;
        }
    }

    fn div_small_abs(&mut self, divisor: u32) -> u32 {
        let mut rem = 0u64;
        for limb in self.limbs.iter_mut().rev() {
            let n = rem * Self::BASE + *limb as u64;
            *limb = (n / divisor as u64) as u32;
            rem = n % divisor as u64;
        }
        self.normalize();
        rem as u32
    }

    fn mul_abs(&self, rhs: &Self) -> Vec<u32> {
        if self.is_zero() || rhs.is_zero() {
            return Vec::new();
        }
        let mut out = vec![0u32; self.limbs.len() + rhs.limbs.len()];
        for (i, &a) in self.limbs.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &b) in rhs.limbs.iter().enumerate() {
                let k = i + j;
                let n = out[k] as u64 + a as u64 * b as u64 + carry;
                out[k] = (n % Self::BASE) as u32;
                carry = n / Self::BASE;
            }
            let mut k = i + rhs.limbs.len();
            while carry != 0 {
                if k == out.len() {
                    out.push(0);
                }
                let n = out[k] as u64 + carry;
                out[k] = (n % Self::BASE) as u32;
                carry = n / Self::BASE;
                k += 1;
            }
        }
        while out.last() == Some(&0) {
            out.pop();
        }
        out
    }

    fn abs_value(&self) -> Self {
        Self { negative: false, limbs: self.limbs.clone() }
    }

    fn abs_div_rem(&self, divisor: &Self) -> (Self, Self) {
        let mut quotient = Self::zero();
        let mut remainder = Self::zero();

        // Process the base-10^9 limbs from most significant to least.
        for &limb in self.limbs.iter().rev() {
            remainder.limbs.insert(0, limb);
            remainder.normalize();

            let mut low = 0u64;
            let mut high = Self::BASE - 1;
            let mut best = 0u64;
            while low <= high {
                let mid = low + (high - low) / 2;
                let mut product = divisor.clone();
                product.negative = false;
                product.mul_small_abs(mid as u32);
                match product.abs_cmp(&remainder) {
                    Ordering::Greater => {
                        if mid == 0 {
                            break;
                        }
                        high = mid - 1;
                    }
                    _ => {
                        best = mid;
                        low = mid + 1;
                    }
                }
            }

            quotient.limbs.insert(0, best as u32);
            if best != 0 {
                let mut product = divisor.clone();
                product.negative = false;
                product.mul_small_abs(best as u32);
                remainder.limbs = remainder.abs_sub(&product);
            }
        }
        quotient.normalize();
        remainder.normalize();
        (quotient, remainder)
    }

    fn digit_value(byte: u8) -> Option<u32> {
        match byte {
            b'0'..=b'9' => Some((byte - b'0') as u32),
            b'a'..=b'z' => Some((byte - b'a' + 10) as u32),
            b'A'..=b'Z' => Some((byte - b'A' + 10) as u32),
            _ => None,
        }
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
            b'-' => (true, 1),
            b'+' => (false, 1),
            _ => (false, 0),
        };
        if start == bytes.len() {
            return Err(Error::SignOnly);
        }

        let mut invalid = None;
        for i in start..bytes.len() {
            if bytes[i] == b'_' {
                continue;
            }
            if Self::digit_value(bytes[i]).is_none_or(|v| v >= radix) {
                invalid = Some(i);
                break;
            }
        }
        if let Some(index) = invalid {
            return Err(Error::InvalidDigit { index });
        }

        let mut separator_error = None;
        for i in start..bytes.len() {
            if bytes[i] == b'_' {
                let prev_valid = i > start
                    && bytes[i - 1] != b'_'
                    && Self::digit_value(bytes[i - 1]).is_some_and(|v| v < radix);
                let next_valid = i + 1 < bytes.len()
                    && bytes[i + 1] != b'_'
                    && Self::digit_value(bytes[i + 1]).is_some_and(|v| v < radix);
                if !(prev_valid && next_valid) {
                    separator_error = Some(i);
                    break;
                }
            }
        }
        if let Some(index) = separator_error {
            return Err(Error::Separator { index });
        }

        let digit_positions: Vec<usize> =
            (start..bytes.len()).filter(|&i| bytes[i] != b'_').collect();
        if digit_positions.len() > 1 && bytes[digit_positions[0]] == b'0' {
            return Err(Error::LeadingZero { index: digit_positions[0] });
        }
        if digit_positions.len() > max_digits {
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: digit_positions[max_digits],
            });
        }

        let mut value = Self::zero();
        for &i in &digit_positions {
            value.mul_small_abs(radix);
            value.add_small_abs(Self::digit_value(bytes[i]).unwrap());
        }
        value.negative = negative && !value.is_zero();
        Ok(value)
    }

    pub fn format(
        &self,
        radix: u32,
        min_digits: usize,
        group: usize,
    ) -> Result<String, Error> {
        if !(2..=36).contains(&radix) {
            return Err(Error::InvalidRadix { radix });
        }
        let mut value = self.abs_value();
        let mut digits = Vec::new();
        if value.is_zero() {
            digits.push(0u8);
        } else {
            while !value.is_zero() {
                digits.push(value.div_small_abs(radix) as u8);
            }
            digits.reverse();
        }
        let target_len = min_digits.max(digits.len()).max(1);
        if digits.len() < target_len {
            let mut padded = vec![0u8; target_len - digits.len()];
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
            out.push(match *digit {
                0..=9 => (b'0' + *digit) as char,
                _ => (b'a' + (*digit - 10)) as char,
            });
        }
        Ok(out)
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.negative, rhs.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => self.abs_cmp(rhs),
            (true, true) => self.abs_cmp(rhs).reverse(),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        let mut result = match (self.negative, rhs.negative) {
            (a, b) if a == b => Self { negative: a, limbs: self.abs_add(rhs) },
            _ => match self.abs_cmp(rhs) {
                Ordering::Greater => Self {
                    negative: self.negative,
                    limbs: self.abs_sub(rhs),
                },
                Ordering::Less => Self {
                    negative: rhs.negative,
                    limbs: rhs.abs_sub(self),
                },
                Ordering::Equal => Self::zero(),
            },
        };
        result.normalize();
        result
    }

    pub fn sub(&self, rhs: &Self) -> Self {
        self.add(&rhs.neg())
    }

    pub fn neg(&self) -> Self {
        let mut out = self.clone();
        if !out.is_zero() {
            out.negative = !out.negative;
        }
        out
    }

    pub fn signum(&self) -> i8 {
        if self.is_zero() { 0 } else if self.negative { -1 } else { 1 }
    }

    pub fn mul(&self, rhs: &Self) -> Self {
        let mut out = Self {
            negative: self.negative ^ rhs.negative,
            limbs: self.mul_abs(rhs),
        };
        out.normalize();
        out
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }
        let (mut quotient, mut remainder) =
            self.abs_value().abs_div_rem(&rhs.abs_value());
        quotient.negative = (self.negative ^ rhs.negative) && !quotient.is_zero();
        remainder.negative = self.negative && !remainder.is_zero();
        Ok((quotient, remainder))
    }

    pub fn pow(&self, exponent: u32) -> Result<Self, Error> {
        if exponent > Self::EXPONENT_LIMIT {
            return Err(Error::ExponentTooLarge {
                exponent,
                limit: Self::EXPONENT_LIMIT,
            });
        }
        let mut exp = exponent;
        let mut base = self.clone();
        let mut result = Self::one();
        while exp != 0 {
            if exp & 1 == 1 {
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
        let mut a = self.abs_value();
        let mut b = rhs.abs_value();
        while !b.is_zero() {
            let remainder = a.div_rem(&b).expect("nonzero divisor").1.abs_value();
            a = b;
            b = remainder;
        }
        a.negative = false;
        a
    }
}

fn main() {}