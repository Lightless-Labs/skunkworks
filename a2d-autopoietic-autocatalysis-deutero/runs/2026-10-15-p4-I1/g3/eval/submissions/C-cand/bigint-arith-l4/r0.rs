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
        Self { negative: false, limbs: vec![1] }
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

    fn abs_cmp(&self, rhs: &Self) -> Ordering {
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

    fn add_abs(a: &[u32], b: &[u32]) -> Vec<u32> {
        let n = a.len().max(b.len());
        let mut out = Vec::with_capacity(n.saturating_add(1));
        let mut carry = 0u64;
        for i in 0..n {
            let x = a.get(i).copied().unwrap_or(0) as u64;
            let y = b.get(i).copied().unwrap_or(0) as u64;
            let sum = x + y + carry;
            out.push((sum % Self::BASE) as u32);
            carry = sum / Self::BASE;
        }
        if carry != 0 {
            out.push(carry as u32);
        }
        out
    }

    // Requires a >= b.
    fn sub_abs(a: &[u32], b: &[u32]) -> Vec<u32> {
        let mut out = Vec::with_capacity(a.len());
        let mut borrow = 0i64;
        for i in 0..a.len() {
            let x = a[i] as i64 - borrow;
            let y = b.get(i).copied().unwrap_or(0) as i64;
            if x < y {
                out.push((x + Self::BASE as i64 - y) as u32);
                borrow = 1;
            } else {
                out.push((x - y) as u32);
                borrow = 0;
            }
        }
        while out.last() == Some(&0) {
            out.pop();
        }
        out
    }

    fn mul_small_abs(a: &[u32], m: u32) -> Vec<u32> {
        if m == 0 || a.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(a.len().saturating_add(1));
        let mut carry = 0u64;
        for &limb in a {
            let product = limb as u64 * m as u64 + carry;
            out.push((product % Self::BASE) as u32);
            carry = product / Self::BASE;
        }
        if carry != 0 {
            out.push(carry as u32);
        }
        out
    }

    fn add_small_abs(a: &[u32], n: u32) -> Vec<u32> {
        let mut out = a.to_vec();
        let mut carry = n as u64;
        let mut i = 0;
        while carry != 0 {
            if i == out.len() {
                out.push(0);
            }
            let sum = out[i] as u64 + carry;
            out[i] = (sum % Self::BASE) as u32;
            carry = sum / Self::BASE;
            i += 1;
        }
        out
    }

    fn div_small_abs(a: &[u32], divisor: u32) -> (Vec<u32>, u32) {
        let mut q = vec![0; a.len()];
        let mut rem = 0u64;
        for i in (0..a.len()).rev() {
            let cur = rem * Self::BASE + a[i] as u64;
            q[i] = (cur / divisor as u64) as u32;
            rem = cur % divisor as u64;
        }
        while q.last() == Some(&0) {
            q.pop();
        }
        (q, rem as u32)
    }

    fn abs_div_rem(a: &Self, b: &Self) -> (Self, Self) {
        debug_assert!(!b.is_zero());
        match a.abs_cmp(b) {
            Ordering::Less => return (Self::zero(), Self { negative: false, limbs: a.limbs.clone() }),
            Ordering::Equal => return (Self::one(), Self::zero()),
            Ordering::Greater => {}
        }

        let mut quotient_be = Vec::with_capacity(a.limbs.len());
        let mut remainder = Vec::<u32>::new();

        for &digit in a.limbs.iter().rev() {
            remainder.insert(0, digit);
            while remainder.last() == Some(&0) {
                remainder.pop();
            }

            let mut low = 0u32;
            let mut high = (Self::BASE - 1) as u32;
            let mut best = 0u32;
            while low <= high {
                let mid = low + (high - low) / 2;
                let product = Self::mul_small_abs(&b.limbs, mid);
                match Self::cmp_abs_slices(&product, &remainder) {
                    Ordering::Greater => {
                        if mid == 0 {
                            break;
                        }
                        high = mid - 1;
                    }
                    _ => {
                        best = mid;
                        if mid == u32::MAX {
                            break;
                        }
                        low = mid + 1;
                    }
                }
            }

            if best != 0 {
                let product = Self::mul_small_abs(&b.limbs, best);
                remainder = Self::sub_abs(&remainder, &product);
            }
            quotient_be.push(best);
        }

        quotient_be.reverse();
        while quotient_be.last() == Some(&0) {
            quotient_be.pop();
        }
        while remainder.last() == Some(&0) {
            remainder.pop();
        }

        (
            Self { negative: false, limbs: quotient_be },
            Self { negative: false, limbs: remainder },
        )
    }

    fn cmp_abs_slices(a: &[u32], b: &[u32]) -> Ordering {
        let mut al = a.len();
        while al > 0 && a[al - 1] == 0 {
            al -= 1;
        }
        let mut bl = b.len();
        while bl > 0 && b[bl - 1] == 0 {
            bl -= 1;
        }
        match al.cmp(&bl) {
            Ordering::Equal => {
                for i in (0..al).rev() {
                    match a[i].cmp(&b[i]) {
                        Ordering::Equal => {}
                        other => return other,
                    }
                }
                Ordering::Equal
            }
            other => other,
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
            b'+' => (false, 1),
            b'-' => (true, 1),
            _ => (false, 0),
        };
        if start == bytes.len() {
            return Err(Error::SignOnly);
        }

        let digit_value = |byte: u8| -> Option<u32> {
            match byte {
                b'0'..=b'9' => Some((byte - b'0') as u32),
                b'a'..=b'z' => Some((byte - b'a') as u32 + 10),
                b'A'..=b'Z' => Some((byte - b'A') as u32 + 10),
                _ => None,
            }
        };

        for (i, &byte) in bytes.iter().enumerate().skip(start) {
            if byte == b'_' {
                continue;
            }
            if digit_value(byte).is_none_or(|v| v >= radix) {
                return Err(Error::InvalidDigit { index: i });
            }
        }

        let mut separator_error = None;
        for i in start..bytes.len() {
            if bytes[i] == b'_' {
                let left_ok = i > start
                    && bytes[i - 1] != b'_'
                    && digit_value(bytes[i - 1]).is_some_and(|v| v < radix);
                let right_ok = i + 1 < bytes.len()
                    && bytes[i + 1] != b'_'
                    && digit_value(bytes[i + 1]).is_some_and(|v| v < radix);
                if (!left_ok || !right_ok) && separator_error.is_none() {
                    separator_error = Some(i);
                }
            }
        }
        if let Some(index) = separator_error {
            return Err(Error::Separator { index });
        }

        let digits: Vec<(usize, u32)> = (start..bytes.len())
            .filter(|&i| bytes[i] != b'_')
            .map(|i| (i, digit_value(bytes[i]).unwrap()))
            .collect();

        if digits.len() > 1 && digits[0].1 == 0 {
            return Err(Error::LeadingZero { index: digits[0].0 });
        }
        if digits.len() > max_digits {
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: digits[max_digits].0,
            });
        }

        let mut limbs = Vec::new();
        for &(_, digit) in &digits {
            limbs = Self::mul_small_abs(&limbs, radix);
            limbs = Self::add_small_abs(&limbs, digit);
        }

        let mut result = Self { negative, limbs };
        result.normalize();
        Ok(result)
    }

    pub fn format(&self, radix: u32, min_digits: usize, group: usize) -> Result<String, Error> {
        if !(2..=36).contains(&radix) {
            return Err(Error::InvalidRadix { radix });
        }

        let mut value = self.limbs.clone();
        let mut digits = Vec::new();
        if value.is_empty() {
            digits.push(0u32);
        } else {
            while !value.is_empty() {
                let (quotient, rem) = Self::div_small_abs(&value, radix);
                digits.push(rem);
                value = quotient;
            }
            digits.reverse();
        }

        let target = min_digits.max(1);
        if digits.len() < target {
            let padding = target - digits.len();
            let mut padded = Vec::with_capacity(target);
            padded.resize(padding, 0);
            padded.extend(digits);
            digits = padded;
        }

        let mut magnitude = String::new();
        for (i, &digit) in digits.iter().enumerate() {
            if group > 0 && i > 0 && (digits.len() - i) % group == 0 {
                magnitude.push('_');
            }
            magnitude.push(if digit < 10 {
                (b'0' + digit as u8) as char
            } else {
                (b'a' + (digit - 10) as u8) as char
            });
        }

        if self.negative && !self.is_zero() {
            magnitude.insert(0, '-');
        }
        Ok(magnitude)
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.negative, rhs.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => self.abs_cmp(rhs),
            (true, true) => self.abs_cmp(rhs).reverse(),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        let mut result = if self.negative == rhs.negative {
            Self {
                negative: self.negative,
                limbs: Self::add_abs(&self.limbs, &rhs.limbs),
            }
        } else {
            match self.abs_cmp(rhs) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => Self {
                    negative: self.negative,
                    limbs: Self::sub_abs(&self.limbs, &rhs.limbs),
                },
                Ordering::Less => Self {
                    negative: rhs.negative,
                    limbs: Self::sub_abs(&rhs.limbs, &self.limbs),
                },
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
        let mut limbs = vec![0u32; self.limbs.len().saturating_add(rhs.limbs.len())];
        for (i, &a) in self.limbs.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &b) in rhs.limbs.iter().enumerate() {
                let k = i + j;
                let current = limbs[k] as u64 + a as u64 * b as u64 + carry;
                limbs[k] = (current % Self::BASE) as u32;
                carry = current / Self::BASE;
            }
            let mut k = i + rhs.limbs.len();
            while carry != 0 {
                if k == limbs.len() {
                    limbs.push(0);
                }
                let current = limbs[k] as u64 + carry;
                limbs[k] = (current % Self::BASE) as u32;
                carry = current / Self::BASE;
                k += 1;
            }
        }
        let mut result = Self {
            negative: self.negative != rhs.negative,
            limbs,
        };
        result.normalize();
        result
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }
        let (mut quotient, mut remainder) = Self::abs_div_rem(self, rhs);
        quotient.negative = !quotient.is_zero() && (self.negative != rhs.negative);
        remainder.negative = !remainder.is_zero() && self.negative;
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
        let mut a = Self { negative: false, limbs: self.limbs.clone() };
        let mut b = Self { negative: false, limbs: rhs.limbs.clone() };
        while !b.is_zero() {
            let (_, remainder) = Self::abs_div_rem(&a, &b);
            a = b;
            b = remainder;
        }
        a.negative = false;
        a
    }
}

fn main() {}