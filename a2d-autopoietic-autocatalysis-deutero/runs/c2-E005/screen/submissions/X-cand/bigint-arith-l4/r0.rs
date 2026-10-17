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
        Self { negative: false, limbs: vec![0] }
    }

    fn one() -> Self {
        Self { negative: false, limbs: vec![1] }
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

    fn mul_small_add(&mut self, multiplier: u32, addend: u32) {
        let mut carry = addend as u64;
        for limb in &mut self.limbs {
            let value = (*limb as u64) * multiplier as u64 + carry;
            *limb = (value % Self::BASE) as u32;
            carry = value / Self::BASE;
        }
        while carry != 0 {
            self.limbs.push((carry % Self::BASE) as u32);
            carry /= Self::BASE;
        }
        self.normalize();
    }

    pub fn parse(text: &str, radix: u32, max_digits: usize) -> Result<Self, Error> {
        if !(2..=36).contains(&radix) {
            return Err(Error::InvalidRadix { radix });
        }
        if text.is_empty() {
            return Err(Error::Empty);
        }

        let bytes = text.as_bytes();
        let mut start = 0;
        let mut negative = false;
        if bytes[0] == b'+' || bytes[0] == b'-' {
            negative = bytes[0] == b'-';
            start = 1;
            if start == bytes.len() {
                return Err(Error::SignOnly);
            }
        }

        let mut invalid = None;
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' && !Self::digit_value(byte).is_some_and(|d| d < radix) {
                invalid = Some(index);
                break;
            }
        }
        if let Some(index) = invalid {
            return Err(Error::InvalidDigit { index });
        }

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
            .filter(|&index| bytes[index] != b'_')
            .collect();

        if digit_indices.len() > 1 && Self::digit_value(bytes[digit_indices[0]]) == Some(0) {
            return Err(Error::LeadingZero { index: digit_indices[0] });
        }

        if digit_indices.len() > max_digits {
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: digit_indices[max_digits],
            });
        }

        let mut result = Self::zero();
        for index in digit_indices {
            result.mul_small_add(radix, Self::digit_value(bytes[index]).unwrap());
        }
        result.negative = negative && !result.is_zero();
        Ok(result)
    }

    pub fn format(&self, radix: u32, min_digits: usize, group: usize) -> Result<String, Error> {
        if !(2..=36).contains(&radix) {
            return Err(Error::InvalidRadix { radix });
        }

        let mut n = self.abs();
        let mut digits = Vec::new();
        if n.is_zero() {
            digits.push(0u8);
        } else {
            while !n.is_zero() {
                let remainder = n.div_small(radix);
                digits.push(remainder as u8);
            }
        }

        while digits.len() < min_digits {
            digits.push(0);
        }

        let mut magnitude = String::new();
        for (i, &digit) in digits.iter().rev().enumerate() {
            if group > 0 && i > 0 && (digits.len() - i) % group == 0 {
                magnitude.push('_');
            }
            magnitude.push(if digit < 10 {
                (b'0' + digit) as char
            } else {
                (b'a' + digit - 10) as char
            });
        }

        if self.negative && !self.is_zero() {
            Ok(format!("-{magnitude}"))
        } else {
            Ok(magnitude)
        }
    }

    fn div_small(&mut self, divisor: u32) -> u32 {
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
        self.limbs.len().cmp(&rhs.limbs.len()).then_with(|| {
            self.limbs.iter().rev().cmp(rhs.limbs.iter().rev())
        })
    }

    fn add_abs(a: &Self, b: &Self) -> Self {
        let len = a.limbs.len().max(b.limbs.len());
        let mut result = Vec::with_capacity(len + 1);
        let mut carry = 0u64;
        for i in 0..len {
            let sum = a.limbs.get(i).copied().unwrap_or(0) as u64
                + b.limbs.get(i).copied().unwrap_or(0) as u64
                + carry;
            result.push((sum % Self::BASE) as u32);
            carry = sum / Self::BASE;
        }
        if carry != 0 {
            result.push(carry as u32);
        }
        let mut value = Self { negative: false, limbs: result };
        value.normalize();
        value
    }

    // Requires |a| >= |b|.
    fn sub_abs(a: &Self, b: &Self) -> Self {
        let mut result = Vec::with_capacity(a.limbs.len());
        let mut borrow = 0i64;
        for i in 0..a.limbs.len() {
            let mut value = a.limbs[i] as i64
                - b.limbs.get(i).copied().unwrap_or(0) as i64
                - borrow;
            if value < 0 {
                value += Self::BASE as i64;
                borrow = 1;
            } else {
                borrow = 0;
            }
            result.push(value as u32);
        }
        let mut value = Self { negative: false, limbs: result };
        value.normalize();
        value
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
        if self.is_zero() { 0 } else if self.negative { -1 } else { 1 }
    }

    pub fn mul(&self, rhs: &Self) -> Self {
        if self.is_zero() || rhs.is_zero() {
            return Self::zero();
        }
        let mut result = vec![0u64; self.limbs.len() + rhs.limbs.len()];
        for (i, &a) in self.limbs.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &b) in rhs.limbs.iter().enumerate() {
                let index = i + j;
                let value = result[index] + a as u64 * b as u64 + carry;
                result[index] = value % Self::BASE;
                carry = value / Self::BASE;
            }
            let mut index = i + rhs.limbs.len();
            while carry != 0 {
                if index == result.len() {
                    result.push(0);
                }
                let value = result[index] + carry;
                result[index] = value % Self::BASE;
                carry = value / Self::BASE;
                index += 1;
            }
        }
        let mut value = Self {
            negative: self.negative != rhs.negative,
            limbs: result.into_iter().map(|x| x as u32).collect(),
        };
        value.normalize();
        value
    }

    fn mul_abs_small(&self, multiplier: u32) -> Self {
        let mut result = self.abs();
        result.mul_small_add(multiplier, 0);
        result
    }

    fn div_rem_abs(dividend: &Self, divisor: &Self) -> (Self, Self) {
        if dividend.cmp_abs(divisor) == Ordering::Less {
            return (Self::zero(), dividend.abs());
        }

        let mut quotient_be = Vec::with_capacity(dividend.limbs.len());
        let mut remainder = Self::zero();

        for &limb in dividend.limbs.iter().rev() {
            remainder.limbs.insert(0, limb);
            remainder.normalize();

            let mut low = 0u32;
            let mut high = (Self::BASE - 1) as u32;
            let mut best = 0u32;
            while low <= high {
                let mid = low + (high - low) / 2;
                let product = divisor.mul_abs_small(mid);
                match product.cmp_abs(&remainder) {
                    Ordering::Greater => {
                        if mid == 0 { break; }
                        high = mid - 1;
                    }
                    _ => {
                        best = mid;
                        if mid == u32::MAX { break; }
                        low = mid + 1;
                    }
                }
            }

            let product = divisor.mul_abs_small(best);
            remainder = Self::sub_abs(&remainder, &product);
            quotient_be.push(best);
        }

        quotient_be.reverse();
        let mut quotient = Self { negative: false, limbs: quotient_be };
        quotient.normalize();
        remainder.normalize();
        (quotient, remainder)
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }
        let (mut quotient, mut remainder) = Self::div_rem_abs(&self.abs(), &rhs.abs());
        quotient.negative = (self.negative != rhs.negative) && !quotient.is_zero();
        remainder.negative = self.negative && !remainder.is_zero();
        Ok((quotient, remainder))
    }

    pub fn pow(&self, exponent: u32) -> Result<Self, Error> {
        const LIMIT: u32 = 1024;
        if exponent > LIMIT {
            return Err(Error::ExponentTooLarge { exponent, limit: LIMIT });
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
            let remainder = a.div_rem(&b).unwrap().1.abs();
            a = b;
            b = remainder;
        }
        a
    }
}