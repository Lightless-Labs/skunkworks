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

    fn from_u32(value: u32) -> Self {
        if value == 0 {
            Self::zero()
        } else {
            Self {
                negative: false,
                limbs: vec![value],
            }
        }
    }

    fn cmp_abs(&self, rhs: &Self) -> Ordering {
        match self.limbs.len().cmp(&rhs.limbs.len()) {
            Ordering::Equal => self
                .limbs
                .iter()
                .rev()
                .cmp(rhs.limbs.iter().rev()),
            other => other,
        }
    }

    fn add_abs(&self, rhs: &Self) -> Self {
        let n = self.limbs.len().max(rhs.limbs.len());
        let mut limbs = Vec::with_capacity(n + 1);
        let mut carry = 0u64;

        for i in 0..n {
            let a = self.limbs.get(i).copied().unwrap_or(0) as u64;
            let b = rhs.limbs.get(i).copied().unwrap_or(0) as u64;
            let sum = a + b + carry;
            limbs.push((sum % Self::BASE) as u32);
            carry = sum / Self::BASE;
        }

        if carry != 0 {
            limbs.push(carry as u32);
        }

        Self {
            negative: false,
            limbs,
        }
    }

    // Requires |self| >= |rhs|.
    fn sub_abs(&self, rhs: &Self) -> Self {
        let mut limbs = Vec::with_capacity(self.limbs.len());
        let mut borrow = 0i64;

        for i in 0..self.limbs.len() {
            let a = self.limbs[i] as i64;
            let b = rhs.limbs.get(i).copied().unwrap_or(0) as i64;
            let mut difference = a - b - borrow;
            if difference < 0 {
                difference += Self::BASE as i64;
                borrow = 1;
            } else {
                borrow = 0;
            }
            limbs.push(difference as u32);
        }

        let mut result = Self {
            negative: false,
            limbs,
        };
        result.normalize();
        result
    }

    fn mul_small_abs(&mut self, value: u32) {
        if value == 0 || self.is_zero() {
            self.limbs.clear();
            self.negative = false;
            return;
        }

        let mut carry = 0u64;
        for limb in &mut self.limbs {
            let product = *limb as u64 * value as u64 + carry;
            *limb = (product % Self::BASE) as u32;
            carry = product / Self::BASE;
        }

        while carry != 0 {
            self.limbs.push((carry % Self::BASE) as u32);
            carry /= Self::BASE;
        }
    }

    fn add_small_abs(&mut self, value: u32) {
        if value == 0 {
            return;
        }

        let mut carry = value as u64;
        let mut i = 0;
        while carry != 0 {
            if i == self.limbs.len() {
                self.limbs.push(0);
            }

            let sum = self.limbs[i] as u64 + carry;
            self.limbs[i] = (sum % Self::BASE) as u32;
            carry = sum / Self::BASE;
            i += 1;
        }
    }

    fn div_rem_small_abs(&self, divisor: u32) -> (Self, u32) {
        let mut quotient = vec![0; self.limbs.len()];
        let mut remainder = 0u64;

        for i in (0..self.limbs.len()).rev() {
            let current = remainder * Self::BASE + self.limbs[i] as u64;
            quotient[i] = (current / divisor as u64) as u32;
            remainder = current % divisor as u64;
        }

        let mut q = Self {
            negative: false,
            limbs: quotient,
        };
        q.normalize();
        (q, remainder as u32)
    }

    fn mul_abs(&self, rhs: &Self) -> Self {
        if self.is_zero() || rhs.is_zero() {
            return Self::zero();
        }

        let mut limbs = vec![0u32; self.limbs.len() + rhs.limbs.len()];

        for (i, &a) in self.limbs.iter().enumerate() {
            let mut carry = 0u64;

            for (j, &b) in rhs.limbs.iter().enumerate() {
                let index = i + j;
                let product =
                    limbs[index] as u64 + a as u64 * b as u64 + carry;
                limbs[index] = (product % Self::BASE) as u32;
                carry = product / Self::BASE;
            }

            let mut index = i + rhs.limbs.len();
            while carry != 0 {
                if index == limbs.len() {
                    limbs.push(0);
                }
                let sum = limbs[index] as u64 + carry;
                limbs[index] = (sum % Self::BASE) as u32;
                carry = sum / Self::BASE;
                index += 1;
            }
        }

        let mut result = Self {
            negative: false,
            limbs,
        };
        result.normalize();
        result
    }

    // Requires a nonzero rhs magnitude.
    fn div_rem_abs(&self, rhs: &Self) -> (Self, Self) {
        if self.cmp_abs(rhs) == Ordering::Less {
            let mut remainder = self.clone();
            remainder.negative = false;
            return (Self::zero(), remainder);
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

            let mut low = 0u64;
            let mut high = Self::BASE - 1;
            let mut best = 0u64;

            while low <= high {
                let mid = low + (high - low) / 2;
                let product = rhs.mul_small_abs_copy(mid as u32);
                match product.cmp_abs(&remainder) {
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

            quotient_be.push(best as u32);
            if best != 0 {
                let product = rhs.mul_small_abs_copy(best as u32);
                remainder = remainder.sub_abs(&product);
            }
        }

        quotient_be.reverse();
        let mut quotient = Self {
            negative: false,
            limbs: quotient_be,
        };
        quotient.normalize();
        remainder.normalize();
        (quotient, remainder)
    }

    fn mul_small_abs_copy(&self, value: u32) -> Self {
        let mut result = self.clone();
        result.negative = false;
        result.mul_small_abs(value);
        result
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
        if text.is_empty() {
            return Err(Error::Empty);
        }

        let bytes = text.as_bytes();
        let has_sign = bytes[0] == b'+' || bytes[0] == b'-';
        let negative = bytes[0] == b'-';
        let start = usize::from(has_sign);

        if start == bytes.len() {
            return Err(Error::SignOnly);
        }

        // Invalid digits take precedence over malformed separators.
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' && Self::digit_value(byte).is_none_or(|d| d >= radix) {
                return Err(Error::InvalidDigit { index });
            }
        }

        // Find the first malformed separator after all digit bytes pass.
        for index in start..bytes.len() {
            if bytes[index] == b'_'
                && (index == start
                    || index + 1 == bytes.len()
                    || bytes[index - 1] == b'_'
                    || bytes[index + 1] == b'_')
            {
                return Err(Error::Separator { index });
            }
        }

        let digit_indices: Vec<usize> = (start..bytes.len())
            .filter(|&i| bytes[i] != b'_')
            .collect();

        if digit_indices.len() > 1
            && Self::digit_value(bytes[digit_indices[0]]) == Some(0)
        {
            return Err(Error::LeadingZero {
                index: digit_indices[0],
            });
        }

        if digit_indices.len() > max_digits {
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: digit_indices[max_digits],
            });
        }

        let mut value = Self::zero();
        for index in digit_indices {
            let digit = Self::digit_value(bytes[index]).unwrap();
            value.mul_small_abs(radix);
            value.add_small_abs(digit);
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

        let mut magnitude = self.clone();
        magnitude.negative = false;
        let mut digits = Vec::new();

        if magnitude.is_zero() {
            digits.push(0);
        } else {
            while !magnitude.is_zero() {
                let (quotient, remainder) = magnitude.div_rem_small_abs(radix);
                digits.push(remainder);
                magnitude = quotient;
            }
            digits.reverse();
        }

        if digits.len() < min_digits {
            let padding = min_digits - digits.len();
            let mut padded = Vec::with_capacity(min_digits);
            padded.resize(padding, 0);
            padded.extend(digits);
            digits = padded;
        }

        let mut output = String::new();
        if self.negative && !self.is_zero() {
            output.push('-');
        }

        for (i, digit) in digits.iter().enumerate() {
            if group > 0 && i > 0 && (digits.len() - i) % group == 0 {
                output.push('_');
            }

            let byte = match *digit {
                0..=9 => b'0' + *digit as u8,
                _ => b'a' + (*digit as u8 - 10),
            };
            output.push(byte as char);
        }

        Ok(output)
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.negative, rhs.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => self.cmp_abs(rhs),
            (true, true) => rhs.cmp_abs(self),
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
                    result.negative = self.negative && !result.is_zero();
                    result
                }
                Ordering::Less => {
                    let mut result = rhs.sub_abs(self);
                    result.negative = rhs.negative && !result.is_zero();
                    result
                }
            }
        }
    }

    pub fn sub(&self, rhs: &Self) -> Self {
        let mut negated = rhs.clone();
        if !negated.is_zero() {
            negated.negative = !negated.negative;
        }
        self.add(&negated)
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
        result.negative =
            !result.is_zero() && (self.negative != rhs.negative);
        result
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }

        let (mut quotient, mut remainder) = self.div_rem_abs(rhs);
        quotient.negative =
            !quotient.is_zero() && (self.negative != rhs.negative);
        remainder.negative = !remainder.is_zero() && self.negative;
        Ok((quotient, remainder))
    }

    pub fn pow(&self, exponent: u32) -> Result<Self, Error> {
        if exponent > Self::EXPONENT_LIMIT {
            return Err(Error::ExponentTooLarge {
                exponent,
                limit: Self::EXPONENT_LIMIT,
            });
        }

        let mut power = exponent;
        let mut base = self.clone();
        let mut result = Self::one();

        while power != 0 {
            if power & 1 == 1 {
                result = result.mul(&base);
            }
            power >>= 1;
            if power != 0 {
                base = base.mul(&base);
            }
        }

        Ok(result)
    }

    pub fn gcd(&self, rhs: &Self) -> Self {
        let mut a = self.clone();
        let mut b = rhs.clone();
        a.negative = false;
        b.negative = false;

        while !b.is_zero() {
            let (_, remainder) = a.div_rem_abs(&b);
            a = b;
            b = remainder;
        }

        a.negative = false;
        a
    }
}

fn main() {}