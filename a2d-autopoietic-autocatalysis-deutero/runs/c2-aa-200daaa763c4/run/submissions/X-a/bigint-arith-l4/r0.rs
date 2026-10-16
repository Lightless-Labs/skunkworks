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

    fn abs_cmp(&self, other: &Self) -> Ordering {
        match self.limbs.len().cmp(&other.limbs.len()) {
            Ordering::Equal => {
                for (a, b) in self.limbs.iter().rev().zip(other.limbs.iter().rev()) {
                    match a.cmp(b) {
                        Ordering::Equal => {}
                        order => return order,
                    }
                }
                Ordering::Equal
            }
            order => order,
        }
    }

    fn abs_add(&self, other: &Self) -> Self {
        let mut out = Vec::with_capacity(self.limbs.len().max(other.limbs.len()) + 1);
        let mut carry = 0u64;
        for i in 0..self.limbs.len().max(other.limbs.len()) {
            let a = self.limbs.get(i).copied().unwrap_or(0) as u64;
            let b = other.limbs.get(i).copied().unwrap_or(0) as u64;
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

    // Requires |self| >= |other|.
    fn abs_sub(&self, other: &Self) -> Self {
        let mut out = Vec::with_capacity(self.limbs.len());
        let mut borrow = 0i64;
        for i in 0..self.limbs.len() {
            let a = self.limbs[i] as i64;
            let b = other.limbs.get(i).copied().unwrap_or(0) as i64;
            let mut value = a - b - borrow;
            if value < 0 {
                value += Self::BASE as i64;
                borrow = 1;
            } else {
                borrow = 0;
            }
            out.push(value as u32);
        }
        let mut result = Self {
            negative: false,
            limbs: out,
        };
        result.normalize();
        result
    }

    fn mul_small_assign(&mut self, factor: u32) {
        if factor == 0 || self.is_zero() {
            self.limbs.clear();
            self.negative = false;
            return;
        }
        let mut carry = 0u64;
        for limb in &mut self.limbs {
            let product = *limb as u64 * factor as u64 + carry;
            *limb = (product % Self::BASE) as u32;
            carry = product / Self::BASE;
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

    fn div_rem_small_assign(&mut self, divisor: u32) -> u32 {
        let mut remainder = 0u64;
        for limb in self.limbs.iter_mut().rev() {
            let current = remainder * Self::BASE + *limb as u64;
            *limb = (current / divisor as u64) as u32;
            remainder = current % divisor as u64;
        }
        self.normalize();
        remainder as u32
    }

    fn abs_mul(&self, other: &Self) -> Self {
        if self.is_zero() || other.is_zero() {
            return Self::zero();
        }
        let mut out = vec![0u32; self.limbs.len() + other.limbs.len()];
        for (i, &a) in self.limbs.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &b) in other.limbs.iter().enumerate() {
                let index = i + j;
                let product = out[index] as u64 + a as u64 * b as u64 + carry;
                out[index] = (product % Self::BASE) as u32;
                carry = product / Self::BASE;
            }
            let mut index = i + other.limbs.len();
            while carry != 0 {
                if index == out.len() {
                    out.push(0);
                }
                let sum = out[index] as u64 + carry;
                out[index] = (sum % Self::BASE) as u32;
                carry = sum / Self::BASE;
                index += 1;
            }
        }
        let mut result = Self {
            negative: false,
            limbs: out,
        };
        result.normalize();
        result
    }

    fn abs_div_rem(&self, divisor: &Self) -> (Self, Self) {
        debug_assert!(!divisor.is_zero());
        if self.abs_cmp(divisor) == Ordering::Less {
            let mut remainder = self.clone();
            remainder.negative = false;
            return (Self::zero(), remainder);
        }

        let mut quotient_be = Vec::with_capacity(self.limbs.len());
        let mut remainder = Self::zero();
        for &limb in self.limbs.iter().rev() {
            remainder.mul_small_assign(Self::BASE as u32);
            remainder.add_small_assign(limb);

            let mut low = 0u64;
            let mut high = Self::BASE - 1;
            let mut best = 0u64;
            while low <= high {
                let mid = (low + high) / 2;
                let product = divisor.abs_mul_small(mid as u32);
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
            if best != 0 {
                let product = divisor.abs_mul_small(best as u32);
                remainder = remainder.abs_sub(&product);
            }
            quotient_be.push(best as u32);
        }
        quotient_be.reverse();
        let mut quotient = Self {
            negative: false,
            limbs: quotient_be,
        };
        quotient.normalize();
        (quotient, remainder)
    }

    fn abs_mul_small(&self, factor: u32) -> Self {
        let mut out = self.clone();
        out.negative = false;
        out.mul_small_assign(factor);
        out
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
        let mut digits = Vec::new();
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte == b'_' {
                continue;
            }
            match Self::digit_value(byte) {
                Some(value) if value < radix => digits.push((index, value)),
                _ => {
                    if invalid.is_none() {
                        invalid = Some(index);
                    }
                }
            }
        }
        if let Some(index) = invalid {
            return Err(Error::InvalidDigit { index });
        }

        let mut malformed_separator = None;
        for index in start..bytes.len() {
            if bytes[index] == b'_' {
                let previous_is_digit = index > start
                    && Self::digit_value(bytes[index - 1]).is_some_and(|d| d < radix);
                let next_is_digit = index + 1 < bytes.len()
                    && Self::digit_value(bytes[index + 1]).is_some_and(|d| d < radix);
                if !(previous_is_digit && next_is_digit) {
                    malformed_separator = Some(index);
                    break;
                }
            }
        }
        if let Some(index) = malformed_separator {
            return Err(Error::Separator { index });
        }

        if digits.len() > 1 && digits[0].1 == 0 {
            return Err(Error::LeadingZero { index: digits[0].0 });
        }

        if digits.len() > max_digits {
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: digits[max_digits].0,
            });
        }

        let mut result = Self::zero();
        for &(_, digit) in &digits {
            result.mul_small_assign(radix);
            result.add_small_assign(digit);
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

        let mut magnitude = self.clone();
        magnitude.negative = false;
        let mut digits = Vec::new();
        if magnitude.is_zero() {
            digits.push('0');
        } else {
            while !magnitude.is_zero() {
                let remainder = magnitude.div_rem_small_assign(radix);
                let ch = if remainder < 10 {
                    (b'0' + remainder as u8) as char
                } else {
                    (b'a' + (remainder - 10) as u8) as char
                };
                digits.push(ch);
            }
            digits.reverse();
        }
        if digits.len() < min_digits {
            let mut padded = vec!['0'; min_digits - digits.len()];
            padded.extend(digits);
            digits = padded;
        }

        let mut output = String::new();
        if self.negative && !self.is_zero() {
            output.push('-');
        }
        for (i, ch) in digits.iter().enumerate() {
            if group > 0 && i > 0 && (digits.len() - i) % group == 0 {
                output.push('_');
            }
            output.push(*ch);
        }
        Ok(output)
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.negative, rhs.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => self.abs_cmp(rhs),
            (true, true) => rhs.abs_cmp(self),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            let mut result = self.abs_add(rhs);
            result.negative = self.negative && !result.is_zero();
            result
        } else {
            match self.abs_cmp(rhs) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => {
                    let mut result = self.abs_sub(rhs);
                    result.negative = self.negative;
                    result
                }
                Ordering::Less => {
                    let mut result = rhs.abs_sub(self);
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
        let mut result = self.abs_mul(rhs);
        result.negative = (self.negative != rhs.negative) && !result.is_zero();
        result
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }
        let mut dividend = self.clone();
        let mut divisor = rhs.clone();
        dividend.negative = false;
        divisor.negative = false;
        let (mut quotient, mut remainder) = dividend.abs_div_rem(&divisor);
        quotient.negative = (self.negative != rhs.negative) && !quotient.is_zero();
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
            let (_, remainder) = a.abs_div_rem(&b);
            a = b;
            b = remainder;
        }
        a.negative = false;
        a
    }
}