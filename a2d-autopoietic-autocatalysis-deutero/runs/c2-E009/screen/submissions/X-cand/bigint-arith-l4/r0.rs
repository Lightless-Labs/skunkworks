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

const BASE: u64 = 1_000_000_000;
const POW_LIMIT: u32 = 1024;

impl Big {
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

    fn abs(&self) -> Self {
        let mut result = self.clone();
        result.negative = false;
        result
    }

    fn digit_value(byte: u8) -> Option<u32> {
        match byte {
            b'0'..=b'9' => Some((byte - b'0') as u32),
            b'a'..=b'z' => Some((byte - b'a') as u32 + 10),
            b'A'..=b'Z' => Some((byte - b'A') as u32 + 10),
            _ => None,
        }
    }

    fn mul_small_abs(&mut self, multiplier: u32) {
        if multiplier == 0 || self.is_zero() {
            self.limbs.clear();
            self.negative = false;
            return;
        }
        let mut carry = 0u64;
        for limb in &mut self.limbs {
            let value = (*limb as u64) * (multiplier as u64) + carry;
            *limb = (value % BASE) as u32;
            carry = value / BASE;
        }
        while carry != 0 {
            self.limbs.push((carry % BASE) as u32);
            carry /= BASE;
        }
    }

    fn add_small_abs(&mut self, addend: u32) {
        let mut carry = addend as u64;
        let mut i = 0;
        while carry != 0 {
            if i == self.limbs.len() {
                self.limbs.push(0);
            }
            let value = self.limbs[i] as u64 + carry;
            self.limbs[i] = (value % BASE) as u32;
            carry = value / BASE;
            i += 1;
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

    // Requires |self| >= |rhs| and both values nonnegative.
    fn sub_abs_assign(&mut self, rhs: &Self) {
        let mut borrow = 0i64;
        for i in 0..self.limbs.len() {
            let right = rhs.limbs.get(i).copied().unwrap_or(0) as i64;
            let mut value = self.limbs[i] as i64 - right - borrow;
            if value < 0 {
                value += BASE as i64;
                borrow = 1;
            } else {
                borrow = 0;
            }
            self.limbs[i] = value as u32;
        }
        self.normalize();
    }

    fn add_abs(lhs: &Self, rhs: &Self) -> Self {
        let mut result = Self {
            negative: false,
            limbs: Vec::new(),
        };
        let count = lhs.limbs.len().max(rhs.limbs.len());
        let mut carry = 0u64;
        for i in 0..count {
            let value = lhs.limbs.get(i).copied().unwrap_or(0) as u64
                + rhs.limbs.get(i).copied().unwrap_or(0) as u64
                + carry;
            result.limbs.push((value % BASE) as u32);
            carry = value / BASE;
        }
        if carry != 0 {
            result.limbs.push(carry as u32);
        }
        result
    }

    fn mul_abs(lhs: &Self, rhs: &Self) -> Self {
        if lhs.is_zero() || rhs.is_zero() {
            return Self::zero();
        }
        let mut result = Self {
            negative: false,
            limbs: vec![0; lhs.limbs.len() + rhs.limbs.len()],
        };
        for (i, &a) in lhs.limbs.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &b) in rhs.limbs.iter().enumerate() {
                let k = i + j;
                let value = result.limbs[k] as u64 + (a as u64) * (b as u64) + carry;
                result.limbs[k] = (value % BASE) as u32;
                carry = value / BASE;
            }
            let mut k = i + rhs.limbs.len();
            while carry != 0 {
                if k == result.limbs.len() {
                    result.limbs.push(0);
                }
                let value = result.limbs[k] as u64 + carry;
                result.limbs[k] = (value % BASE) as u32;
                carry = value / BASE;
                k += 1;
            }
        }
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
            if !remainder.is_zero() {
                remainder.limbs.insert(0, limb);
            } else if limb != 0 {
                remainder.limbs.push(limb);
            }
            remainder.normalize();

            let mut low = 0u64;
            let mut high = BASE - 1;
            let mut best = 0u64;
            while low <= high {
                let mid = low + (high - low) / 2;
                let product = divisor.mul_small_abs_copy(mid as u32);
                if product.cmp_abs(&remainder) != Ordering::Greater {
                    best = mid;
                    low = mid + 1;
                } else {
                    if mid == 0 {
                        break;
                    }
                    high = mid - 1;
                }
            }
            quotient_be.push(best as u32);
            if best != 0 {
                let product = divisor.mul_small_abs_copy(best as u32);
                remainder.sub_abs_assign(&product);
            }
        }

        let mut quotient = Self {
            negative: false,
            limbs: quotient_be.into_iter().rev().collect(),
        };
        quotient.normalize();
        remainder.normalize();
        (quotient, remainder)
    }

    fn mul_small_abs_copy(&self, multiplier: u32) -> Self {
        let mut result = self.abs();
        result.mul_small_abs(multiplier);
        result
    }

    fn from_decimal_parts(negative: bool, digits: &[u8], radix: u32) -> Self {
        let mut result = Self::zero();
        for &digit in digits {
            result.mul_small_abs(radix);
            result.add_small_abs(digit as u32);
        }
        result.negative = negative && !result.is_zero();
        result
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
            if byte != b'_' && Self::digit_value(byte).is_none_or(|digit| digit >= radix) {
                invalid = Some(index);
                break;
            }
        }
        if let Some(index) = invalid {
            return Err(Error::InvalidDigit { index });
        }

        let mut separator = None;
        for index in start..bytes.len() {
            if bytes[index] == b'_' {
                let before_valid = index > start
                    && bytes[index - 1] != b'_'
                    && Self::digit_value(bytes[index - 1]).is_some_and(|d| d < radix);
                let after_valid = index + 1 < bytes.len()
                    && bytes[index + 1] != b'_'
                    && Self::digit_value(bytes[index + 1]).is_some_and(|d| d < radix);
                if !before_valid || !after_valid {
                    separator = Some(index);
                    break;
                }
            }
        }
        if let Some(index) = separator {
            return Err(Error::Separator { index });
        }

        let digit_count = bytes[start..].iter().filter(|&&byte| byte != b'_').count();
        let first_digit_index = (start..bytes.len()).find(|&index| bytes[index] != b'_').unwrap();
        let first_digit = Self::digit_value(bytes[first_digit_index]).unwrap();
        if digit_count > 1 && first_digit == 0 {
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
            return Err(Error::TooManyDigits { limit: max_digits, index });
        }

        let digits: Vec<u8> = bytes[start..]
            .iter()
            .filter_map(|&byte| {
                if byte == b'_' {
                    None
                } else {
                    Self::digit_value(byte)
                        .map(|digit| Some(digit as u8))
                        .unwrap_or(None)
                }
            })
            .collect();
        Ok(Self::from_decimal_parts(negative, &digits, radix))
    }

    pub fn format(&self, radix: u32, min_digits: usize, group: usize) -> Result<String, Error> {
        if !(2..=36).contains(&radix) {
            return Err(Error::InvalidRadix { radix });
        }
        let mut value = self.abs();
        let mut digits = Vec::new();
        if value.is_zero() {
            digits.push(0u8);
        } else {
            while !value.is_zero() {
                let (quotient, remainder) = value.div_rem_small(radix);
                digits.push(remainder as u8);
                value = quotient;
            }
            digits.reverse();
        }

        if digits.len() < min_digits {
            let padding = min_digits - digits.len();
            let mut padded = Vec::new();
            padded.resize(padding, 0);
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
                0..=9 => (b'0' + *digit) as char,
                _ => (b'a' + (*digit - 10)) as char,
            });
        }
        Ok(output)
    }

    fn div_rem_small(&self, divisor: u32) -> (Self, u32) {
        let mut quotient = Self::zero();
        quotient.limbs.resize(self.limbs.len(), 0);
        let mut remainder = 0u64;
        for i in (0..self.limbs.len()).rev() {
            let value = remainder * BASE + self.limbs[i] as u64;
            quotient.limbs[i] = (value / divisor as u64) as u32;
            remainder = value % divisor as u64;
        }
        quotient.normalize();
        (quotient, remainder as u32)
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
            result.negative = self.negative;
            result.normalize();
            result
        } else {
            match self.cmp_abs(rhs) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => {
                    let mut result = self.abs();
                    result.sub_abs_assign(&rhs.abs());
                    result.negative = self.negative;
                    result
                }
                Ordering::Less => {
                    let mut result = rhs.abs();
                    result.sub_abs_assign(&self.abs());
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
        result.negative = !result.is_zero() && self.negative != rhs.negative;
        result
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }
        let (mut quotient, mut remainder) = Self::div_rem_abs(&self.abs(), &rhs.abs());
        quotient.negative = !quotient.is_zero() && self.negative != rhs.negative;
        remainder.negative = !remainder.is_zero() && self.negative;
        Ok((quotient, remainder))
    }

    pub fn pow(&self, exponent: u32) -> Result<Self, Error> {
        if exponent > POW_LIMIT {
            return Err(Error::ExponentTooLarge {
                exponent,
                limit: POW_LIMIT,
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
            let (_, remainder) = a.div_rem(&b).expect("nonzero divisor");
            a = b;
            b = remainder.abs();
        }
        a
    }
}