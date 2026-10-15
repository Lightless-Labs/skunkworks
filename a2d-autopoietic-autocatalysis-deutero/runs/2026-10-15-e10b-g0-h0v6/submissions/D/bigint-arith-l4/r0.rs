use std::cmp::Ordering;
use std::fmt;

#[derive(Clone, Debug)]
pub struct Big {
    negative: bool,
    // Little-endian base 10^9 limbs. Zero is represented by an empty vector.
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

impl PartialEq for Big {
    fn eq(&self, other: &Self) -> bool {
        self.negative == other.negative && self.limbs == other.limbs
    }
}

impl Eq for Big {}

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

    fn from_u32(n: u32) -> Self {
        if n == 0 {
            Self::zero()
        } else {
            Self {
                negative: false,
                limbs: vec![n],
            }
        }
    }

    fn abs(&self) -> Self {
        let mut n = self.clone();
        n.negative = false;
        n
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
            *self = Self::zero();
            return;
        }
        let mut carry = 0u64;
        for limb in &mut self.limbs {
            let value = (*limb as u64) * multiplier as u64 + carry;
            *limb = (value % BASE) as u32;
            carry = value / BASE;
        }
        while carry > 0 {
            self.limbs.push((carry % BASE) as u32);
            carry /= BASE;
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
            self.limbs[i] = (sum % BASE) as u32;
            carry = sum / BASE;
            i += 1;
        }
    }

    fn cmp_abs(&self, other: &Self) -> Ordering {
        match self.limbs.len().cmp(&other.limbs.len()) {
            Ordering::Equal => {
                for (a, b) in self.limbs.iter().rev().zip(other.limbs.iter().rev()) {
                    match a.cmp(b) {
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
        let mut out = Vec::with_capacity(a.limbs.len().max(b.limbs.len()) + 1);
        let mut carry = 0u64;
        for i in 0..a.limbs.len().max(b.limbs.len()) {
            let sum = a.limbs.get(i).copied().unwrap_or(0) as u64
                + b.limbs.get(i).copied().unwrap_or(0) as u64
                + carry;
            out.push((sum % BASE) as u32);
            carry = sum / BASE;
        }
        if carry > 0 {
            out.push(carry as u32);
        }
        let mut result = Self {
            negative: false,
            limbs: out,
        };
        result.normalize();
        result
    }

    // Requires |a| >= |b|.
    fn sub_abs(a: &Self, b: &Self) -> Self {
        let mut out = Vec::with_capacity(a.limbs.len());
        let mut borrow = 0i64;
        for i in 0..a.limbs.len() {
            let mut value = a.limbs[i] as i64
                - b.limbs.get(i).copied().unwrap_or(0) as i64
                - borrow;
            if value < 0 {
                value += BASE as i64;
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

    fn div_rem_small_assign(&mut self, divisor: u32) -> u32 {
        let mut remainder = 0u64;
        for limb in self.limbs.iter_mut().rev() {
            let value = remainder * BASE + *limb as u64;
            *limb = (value / divisor as u64) as u32;
            remainder = value % divisor as u64;
        }
        self.normalize();
        remainder as u32
    }

    fn to_radix_digits(&self, radix: u32) -> Vec<u32> {
        if self.is_zero() {
            return vec![0];
        }
        let mut value = self.abs();
        let mut digits = Vec::new();
        while !value.is_zero() {
            digits.push(value.div_rem_small_assign(radix));
        }
        digits.reverse();
        digits
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
            if Self::digit_value(byte).is_none_or(|digit| digit >= radix) {
                invalid = Some(index);
                break;
            }
        }
        if let Some(index) = invalid {
            return Err(Error::InvalidDigit { index });
        }

        let mut first_underscore = None;
        for i in start..bytes.len() {
            if bytes[i] == b'_'
                && (i == start
                    || i + 1 == bytes.len()
                    || bytes[i - 1] == b'_'
                    || bytes[i + 1] == b'_')
            {
                first_underscore = Some(i);
                break;
            }
        }
        if let Some(index) = first_underscore {
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
            let excess_number = max_digits;
            let index = bytes[start..]
                .iter()
                .enumerate()
                .filter(|(_, b)| **b != b'_')
                .nth(excess_number)
                .map(|(offset, _)| start + offset)
                .unwrap();
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
            let digit = Self::digit_value(byte).unwrap();
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

        let digits = self.to_radix_digits(radix);
        let width = min_digits.max(digits.len());
        let pad = width - digits.len();
        let mut chars = Vec::with_capacity(width + width / group.max(1) + 1);

        for _ in 0..pad {
            chars.push('0');
        }
        for digit in digits {
            chars.push(if digit < 10 {
                (b'0' + digit as u8) as char
            } else {
                (b'a' + (digit - 10) as u8) as char
            });
        }

        let mut magnitude = String::with_capacity(chars.len() + chars.len() / group.max(1));
        for (i, ch) in chars.iter().enumerate() {
            if group > 0 && i > 0 && (chars.len() - i) % group == 0 {
                magnitude.push('_');
            }
            magnitude.push(*ch);
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
                let value = out[index] + (a as u64) * (b as u64) + carry;
                out[index] = value % BASE;
                carry = value / BASE;
            }
            let mut index = i + rhs.limbs.len();
            while carry > 0 {
                if index == out.len() {
                    out.push(0);
                }
                let value = out[index] + carry;
                out[index] = value % BASE;
                carry = value / BASE;
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

    fn shift_base_add(&mut self, limb: u32) {
        if self.is_zero() {
            if limb > 0 {
                self.limbs.push(limb);
            }
        } else {
            self.limbs.insert(0, limb);
        }
    }

    fn div_rem_abs(dividend: &Self, divisor: &Self) -> (Self, Self) {
        let mut quotient = vec![0u32; dividend.limbs.len()];
        let mut remainder = Self::zero();

        for i in (0..dividend.limbs.len()).rev() {
            remainder.shift_base_add(dividend.limbs[i]);

            let mut low = 0u64;
            let mut high = BASE - 1;
            let mut best = 0u64;
            while low <= high {
                let mid = (low + high) / 2;
                let product = divisor.mul(&Self {
                    negative: false,
                    limbs: if mid == 0 {
                        Vec::new()
                    } else {
                        let mut value = mid;
                        let mut limbs = Vec::new();
                        while value > 0 {
                            limbs.push((value % BASE) as u32);
                            value /= BASE;
                        }
                        limbs
                    },
                });
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
            quotient[i] = best as u32;
            if best > 0 {
                let product = divisor.mul(&Self {
                    negative: false,
                    limbs: {
                        let mut value = best;
                        let mut limbs = Vec::new();
                        while value > 0 {
                            limbs.push((value % BASE) as u32);
                            value /= BASE;
                        }
                        limbs
                    },
                });
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

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }
        if self.is_zero() {
            return Ok((Self::zero(), Self::zero()));
        }

        let (mut quotient, mut remainder) = Self::div_rem_abs(&self.abs(), &rhs.abs());
        quotient.negative = (self.negative ^ rhs.negative) && !quotient.is_zero();
        remainder.negative = self.negative && !remainder.is_zero();
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