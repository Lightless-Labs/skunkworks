use std::cmp::Ordering;
use std::fmt;

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

#[derive(Clone, PartialEq, Eq)]
pub struct Big {
    negative: bool,
    // Little-endian base-1,000,000,000 limbs. Zero is an empty limb vector.
    limbs: Vec<u32>,
}

impl fmt::Debug for Big {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Big")
            .field("negative", &self.negative)
            .field("limbs", &self.limbs)
            .finish()
    }
}

impl Big {
    const LIMB_BASE: u64 = 1_000_000_000;
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
            out.push((sum % Self::LIMB_BASE) as u32);
            carry = sum / Self::LIMB_BASE;
        }
        if carry != 0 {
            out.push(carry as u32);
        }
        let mut result = Self {
            negative: false,
            limbs: out,
        };
        result.normalize();
        result
    }

    // Requires |self| >= |rhs|.
    fn sub_abs(&self, rhs: &Self) -> Self {
        let mut out = Vec::with_capacity(self.limbs.len());
        let mut borrow = 0i64;
        for i in 0..self.limbs.len() {
            let a = self.limbs[i] as i64;
            let b = rhs.limbs.get(i).copied().unwrap_or(0) as i64;
            let mut v = a - b - borrow;
            if v < 0 {
                v += Self::LIMB_BASE as i64;
                borrow = 1;
            } else {
                borrow = 0;
            }
            out.push(v as u32);
        }
        let mut result = Self {
            negative: false,
            limbs: out,
        };
        result.normalize();
        result
    }

    fn mul_small_abs(&mut self, m: u32) {
        if m == 0 || self.is_zero() {
            self.limbs.clear();
            self.negative = false;
            return;
        }
        let mut carry = 0u64;
        for limb in &mut self.limbs {
            let v = (*limb as u64) * (m as u64) + carry;
            *limb = (v % Self::LIMB_BASE) as u32;
            carry = v / Self::LIMB_BASE;
        }
        while carry != 0 {
            self.limbs.push((carry % Self::LIMB_BASE) as u32);
            carry /= Self::LIMB_BASE;
        }
    }

    fn add_small_abs(&mut self, n: u32) {
        let mut carry = n as u64;
        let mut i = 0;
        while carry != 0 {
            if i == self.limbs.len() {
                self.limbs.push(0);
            }
            let v = self.limbs[i] as u64 + carry;
            self.limbs[i] = (v % Self::LIMB_BASE) as u32;
            carry = v / Self::LIMB_BASE;
            i += 1;
        }
    }

    // Returns quotient and remainder for nonnegative magnitudes.
    fn div_rem_abs(&self, divisor: &Self) -> (Self, Self) {
        debug_assert!(!divisor.is_zero());
        if self.cmp_abs(divisor) == Ordering::Less {
            return (Self::zero(), self.abs());
        }

        let mut quotient_be = Vec::with_capacity(self.limbs.len());
        let mut rem = Self::zero();
        for &limb in self.limbs.iter().rev() {
            if !rem.is_zero() {
                rem.limbs.insert(0, limb);
            } else if limb != 0 {
                rem.limbs.push(limb);
            }
            rem.normalize();

            let mut lo = 0u64;
            let mut hi = Self::LIMB_BASE - 1;
            let mut best = 0u64;
            while lo <= hi {
                let mid = lo + (hi - lo) / 2;
                let mut product = divisor.abs();
                product.mul_small_abs(mid as u32);
                match product.cmp_abs(&rem) {
                    Ordering::Greater => {
                        if mid == 0 {
                            break;
                        }
                        hi = mid - 1;
                    }
                    _ => {
                        best = mid;
                        lo = mid + 1;
                    }
                }
            }
            if best != 0 {
                let mut product = divisor.abs();
                product.mul_small_abs(best as u32);
                rem = rem.sub_abs(&product);
            }
            quotient_be.push(best as u32);
        }
        quotient_be.reverse();
        let mut quotient = Self {
            negative: false,
            limbs: quotient_be,
        };
        quotient.normalize();
        (quotient, rem)
    }

    fn div_small_abs(&mut self, divisor: u32) -> u32 {
        let mut rem = 0u64;
        for limb in self.limbs.iter_mut().rev() {
            let current = rem * Self::LIMB_BASE + *limb as u64;
            *limb = (current / divisor as u64) as u32;
            rem = current % divisor as u64;
        }
        self.normalize();
        rem as u32
    }

    fn radix_digits(&self, radix: u32) -> Vec<u8> {
        if self.is_zero() {
            return vec![0];
        }
        let mut n = self.abs();
        let mut digits = Vec::new();
        while !n.is_zero() {
            digits.push(n.div_small_abs(radix) as u8);
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
        let mut digits: Vec<(usize, u32)> = Vec::new();
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte == b'_' {
                continue;
            }
            let value = match byte {
                b'0'..=b'9' => (byte - b'0') as u32,
                b'a'..=b'z' => (byte - b'a' + 10) as u32,
                b'A'..=b'Z' => (byte - b'A' + 10) as u32,
                _ => {
                    invalid.get_or_insert(index);
                    continue;
                }
            };
            if value >= radix {
                invalid.get_or_insert(index);
            } else {
                digits.push((index, value));
            }
        }
        if let Some(index) = invalid {
            return Err(Error::InvalidDigit { index });
        }

        let mut malformed_separator = None;
        for index in start..bytes.len() {
            if bytes[index] == b'_' {
                let prev_valid = index > start
                    && bytes[index - 1] != b'_'
                    && digit_value(bytes[index - 1]).is_some_and(|v| v < radix);
                let next_valid = index + 1 < bytes.len()
                    && bytes[index + 1] != b'_'
                    && digit_value(bytes[index + 1]).is_some_and(|v| v < radix);
                if !(prev_valid && next_valid) {
                    malformed_separator.get_or_insert(index);
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

        let mut value = Self::zero();
        for &(_, digit) in &digits {
            value.mul_small_abs(radix);
            value.add_small_abs(digit);
        }
        value.negative = negative && !value.is_zero();
        Ok(value)
    }

    pub fn format(&self, radix: u32, min_digits: usize, group: usize) -> Result<String, Error> {
        if !(2..=36).contains(&radix) {
            return Err(Error::InvalidRadix { radix });
        }
        let digits = self.radix_digits(radix);
        let padding = min_digits.saturating_sub(digits.len());
        let total = padding.saturating_add(digits.len());
        let mut out = String::new();
        if self.negative && !self.is_zero() {
            out.push('-');
        }
        for i in 0..total {
            if group > 0 && i > 0 && (total - i) % group == 0 {
                out.push('_');
            }
            let d = if i < padding { 0 } else { digits[i - padding] };
            out.push(if d < 10 {
                (b'0' + d) as char
            } else {
                (b'a' + (d - 10)) as char
            });
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
        if self.negative == rhs.negative {
            let mut out = self.add_abs(rhs);
            out.negative = self.negative && !out.is_zero();
            out
        } else {
            match self.cmp_abs(rhs) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => {
                    let mut out = self.sub_abs(rhs);
                    out.negative = self.negative;
                    out
                }
                Ordering::Less => {
                    let mut out = rhs.sub_abs(self);
                    out.negative = rhs.negative;
                    out
                }
            }
        }
    }

    pub fn sub(&self, rhs: &Self) -> Self {
        self.add(&rhs.neg())
    }

    pub fn neg(&self) -> Self {
        let mut out = self.clone();
        if !out.is_zero() {
            out.negative = !out.negative;
        }
        out
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
                let k = i + j;
                let value = out[k] + a as u64 * b as u64 + carry;
                out[k] = value % Self::LIMB_BASE;
                carry = value / Self::LIMB_BASE;
            }
            let mut k = i + rhs.limbs.len();
            while carry != 0 {
                if k == out.len() {
                    out.push(0);
                }
                let value = out[k] + carry;
                out[k] = value % Self::LIMB_BASE;
                carry = value / Self::LIMB_BASE;
                k += 1;
            }
        }
        let mut result = Self {
            negative: self.negative ^ rhs.negative,
            limbs: out.into_iter().map(|v| v as u32).collect(),
        };
        result.normalize();
        result
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }
        let (mut quotient, mut remainder) = self.abs().div_rem_abs(&rhs.abs());
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
        let mut e = exponent;
        let mut base = self.clone();
        let mut result = Self::one();
        while e != 0 {
            if e & 1 == 1 {
                result = result.mul(&base);
            }
            e >>= 1;
            if e != 0 {
                base = base.mul(&base);
            }
        }
        Ok(result)
    }

    pub fn gcd(&self, rhs: &Self) -> Self {
        let mut a = self.abs();
        let mut b = rhs.abs();
        while !b.is_zero() {
            let (_, r) = a.div_rem_abs(&b);
            a = b;
            b = r;
        }
        a
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