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
    // Little-endian base-10^9 limbs; zero is an empty limb vector.
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

    fn mul_small_assign(&mut self, factor: u32) {
        if factor == 0 || self.is_zero() {
            self.limbs.clear();
            self.negative = false;
            return;
        }
        let mut carry = 0u64;
        for limb in &mut self.limbs {
            let value = (*limb as u64) * factor as u64 + carry;
            *limb = (value % Self::BASE) as u32;
            carry = value / Self::BASE;
        }
        while carry > 0 {
            self.limbs.push((carry % Self::BASE) as u32);
            carry /= Self::BASE;
        }
    }

    fn add_small_assign(&mut self, value: u32) {
        let mut carry = value as u64;
        let mut i = 0;
        while carry > 0 {
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

    fn add_abs(a: &Self, b: &Self) -> Self {
        let n = a.limbs.len().max(b.limbs.len());
        let mut out = Vec::with_capacity(n + 1);
        let mut carry = 0u64;
        for i in 0..n {
            let x = a.limbs.get(i).copied().unwrap_or(0) as u64;
            let y = b.limbs.get(i).copied().unwrap_or(0) as u64;
            let sum = x + y + carry;
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

    // Requires |a| >= |b|.
    fn sub_abs(a: &Self, b: &Self) -> Self {
        let mut out = Vec::with_capacity(a.limbs.len());
        let mut borrow = 0i64;
        for i in 0..a.limbs.len() {
            let x = a.limbs[i] as i64;
            let y = b.limbs.get(i).copied().unwrap_or(0) as i64;
            let mut value = x - y - borrow;
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

    fn abs(&self) -> Self {
        let mut result = self.clone();
        result.negative = false;
        result
    }

    fn div_rem_abs(a: &Self, b: &Self) -> (Self, Self) {
        debug_assert!(!b.is_zero());
        if a.cmp_abs(b) == Ordering::Less {
            return (Self::zero(), a.abs());
        }

        let mut quotient_be = Vec::with_capacity(a.limbs.len());
        let mut remainder = Self::zero();

        for &limb in a.limbs.iter().rev() {
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
                let product = b.mul_small_abs(mid as u32);
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

            if best != 0 {
                let product = b.mul_small_abs(best as u32);
                remainder = Self::sub_abs(&remainder, &product);
            }
            quotient_be.push(best as u32);
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

    fn mul_small_abs(&self, factor: u32) -> Self {
        let mut out = self.clone();
        out.negative = false;
        out.mul_small_assign(factor);
        out
    }

    fn to_radix_digits(&self, radix: u32) -> Vec<u32> {
        if self.is_zero() {
            return vec![0];
        }
        let mut value = self.abs();
        let mut digits = Vec::new();
        while !value.is_zero() {
            let mut remainder = 0u64;
            for limb in value.limbs.iter_mut().rev() {
                let current = remainder * Self::BASE + *limb as u64;
                *limb = (current / radix as u64) as u32;
                remainder = current % radix as u64;
            }
            digits.push(remainder as u32);
            value.normalize();
        }
        digits.reverse();
        digits
    }

    pub fn parse(text: &str, radix: u32, max_digits: usize) -> Result<Self, Error> {
        if !(2..=36).contains(&radix) {
            return Err(Error::InvalidRadix { radix });
        }
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(Error::Empty);
        }

        let mut negative = false;
        let mut start = 0usize;
        if bytes[0] == b'+' || bytes[0] == b'-' {
            negative = bytes[0] == b'-';
            start = 1;
            if start == bytes.len() {
                return Err(Error::SignOnly);
            }
        }

        let mut invalid = None;
        for i in start..bytes.len() {
            if bytes[i] == b'_' {
                continue;
            }
            let value = match bytes[i] {
                b'0'..=b'9' => (bytes[i] - b'0') as u32,
                b'a'..=b'z' => (bytes[i] - b'a') as u32 + 10,
                b'A'..=b'Z' => (bytes[i] - b'A') as u32 + 10,
                _ => {
                    invalid = Some(i);
                    break;
                }
            };
            if value >= radix {
                invalid = Some(i);
                break;
            }
        }
        if let Some(index) = invalid {
            return Err(Error::InvalidDigit { index });
        }

        let mut first_separator = None;
        for i in start..bytes.len() {
            if bytes[i] == b'_' {
                let prev_valid = i > start && bytes[i - 1] != b'_';
                let next_valid = i + 1 < bytes.len() && bytes[i + 1] != b'_';
                if !prev_valid || !next_valid {
                    first_separator = Some(i);
                    break;
                }
            }
        }
        if let Some(index) = first_separator {
            return Err(Error::Separator { index });
        }

        let mut digit_count = 0usize;
        let mut first_digit = None;
        let mut first_value = 0u32;
        for i in start..bytes.len() {
            if bytes[i] == b'_' {
                continue;
            }
            let value = match bytes[i] {
                b'0'..=b'9' => (bytes[i] - b'0') as u32,
                b'a'..=b'z' => (bytes[i] - b'a') as u32 + 10,
                b'A'..=b'Z' => (bytes[i] - b'A') as u32 + 10,
                _ => unreachable!(),
            };
            if first_digit.is_none() {
                first_digit = Some(i);
                first_value = value;
            }
            digit_count += 1;
        }

        if digit_count > 1 && first_value == 0 {
            return Err(Error::LeadingZero {
                index: first_digit.unwrap(),
            });
        }
        if digit_count > max_digits {
            let target = max_digits.saturating_add(1);
            let mut seen = 0usize;
            let mut index = start;
            for i in start..bytes.len() {
                if bytes[i] != b'_' {
                    seen += 1;
                    if seen == target {
                        index = i;
                        break;
                    }
                }
            }
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index,
            });
        }

        let mut result = Self::zero();
        for &byte in &bytes[start..] {
            if byte == b'_' {
                continue;
            }
            let digit = match byte {
                b'0'..=b'9' => (byte - b'0') as u32,
                b'a'..=b'z' => (byte - b'a') as u32 + 10,
                b'A'..=b'Z' => (byte - b'A') as u32 + 10,
                _ => unreachable!(),
            };
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
        let mut digits = self.to_radix_digits(radix);
        if digits.len() < min_digits {
            let padding = min_digits - digits.len();
            let mut padded = Vec::with_capacity(min_digits);
            padded.resize(padding, 0);
            padded.extend(digits);
            digits = padded;
        }

        let mut magnitude = String::new();
        for (i, digit) in digits.iter().enumerate() {
            if group > 0 && i > 0 && (digits.len() - i) % group == 0 {
                magnitude.push('_');
            }
            let ch = if *digit < 10 {
                (b'0' + *digit as u8) as char
            } else {
                (b'a' + (*digit - 10) as u8) as char
            };
            magnitude.push(ch);
        }

        if self.negative && !self.is_zero() {
            Ok(format!("-{magnitude}"))
        } else {
            Ok(magnitude)
        }
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
        if self.is_zero() || rhs.is_zero() {
            return Self::zero();
        }
        let mut out = vec![0u64; self.limbs.len() + rhs.limbs.len()];
        for (i, &a) in self.limbs.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &b) in rhs.limbs.iter().enumerate() {
                let index = i + j;
                let value = out[index] + a as u64 * b as u64 + carry;
                out[index] = value % Self::BASE;
                carry = value / Self::BASE;
            }
            let mut index = i + rhs.limbs.len();
            while carry > 0 {
                if index == out.len() {
                    out.push(0);
                }
                let value = out[index] + carry;
                out[index] = value % Self::BASE;
                carry = value / Self::BASE;
                index += 1;
            }
        }
        let mut result = Self {
            negative: self.negative ^ rhs.negative,
            limbs: out.into_iter().map(|x| x as u32).collect(),
        };
        result.normalize();
        result
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }
        let (mut quotient, mut remainder) = Self::div_rem_abs(self, rhs);
        quotient.negative = (self.negative ^ rhs.negative) && !quotient.is_zero();
        remainder.negative = self.negative && !remainder.is_zero();
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
            let (_, remainder) = a.div_rem(&b).expect("nonzero divisor");
            a = b;
            b = remainder.abs();
        }
        a.negative = false;
        a
    }
}

fn main() {}