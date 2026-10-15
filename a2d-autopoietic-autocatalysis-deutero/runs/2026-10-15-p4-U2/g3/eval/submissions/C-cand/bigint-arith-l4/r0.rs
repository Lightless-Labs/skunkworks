use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    negative: bool,
    // Little-endian base-1_000_000_000 limbs.
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
    DivisionByZero,
    InvalidRadix { radix: u32 },
    ExponentTooLarge { exponent: u32, limit: u32 },
}

impl Big {
    const BASE: u64 = 1_000_000_000;
    const POW_LIMIT: u32 = 1024;

    fn zero() -> Self {
        Self {
            negative: false,
            limbs: vec![0],
        }
    }

    fn one() -> Self {
        Self {
            negative: false,
            limbs: vec![1],
        }
    }

    fn normalize(&mut self) {
        while self.limbs.len() > 1 && self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }
        if self.limbs.is_empty() {
            self.limbs.push(0);
        }
        if self.is_zero() {
            self.negative = false;
        }
    }

    fn is_zero(&self) -> bool {
        self.limbs.len() == 1 && self.limbs[0] == 0
    }

    fn abs(&self) -> Self {
        let mut result = self.clone();
        result.negative = false;
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

    fn mul_small_assign(&mut self, factor: u32) {
        if factor == 0 {
            *self = Self::zero();
            return;
        }
        let mut carry = 0u64;
        for limb in &mut self.limbs {
            let value = *limb as u64 * factor as u64 + carry;
            *limb = (value % Self::BASE) as u32;
            carry = value / Self::BASE;
        }
        while carry != 0 {
            self.limbs.push((carry % Self::BASE) as u32);
            carry /= Self::BASE;
        }
    }

    fn add_small_assign(&mut self, addend: u32) {
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

    fn div_small_assign(&mut self, divisor: u32) -> u32 {
        let mut remainder = 0u64;
        for limb in self.limbs.iter_mut().rev() {
            let value = remainder * Self::BASE + *limb as u64;
            *limb = (value / divisor as u64) as u32;
            remainder = value % divisor as u64;
        }
        self.normalize();
        remainder as u32
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

    fn add_abs(lhs: &Self, rhs: &Self) -> Self {
        let len = lhs.limbs.len().max(rhs.limbs.len());
        let mut limbs = Vec::with_capacity(len + 1);
        let mut carry = 0u64;
        for i in 0..len {
            let a = lhs.limbs.get(i).copied().unwrap_or(0) as u64;
            let b = rhs.limbs.get(i).copied().unwrap_or(0) as u64;
            let sum = a + b + carry;
            limbs.push((sum % Self::BASE) as u32);
            carry = sum / Self::BASE;
        }
        if carry != 0 {
            limbs.push(carry as u32);
        }
        let mut result = Self {
            negative: false,
            limbs,
        };
        result.normalize();
        result
    }

    // Requires |lhs| >= |rhs|.
    fn sub_abs(lhs: &Self, rhs: &Self) -> Self {
        let mut limbs = Vec::with_capacity(lhs.limbs.len());
        let mut borrow = 0i64;
        for i in 0..lhs.limbs.len() {
            let a = lhs.limbs[i] as i64 - borrow;
            let b = rhs.limbs.get(i).copied().unwrap_or(0) as i64;
            if a < b {
                limbs.push((a + Self::BASE as i64 - b) as u32);
                borrow = 1;
            } else {
                limbs.push((a - b) as u32);
                borrow = 0;
            }
        }
        let mut result = Self {
            negative: false,
            limbs,
        };
        result.normalize();
        result
    }

    fn mul_abs(lhs: &Self, rhs: &Self) -> Self {
        if lhs.is_zero() || rhs.is_zero() {
            return Self::zero();
        }
        let mut accum = vec![0u128; lhs.limbs.len() + rhs.limbs.len()];
        for (i, &a) in lhs.limbs.iter().enumerate() {
            for (j, &b) in rhs.limbs.iter().enumerate() {
                accum[i + j] += a as u128 * b as u128;
            }
        }

        let base = Self::BASE as u128;
        let mut limbs = Vec::with_capacity(accum.len() + 1);
        let mut carry = 0u128;
        for value in accum {
            let total = value + carry;
            limbs.push((total % base) as u32);
            carry = total / base;
        }
        while carry != 0 {
            limbs.push((carry % base) as u32);
            carry /= base;
        }

        let mut result = Self {
            negative: false,
            limbs,
        };
        result.normalize();
        result
    }

    fn mul_abs_small(lhs: &Self, factor: u32) -> Self {
        let mut result = lhs.clone();
        result.negative = false;
        result.mul_small_assign(factor);
        result
    }

    // Requires rhs > 0.
    fn div_rem_abs(lhs: &Self, rhs: &Self) -> (Self, Self) {
        if lhs.cmp_abs(rhs) == Ordering::Less {
            return (Self::zero(), lhs.abs());
        }

        let mut quotient_be = Vec::with_capacity(lhs.limbs.len());
        let mut remainder = Self::zero();

        for &limb in lhs.limbs.iter().rev() {
            if remainder.is_zero() {
                remainder.limbs[0] = limb;
            } else {
                remainder.limbs.insert(0, limb);
            }
            remainder.normalize();

            let mut low = 0u32;
            let mut high = (Self::BASE - 1) as u32;
            let mut best = 0u32;
            while low <= high {
                let mid = low + (high - low) / 2;
                let product = Self::mul_abs_small(rhs, mid);
                match product.cmp_abs(&remainder) {
                    Ordering::Greater => {
                        if mid == 0 {
                            break;
                        }
                        high = mid - 1;
                    }
                    _ => {
                        best = mid;
                        if mid == u32::MAX {
                            break;
                        }
                        low = mid + 1;
                    }
                }
            }

            if best != 0 {
                let product = Self::mul_abs_small(rhs, best);
                remainder = Self::sub_abs(&remainder, &product);
            }
            quotient_be.push(best);
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

    pub fn parse(text: &str, radix: u32, max_digits: usize) -> Result<Self, Error> {
        if !(2..=36).contains(&radix) {
            return Err(Error::InvalidRadix { radix });
        }
        if text.is_empty() {
            return Err(Error::Empty);
        }

        let bytes = text.as_bytes();
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
            match Self::digit_value(byte) {
                Some(value) if value < radix => {}
                _ => {
                    invalid = Some(index);
                    break;
                }
            }
        }
        if let Some(index) = invalid {
            return Err(Error::InvalidDigit { index });
        }

        let mut separator_error = None;
        for index in start..bytes.len() {
            if bytes[index] == b'_' {
                let prev_valid = index > start
                    && Self::digit_value(bytes[index - 1])
                        .is_some_and(|value| value < radix);
                let next_valid = index + 1 < bytes.len()
                    && Self::digit_value(bytes[index + 1])
                        .is_some_and(|value| value < radix);
                if !(prev_valid && next_valid) {
                    separator_error = Some(index);
                    break;
                }
            }
        }
        if let Some(index) = separator_error {
            return Err(Error::Separator { index });
        }

        let mut digit_count = 0usize;
        let mut first_digit = None;
        let mut first_excess = None;
        let mut result = Self::zero();

        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte == b'_' {
                continue;
            }
            let digit = Self::digit_value(byte).unwrap();
            if first_digit.is_none() {
                first_digit = Some((digit, index));
            }
            digit_count += 1;
            if digit_count == max_digits.saturating_add(1) {
                first_excess = Some(index);
            }
            result.mul_small_assign(radix);
            result.add_small_assign(digit);
        }

        if digit_count > 1 && first_digit.unwrap().0 == 0 {
            return Err(Error::LeadingZero {
                index: first_digit.unwrap().1,
            });
        }
        if digit_count > max_digits {
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: first_excess.unwrap_or_else(|| {
                    bytes
                        .iter()
                        .enumerate()
                        .skip(start)
                        .find(|(_, byte)| **byte != b'_')
                        .map(|(i, _)| i)
                        .unwrap_or(start)
                }),
            });
        }

        result.negative = negative && !result.is_zero();
        Ok(result)
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

        let mut magnitude = self.abs();
        let mut digits = Vec::new();
        if magnitude.is_zero() {
            digits.push(0u8);
        } else {
            while !magnitude.is_zero() {
                digits.push(magnitude.div_small_assign(radix) as u8);
            }
            digits.reverse();
        }

        let desired = min_digits.max(1);
        if digits.len() < desired {
            let mut padded = vec![0u8; desired - digits.len()];
            padded.extend(digits);
            digits = padded;
        }

        let mut output = String::new();
        if self.negative && !self.is_zero() {
            output.push('-');
        }
        for (index, digit) in digits.iter().copied().enumerate() {
            if group > 0 && index > 0 && (digits.len() - index) % group == 0 {
                output.push('_');
            }
            output.push(match digit {
                0..=9 => (b'0' + digit) as char,
                _ => (b'a' + digit - 10) as char,
            });
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
        let mut result = if self.negative == rhs.negative {
            let mut sum = Self::add_abs(self, rhs);
            sum.negative = self.negative;
            sum
        } else {
            match self.cmp_abs(rhs) {
                Ordering::Greater => {
                    let mut difference = Self::sub_abs(self, rhs);
                    difference.negative = self.negative;
                    difference
                }
                Ordering::Less => {
                    let mut difference = Self::sub_abs(rhs, self);
                    difference.negative = rhs.negative;
                    difference
                }
                Ordering::Equal => Self::zero(),
            }
        };
        result.normalize();
        result
    }

    pub fn sub(&self, rhs: &Self) -> Self {
        let mut neg_rhs = rhs.clone();
        if !neg_rhs.is_zero() {
            neg_rhs.negative = !neg_rhs.negative;
        }
        self.add(&neg_rhs)
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
        let mut result = Self::mul_abs(self, rhs);
        result.negative = !result.is_zero() && (self.negative != rhs.negative);
        result
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }
        let (mut quotient, mut remainder) = Self::div_rem_abs(self, rhs);
        quotient.negative = !quotient.is_zero() && (self.negative != rhs.negative);
        remainder.negative = !remainder.is_zero() && self.negative;
        Ok((quotient, remainder))
    }

    pub fn pow(&self, exponent: u32) -> Result<Self, Error> {
        if exponent > Self::POW_LIMIT {
            return Err(Error::ExponentTooLarge {
                exponent,
                limit: Self::POW_LIMIT,
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
        let mut a = self.abs();
        let mut b = rhs.abs();
        while !b.is_zero() {
            let (_, remainder) = Self::div_rem_abs(&a, &b);
            a = b;
            b = remainder;
        }
        a.negative = false;
        a
    }
}

fn main() {}