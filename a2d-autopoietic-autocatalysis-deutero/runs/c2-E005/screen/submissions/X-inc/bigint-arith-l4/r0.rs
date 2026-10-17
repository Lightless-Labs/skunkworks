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
    // Little-endian base 1_000_000_000 limbs. Zero is represented by [].
    limbs: Vec<u32>,
}

impl Big {
    const BASE: u64 = 1_000_000_000;
    const POW_LIMIT: u32 = 1024;

    fn zero() -> Self {
        Self {
            negative: false,
            limbs: Vec::new(),
        }
    }

    fn one() -> Self {
        Self { negative: false, limbs: vec![1] }
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
        Self { negative: false, limbs: out }
    }

    // Requires |self| >= |rhs|.
    fn sub_abs(&self, rhs: &Self) -> Self {
        let mut out = Vec::with_capacity(self.limbs.len());
        let mut borrow = 0i64;
        for i in 0..self.limbs.len() {
            let a = self.limbs[i] as i64;
            let b = rhs.limbs.get(i).copied().unwrap_or(0) as i64;
            let mut value = a - b - borrow;
            if value < 0 {
                value += Self::BASE as i64;
                borrow = 1;
            } else {
                borrow = 0;
            }
            out.push(value as u32);
        }
        let mut result = Self { negative: false, limbs: out };
        result.normalize();
        result
    }

    fn mul_small_abs(&mut self, multiplier: u32) {
        if multiplier == 0 || self.is_zero() {
            self.limbs.clear();
            self.negative = false;
            return;
        }
        let mut carry = 0u64;
        for limb in &mut self.limbs {
            let value = (*limb as u64) * multiplier as u64 + carry;
            *limb = (value % Self::BASE) as u32;
            carry = value / Self::BASE;
        }
        while carry != 0 {
            self.limbs.push((carry % Self::BASE) as u32);
            carry /= Self::BASE;
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
            self.limbs[i] = (value % Self::BASE) as u32;
            carry = value / Self::BASE;
            i += 1;
        }
    }

    fn div_small_abs(&mut self, divisor: u32) -> u32 {
        let mut remainder = 0u64;
        for limb in self.limbs.iter_mut().rev() {
            let value = remainder * Self::BASE + *limb as u64;
            *limb = (value / divisor as u64) as u32;
            remainder = value % divisor as u64;
        }
        self.normalize();
        remainder as u32
    }

    fn mul_abs(&self, rhs: &Self) -> Self {
        if self.is_zero() || rhs.is_zero() {
            return Self::zero();
        }
        let mut out = vec![0u64; self.limbs.len() + rhs.limbs.len()];
        for (i, &a) in self.limbs.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &b) in rhs.limbs.iter().enumerate() {
                let k = i + j;
                let value = out[k] + a as u64 * b as u64 + carry;
                out[k] = value % Self::BASE;
                carry = value / Self::BASE;
            }
            let mut k = i + rhs.limbs.len();
            while carry != 0 {
                if k == out.len() {
                    out.push(0);
                }
                let value = out[k] + carry;
                out[k] = value % Self::BASE;
                carry = value / Self::BASE;
                k += 1;
            }
        }
        let mut result = Self {
            negative: false,
            limbs: out.into_iter().map(|x| x as u32).collect(),
        };
        result.normalize();
        result
    }

    fn div_rem_abs(dividend: &Self, divisor: &Self) -> (Self, Self) {
        debug_assert!(!divisor.is_zero());
        if dividend.cmp_abs(divisor) == Ordering::Less {
            return (Self::zero(), dividend.abs());
        }

        let mut quotient_be = Vec::with_capacity(dividend.limbs.len() * 2);
        let mut remainder = Self::zero();
        for &limb in dividend.limbs.iter().rev() {
            remainder.limbs.insert(0, limb);
            remainder.normalize();

            let mut low = 0u64;
            let mut high = Self::BASE - 1;
            let mut best = 0u64;
            while low <= high {
                let mid = (low + high) / 2;
                let product = divisor.mul_small_value(mid as u32);
                match product.cmp_abs(&remainder) {
                    Ordering::Greater => {
                        if mid == 0 { break; }
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
                let product = divisor.mul_small_value(best as u32);
                remainder = remainder.sub_abs(&product);
            }
        }

        quotient_be.reverse();
        let mut quotient = Self { negative: false, limbs: quotient_be };
        quotient.normalize();
        remainder.normalize();
        (quotient, remainder)
    }

    fn mul_small_value(&self, multiplier: u32) -> Self {
        let mut result = self.abs();
        result.mul_small_abs(multiplier);
        result
    }

    fn radix_digit(byte: u8) -> Option<u32> {
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
        let (negative, start) = match bytes[0] {
            b'-' => (true, 1),
            b'+' => (false, 1),
            _ => (false, 0),
        };
        if start == bytes.len() {
            return Err(Error::SignOnly);
        }

        let mut invalid = None;
        for (i, &byte) in bytes.iter().enumerate().skip(start) {
            if byte == b'_' {
                continue;
            }
            match Self::radix_digit(byte) {
                Some(digit) if digit < radix => {}
                _ => {
                    invalid = Some(i);
                    break;
                }
            }
        }
        if let Some(index) = invalid {
            return Err(Error::InvalidDigit { index });
        }

        let mut malformed_separator = None;
        for i in start..bytes.len() {
            if bytes[i] == b'_' {
                let previous_valid = i > start
                    && Self::radix_digit(bytes[i - 1]).is_some_and(|d| d < radix);
                let next_valid = i + 1 < bytes.len()
                    && Self::radix_digit(bytes[i + 1]).is_some_and(|d| d < radix);
                if !previous_valid || !next_valid {
                    malformed_separator = Some(i);
                    break;
                }
            }
        }
        if let Some(index) = malformed_separator {
            return Err(Error::Separator { index });
        }

        let digit_positions: Vec<(usize, u32)> = (start..bytes.len())
            .filter_map(|i| {
                if bytes[i] == b'_' {
                    None
                } else {
                    Some((i, Self::radix_digit(bytes[i]).unwrap()))
                }
            })
            .collect();

        if digit_positions.len() > 1 && digit_positions[0].1 == 0 {
            return Err(Error::LeadingZero { index: digit_positions[0].0 });
        }
        if digit_positions.len() > max_digits {
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: digit_positions[max_digits].0,
            });
        }

        let mut result = Self::zero();
        for &(_, digit) in &digit_positions {
            result.mul_small_abs(radix);
            result.add_small_abs(digit);
        }
        result.negative = negative && !result.is_zero();
        Ok(result)
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
                digits.push(value.div_small_abs(radix));
            }
            digits.reverse();
        }
        if digits.len() < min_digits {
            let mut padding = vec![0; min_digits - digits.len()];
            padding.extend(digits);
            digits = padding;
        }

        let mut output = String::new();
        if self.negative && !self.is_zero() {
            output.push('-');
        }
        for (i, digit) in digits.iter().enumerate() {
            if group > 0 && i > 0 && (digits.len() - i) % group == 0 {
                output.push('_');
            }
            output.push(match *digit {
                0..=9 => (b'0' + *digit as u8) as char,
                _ => (b'a' + (*digit as u8 - 10)) as char,
            });
        }
        Ok(output)
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.signum(), rhs.signum()) {
            (a, b) if a < b => Ordering::Less,
            (a, b) if a > b => Ordering::Greater,
            (0, 0) => Ordering::Equal,
            (1, 1) => self.cmp_abs(rhs),
            (-1, -1) => rhs.cmp_abs(self),
            _ => Ordering::Equal,
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        let mut result = if self.negative == rhs.negative {
            let mut sum = self.add_abs(rhs);
            sum.negative = self.negative;
            sum
        } else {
            match self.cmp_abs(rhs) {
                Ordering::Greater => {
                    let mut difference = self.sub_abs(rhs);
                    difference.negative = self.negative;
                    difference
                }
                Ordering::Less => {
                    let mut difference = rhs.sub_abs(self);
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
        let mut result = self.mul_abs(rhs);
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
        let mut result = Self::one();
        let mut base = self.clone();
        let mut power = exponent;
        while power != 0 {
            if power & 1 != 0 {
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