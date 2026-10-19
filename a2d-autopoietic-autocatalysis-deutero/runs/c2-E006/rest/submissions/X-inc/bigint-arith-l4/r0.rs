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
            let value = (*limb as u64) * (factor as u64) + carry;
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
        let mut index = 0;
        while carry != 0 {
            if index == self.limbs.len() {
                self.limbs.push(0);
            }
            let sum = self.limbs[index] as u64 + carry;
            self.limbs[index] = (sum % Self::BASE) as u32;
            carry = sum / Self::BASE;
            index += 1;
        }
    }

    fn cmp_abs(&self, rhs: &Self) -> Ordering {
        match self.limbs.len().cmp(&rhs.limbs.len()) {
            Ordering::Equal => {
                for (a, b) in self.limbs.iter().rev().zip(rhs.limbs.iter().rev()) {
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

    fn add_abs(a: &Self, b: &Self) -> Self {
        let len = a.limbs.len().max(b.limbs.len());
        let mut limbs = Vec::with_capacity(len + 1);
        let mut carry = 0u64;
        for i in 0..len {
            let av = a.limbs.get(i).copied().unwrap_or(0) as u64;
            let bv = b.limbs.get(i).copied().unwrap_or(0) as u64;
            let sum = av + bv + carry;
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

    // Requires |a| >= |b|.
    fn sub_abs(a: &Self, b: &Self) -> Self {
        let mut limbs = Vec::with_capacity(a.limbs.len());
        let mut borrow = 0i64;
        for i in 0..a.limbs.len() {
            let av = a.limbs[i] as i64;
            let bv = b.limbs.get(i).copied().unwrap_or(0) as i64;
            let mut value = av - bv - borrow;
            if value < 0 {
                value += Self::BASE as i64;
                borrow = 1;
            } else {
                borrow = 0;
            }
            limbs.push(value as u32);
        }
        let mut result = Self {
            negative: false,
            limbs,
        };
        result.normalize();
        result
    }

    fn mul_abs(a: &Self, b: &Self) -> Self {
        if a.is_zero() || b.is_zero() {
            return Self::zero();
        }
        let mut limbs = vec![0u32; a.limbs.len() + b.limbs.len()];
        for (i, &av) in a.limbs.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &bv) in b.limbs.iter().enumerate() {
                let index = i + j;
                let value =
                    limbs[index] as u64 + (av as u64) * (bv as u64) + carry;
                limbs[index] = (value % Self::BASE) as u32;
                carry = value / Self::BASE;
            }
            let mut index = i + b.limbs.len();
            while carry != 0 {
                if index == limbs.len() {
                    limbs.push(0);
                }
                let value = limbs[index] as u64 + carry;
                limbs[index] = (value % Self::BASE) as u32;
                carry = value / Self::BASE;
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

    fn div_rem_abs(dividend: &Self, divisor: &Self) -> (Self, Self) {
        debug_assert!(!divisor.is_zero());
        if dividend.cmp_abs(divisor) == Ordering::Less {
            return (Self::zero(), dividend.abs());
        }

        let mut quotient_be = Vec::with_capacity(dividend.limbs.len());
        let mut remainder = Self::zero();

        for &limb in dividend.limbs.iter().rev() {
            remainder.limbs.insert(0, limb);
            remainder.normalize();

            let mut low = 0u64;
            let mut high = Self::BASE - 1;
            let mut best = 0u64;
            while low <= high {
                let mid = low + (high - low) / 2;
                let product = Self::mul_abs_small(divisor, mid as u32);
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
                let product = Self::mul_abs_small(divisor, best as u32);
                remainder = Self::sub_abs(&remainder, &product);
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

    fn mul_abs_small(value: &Self, factor: u32) -> Self {
        let mut result = value.abs();
        result.mul_small_assign(factor);
        result
    }

    fn div_rem_small_assign(&mut self, divisor: u32) -> u32 {
        let mut remainder = 0u64;
        for limb in self.limbs.iter_mut().rev() {
            let value = remainder * Self::BASE + *limb as u64;
            *limb = (value / divisor as u64) as u32;
            remainder = value % divisor as u64;
        }
        self.normalize();
        remainder as u32
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

        let mut first_invalid = None;
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte == b'_' {
                continue;
            }
            match Self::digit_value(byte) {
                Some(value) if value < radix => {}
                _ => {
                    first_invalid = Some(index);
                    break;
                }
            }
        }
        if let Some(index) = first_invalid {
            return Err(Error::InvalidDigit { index });
        }

        let mut first_bad_separator = None;
        for index in start..bytes.len() {
            if bytes[index] == b'_' {
                let before_valid = index > start
                    && bytes[index - 1] != b'_'
                    && Self::digit_value(bytes[index - 1])
                        .is_some_and(|d| d < radix);
                let after_valid = index + 1 < bytes.len()
                    && bytes[index + 1] != b'_'
                    && Self::digit_value(bytes[index + 1])
                        .is_some_and(|d| d < radix);
                if !(before_valid && after_valid) {
                    first_bad_separator = Some(index);
                    break;
                }
            }
        }
        if let Some(index) = first_bad_separator {
            return Err(Error::Separator { index });
        }

        let digit_count = bytes[start..].iter().filter(|&&b| b != b'_').count();
        let first_digit_index = (start..bytes.len()).find(|&i| bytes[i] != b'_').unwrap();
        if digit_count > 1 && Self::digit_value(bytes[first_digit_index]) == Some(0) {
            return Err(Error::LeadingZero {
                index: first_digit_index,
            });
        }

        if digit_count > max_digits {
            let excess_ordinal = max_digits;
            let mut seen = 0usize;
            for (index, &byte) in bytes.iter().enumerate().skip(start) {
                if byte != b'_' {
                    if seen == excess_ordinal {
                        return Err(Error::TooManyDigits {
                            limit: max_digits,
                            index,
                        });
                    }
                    seen += 1;
                }
            }
        }

        let mut value = Self::zero();
        for &byte in &bytes[start..] {
            if byte != b'_' {
                value.mul_small_assign(radix);
                value.add_small_assign(Self::digit_value(byte).unwrap());
            }
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

        let mut magnitude = self.abs();
        let mut digits = Vec::new();
        if magnitude.is_zero() {
            digits.push(0);
        } else {
            while !magnitude.is_zero() {
                digits.push(magnitude.div_rem_small_assign(radix));
            }
            digits.reverse();
        }

        if digits.len() < min_digits {
            let mut padded = vec![0; min_digits - digits.len()];
            padded.extend(digits);
            digits = padded;
        }

        let mut output = String::new();
        if self.negative && !self.is_zero() {
            output.push('-');
        }
        for (index, digit) in digits.iter().enumerate() {
            if group > 0 && index > 0 && (digits.len() - index) % group == 0 {
                output.push('_');
            }
            output.push(match *digit {
                0..=9 => (b'0' + *digit as u8) as char,
                _ => (b'a' + (*digit - 10) as u8) as char,
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
        if self.negative == rhs.negative {
            let mut result = Self::add_abs(self, rhs);
            result.negative = self.negative && !result.is_zero();
            result
        } else {
            match self.cmp_abs(rhs) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => {
                    let mut result = Self::sub_abs(self, rhs);
                    result.negative = self.negative;
                    result
                }
                Ordering::Less => {
                    let mut result = Self::sub_abs(rhs, self);
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
        let mut result = Self::mul_abs(self, rhs);
        result.negative = (self.negative != rhs.negative) && !result.is_zero();
        result
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }
        let (mut quotient, mut remainder) = Self::div_rem_abs(self, rhs);
        quotient.negative =
            (self.negative != rhs.negative) && !quotient.is_zero();
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
            let remainder = Self::div_rem_abs(&a, &b).1;
            a = b;
            b = remainder;
        }
        a.negative = false;
        a
    }
}