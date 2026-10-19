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
    // Little-endian base-10^9 limbs; zero is an empty vector and never negative.
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
        if self.limbs.is_empty() {
            self.negative = false;
        }
    }

    fn valid_radix(radix: u32) -> Result<(), Error> {
        if (2..=36).contains(&radix) {
            Ok(())
        } else {
            Err(Error::InvalidRadix { radix })
        }
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
        Self::valid_radix(radix)?;
        if text.is_empty() {
            return Err(Error::Empty);
        }

        let bytes = text.as_bytes();
        let (negative, start) = match bytes[0] {
            b'-' => (true, 1),
            b'+' => (false, 1),
            _ => (false, 0),
        };
        if start == bytes.len() {
            return Err(Error::SignOnly);
        }

        let magnitude = &bytes[start..];

        // Invalid digits outrank every separator error.
        for (offset, &byte) in magnitude.iter().enumerate() {
            if byte == b'_' {
                continue;
            }
            if !matches!(Self::digit_value(byte), Some(value) if value < radix) {
                return Err(Error::InvalidDigit {
                    index: start + offset,
                });
            }
        }

        // Separator errors outrank leading-zero and digit-limit errors.
        for (i, &byte) in magnitude.iter().enumerate() {
            if byte == b'_'
                && (i == 0
                    || i + 1 == magnitude.len()
                    || magnitude[i - 1] == b'_'
                    || magnitude[i + 1] == b'_')
            {
                return Err(Error::Separator { index: start + i });
            }
        }

        let digit_count = magnitude.iter().filter(|&&b| b != b'_').count();
        let first_digit_offset = magnitude.iter().position(|&b| b != b'_').unwrap();

        if digit_count > 1 && Self::digit_value(magnitude[first_digit_offset]) == Some(0) {
            return Err(Error::LeadingZero {
                index: start + first_digit_offset,
            });
        }

        if digit_count > max_digits {
            let mut seen = 0usize;
            for (i, &byte) in magnitude.iter().enumerate() {
                if byte != b'_' {
                    seen += 1;
                    if seen > max_digits {
                        return Err(Error::TooManyDigits {
                            limit: max_digits,
                            index: start + i,
                        });
                    }
                }
            }
        }

        let mut value = Self::zero();
        for &byte in magnitude {
            if byte == b'_' {
                continue;
            }
            value.mul_small_assign(radix);
            value.add_small_assign(Self::digit_value(byte).unwrap());
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
        Self::valid_radix(radix)?;

        let mut value = self.abs();
        let mut digits = Vec::new();
        if value.is_zero() {
            digits.push('0');
        } else {
            while !value.is_zero() {
                let remainder = value.div_small_assign(radix);
                digits.push(Self::digit_char(remainder));
            }
            digits.reverse();
        }

        if digits.len() < min_digits {
            let padding = min_digits - digits.len();
            let mut padded = Vec::with_capacity(min_digits);
            padded.resize(padding, '0');
            padded.extend(digits);
            digits = padded;
        }

        let mut result = String::new();
        if self.negative && !self.is_zero() {
            result.push('-');
        }
        for (i, digit) in digits.iter().copied().enumerate() {
            if group > 0 && i > 0 && (digits.len() - i) % group == 0 {
                result.push('_');
            }
            result.push(digit);
        }
        Ok(result)
    }

    fn digit_char(value: u32) -> char {
        match value {
            0..=9 => (b'0' + value as u8) as char,
            _ => (b'a' + (value - 10) as u8) as char,
        }
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.negative, rhs.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => Self::cmp_abs(self, rhs),
            (true, true) => Self::cmp_abs(rhs, self),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            let mut result = Self::add_abs(self, rhs);
            result.negative = self.negative && !result.is_zero();
            result
        } else {
            match Self::cmp_abs(self, rhs) {
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
        if self.is_zero() || rhs.is_zero() {
            return Self::zero();
        }
        let mut limbs = vec![0u32; self.limbs.len() + rhs.limbs.len()];
        for (i, &a) in self.limbs.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &b) in rhs.limbs.iter().enumerate() {
                let index = i + j;
                let total = limbs[index] as u64 + a as u64 * b as u64 + carry;
                limbs[index] = (total % Self::BASE) as u32;
                carry = total / Self::BASE;
            }
            let mut index = i + rhs.limbs.len();
            while carry != 0 {
                let total = limbs[index] as u64 + carry;
                limbs[index] = (total % Self::BASE) as u32;
                carry = total / Self::BASE;
                index += 1;
                if index == limbs.len() && carry != 0 {
                    limbs.push(0);
                }
            }
        }
        let mut result = Self {
            negative: self.negative ^ rhs.negative,
            limbs,
        };
        result.normalize();
        result
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }

        let (mut quotient, mut remainder) = Self::div_rem_abs(&self.abs(), &rhs.abs());
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

        let mut power = exponent;
        let mut base = self.clone();
        let mut result = Self::one();
        while power > 0 {
            if power & 1 == 1 {
                result = result.mul(&base);
            }
            power >>= 1;
            if power > 0 {
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
        a
    }

    fn abs(&self) -> Self {
        let mut result = self.clone();
        result.negative = false;
        result
    }

    fn cmp_abs(a: &Self, b: &Self) -> Ordering {
        match a.limbs.len().cmp(&b.limbs.len()) {
            Ordering::Equal => {
                for (x, y) in a.limbs.iter().rev().zip(b.limbs.iter().rev()) {
                    match x.cmp(y) {
                        Ordering::Equal => {}
                        ordering => return ordering,
                    }
                }
                Ordering::Equal
            }
            ordering => ordering,
        }
    }

    fn add_abs(a: &Self, b: &Self) -> Self {
        let len = a.limbs.len().max(b.limbs.len());
        let mut limbs = Vec::with_capacity(len + 1);
        let mut carry = 0u64;
        for i in 0..len {
            let x = a.limbs.get(i).copied().unwrap_or(0) as u64;
            let y = b.limbs.get(i).copied().unwrap_or(0) as u64;
            let sum = x + y + carry;
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
            let x = a.limbs[i] as i64 - borrow;
            let y = b.limbs.get(i).copied().unwrap_or(0) as i64;
            if x < y {
                limbs.push((x + Self::BASE as i64 - y) as u32);
                borrow = 1;
            } else {
                limbs.push((x - y) as u32);
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

    fn div_small_assign(&mut self, divisor: u32) -> u32 {
        let mut remainder = 0u64;
        for limb in self.limbs.iter_mut().rev() {
            let current = remainder * Self::BASE + *limb as u64;
            *limb = (current / divisor as u64) as u32;
            remainder = current % divisor as u64;
        }
        self.normalize();
        remainder as u32
    }

    fn shift_base_add(&mut self, limb: u32) {
        if self.is_zero() {
            if limb != 0 {
                self.limbs.push(limb);
            }
        } else {
            self.limbs.insert(0, limb);
            self.normalize();
        }
    }

    fn div_rem_abs(dividend: &Self, divisor: &Self) -> (Self, Self) {
        debug_assert!(!divisor.is_zero());
        if Self::cmp_abs(dividend, divisor) == Ordering::Less {
            return (Self::zero(), dividend.abs());
        }

        let mut quotient = vec![0u32; dividend.limbs.len()];
        let mut remainder = Self::zero();

        for i in (0..dividend.limbs.len()).rev() {
            remainder.shift_base_add(dividend.limbs[i]);

            let mut low = 0u64;
            let mut high = Self::BASE - 1;
            let mut best = 0u64;
            while low <= high {
                let mid = (low + high) / 2;
                let product = Self::mul_abs_small(divisor, mid as u32);
                match Self::cmp_abs(&product, &remainder) {
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
            quotient[i] = best as u32;
            if best != 0 {
                let product = Self::mul_abs_small(divisor, best as u32);
                remainder = Self::sub_abs(&remainder, &product);
            }
        }

        let mut q = Self {
            negative: false,
            limbs: quotient,
        };
        q.normalize();
        remainder.normalize();
        (q, remainder)
    }

    fn mul_abs_small(value: &Self, factor: u32) -> Self {
        if factor == 0 || value.is_zero() {
            return Self::zero();
        }
        let mut limbs = Vec::with_capacity(value.limbs.len() + 1);
        let mut carry = 0u64;
        for &limb in &value.limbs {
            let product = limb as u64 * factor as u64 + carry;
            limbs.push((product % Self::BASE) as u32);
            carry = product / Self::BASE;
        }
        if carry != 0 {
            limbs.push(carry as u32);
        }
        Self {
            negative: false,
            limbs,
        }
    }
}