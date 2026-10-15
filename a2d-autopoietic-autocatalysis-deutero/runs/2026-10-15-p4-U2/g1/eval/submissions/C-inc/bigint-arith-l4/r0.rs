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
    // Little-endian base-1_000_000_000 limbs.
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

    fn digit_value(byte: u8) -> Option<u32> {
        match byte {
            b'0'..=b'9' => Some((byte - b'0') as u32),
            b'a'..=b'z' => Some((byte - b'a' + 10) as u32),
            b'A'..=b'Z' => Some((byte - b'A' + 10) as u32),
            _ => None,
        }
    }

    fn mul_small_assign(&mut self, multiplier: u32) {
        if multiplier == 0 || self.is_zero() {
            self.limbs.clear();
            self.negative = false;
            return;
        }
        let mut carry = 0u64;
        for limb in &mut self.limbs {
            let value = *limb as u64 * multiplier as u64 + carry;
            *limb = (value % Self::BASE) as u32;
            carry = value / Self::BASE;
        }
        while carry != 0 {
            self.limbs.push((carry % Self::BASE) as u32);
            carry /= Self::BASE;
        }
    }

    fn add_small_assign(&mut self, value: u32) {
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

    fn add_abs(a: &Self, b: &Self) -> Vec<u32> {
        let n = a.limbs.len().max(b.limbs.len());
        let mut result = Vec::with_capacity(n + 1);
        let mut carry = 0u64;
        for i in 0..n {
            let x = a.limbs.get(i).copied().unwrap_or(0) as u64;
            let y = b.limbs.get(i).copied().unwrap_or(0) as u64;
            let sum = x + y + carry;
            result.push((sum % Self::BASE) as u32);
            carry = sum / Self::BASE;
        }
        if carry != 0 {
            result.push(carry as u32);
        }
        result
    }

    // Requires |a| >= |b|.
    fn sub_abs(a: &Self, b: &Self) -> Vec<u32> {
        let mut result = Vec::with_capacity(a.limbs.len());
        let mut borrow = 0i64;
        for i in 0..a.limbs.len() {
            let x = a.limbs[i] as i64;
            let y = b.limbs.get(i).copied().unwrap_or(0) as i64;
            let mut digit = x - y - borrow;
            if digit < 0 {
                digit += Self::BASE as i64;
                borrow = 1;
            } else {
                borrow = 0;
            }
            result.push(digit as u32);
        }
        while result.last() == Some(&0) {
            result.pop();
        }
        result
    }

    fn abs(&self) -> Self {
        Self {
            negative: false,
            limbs: self.limbs.clone(),
        }
    }

    fn div_rem_abs(dividend: &Self, divisor: &Self) -> (Self, Self) {
        if dividend.cmp_abs(divisor) == Ordering::Less {
            return (Self::zero(), dividend.abs());
        }

        let mut quotient_be = Vec::with_capacity(dividend.limbs.len());
        let mut remainder = Self::zero();

        for &limb in dividend.limbs.iter().rev() {
            if !remainder.is_zero() {
                remainder.limbs.insert(0, limb);
            } else if limb != 0 {
                remainder.limbs.push(limb);
            }
            remainder.normalize();

            let mut low = 0u64;
            let mut high = Self::BASE - 1;
            let mut best = 0u64;
            while low <= high {
                let mid = low + (high - low) / 2;
                let mut product = divisor.abs();
                product.mul_small_assign(mid as u32);
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
                let mut product = divisor.abs();
                product.mul_small_assign(best as u32);
                remainder.limbs = Self::sub_abs(&remainder, &product);
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
                        .is_some_and(|d| d < radix);
                let next_valid = index + 1 < bytes.len()
                    && Self::digit_value(bytes[index + 1])
                        .is_some_and(|d| d < radix);
                if !(prev_valid && next_valid) {
                    separator_error = Some(index);
                    break;
                }
            }
        }
        if let Some(index) = separator_error {
            return Err(Error::Separator { index });
        }

        let first_digit_index = (start..bytes.len()).find(|&i| bytes[i] != b'_').unwrap();
        let digit_count = bytes[start..].iter().filter(|&&b| b != b'_').count();
        if digit_count > 1 && bytes[first_digit_index] == b'0' {
            return Err(Error::LeadingZero {
                index: first_digit_index,
            });
        }

        if digit_count > max_digits {
            let excess_number = max_digits;
            let mut seen = 0usize;
            for index in start..bytes.len() {
                if bytes[index] != b'_' {
                    seen += 1;
                    if seen == excess_number + 1 {
                        return Err(Error::TooManyDigits {
                            limit: max_digits,
                            index,
                        });
                    }
                }
            }
        }

        let mut result = Self::zero();
        for &byte in &bytes[start..] {
            if byte != b'_' {
                result.mul_small_assign(radix);
                result.add_small_assign(Self::digit_value(byte).unwrap());
            }
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

        let mut value = self.abs();
        let mut digits = Vec::new();
        if value.is_zero() {
            digits.push(0u8);
        } else {
            while !value.is_zero() {
                let mut remainder = 0u64;
                for limb in value.limbs.iter_mut().rev() {
                    let current = remainder * Self::BASE + *limb as u64;
                    *limb = (current / radix as u64) as u32;
                    remainder = current % radix as u64;
                }
                value.normalize();
                digits.push(remainder as u8);
            }
            digits.reverse();
        }

        while digits.len() < min_digits {
            digits.insert(0, 0);
        }

        let mut out = String::new();
        if self.negative && !self.is_zero() {
            out.push('-');
        }
        for (i, digit) in digits.iter().enumerate() {
            if group > 0 && i > 0 && (digits.len() - i) % group == 0 {
                out.push('_');
            }
            let ch = if *digit < 10 {
                (b'0' + *digit) as char
            } else {
                (b'a' + (*digit - 10)) as char
            };
            out.push(ch);
        }
        Ok(out)
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
        let mut result = match (self.negative, rhs.negative) {
            (a, b) if a == b => Self {
                negative: a,
                limbs: Self::add_abs(self, rhs),
            },
            _ => match self.cmp_abs(rhs) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => Self {
                    negative: self.negative,
                    limbs: Self::sub_abs(self, rhs),
                },
                Ordering::Less => Self {
                    negative: rhs.negative,
                    limbs: Self::sub_abs(rhs, self),
                },
            },
        };
        result.normalize();
        result
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
        let mut result = vec![0u32; self.limbs.len() + rhs.limbs.len()];
        for (i, &a) in self.limbs.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &b) in rhs.limbs.iter().enumerate() {
                let index = i + j;
                let current = result[index] as u64 + a as u64 * b as u64 + carry;
                result[index] = (current % Self::BASE) as u32;
                carry = current / Self::BASE;
            }
            let mut index = i + rhs.limbs.len();
            while carry != 0 {
                if index == result.len() {
                    result.push(0);
                }
                let current = result[index] as u64 + carry;
                result[index] = (current % Self::BASE) as u32;
                carry = current / Self::BASE;
                index += 1;
            }
        }
        let mut value = Self {
            negative: self.negative ^ rhs.negative,
            limbs: result,
        };
        value.normalize();
        value
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }
        let (mut quotient, mut remainder) = Self::div_rem_abs(self, rhs);
        quotient.negative = !quotient.is_zero() && (self.negative ^ rhs.negative);
        remainder.negative = !remainder.is_zero() && self.negative;
        Ok((quotient, remainder))
    }

    pub fn pow(&self, exponent: u32) -> Result<Self, Error> {
        if exponent > 1024 {
            return Err(Error::ExponentTooLarge {
                exponent,
                limit: 1024,
            });
        }
        let mut result = Self::one();
        let mut base = self.clone();
        let mut power = exponent;
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
            let remainder = a.div_rem(&b).unwrap().1;
            a = b;
            b = remainder.abs();
        }
        a.negative = false;
        a
    }
}

fn main() {}