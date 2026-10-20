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
    // Little-endian base-1_000_000_000 limbs. Zero is represented by [0].
    limbs: Vec<u32>,
}

const LIMB_BASE: u64 = 1_000_000_000;

impl Big {
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

    fn is_zero(&self) -> bool {
        self.limbs.len() == 1 && self.limbs[0] == 0
    }

    fn normalize(&mut self) {
        while self.limbs.len() > 1 && self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }
        if self.is_zero() {
            self.negative = false;
        }
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

    fn abs_add(&self, rhs: &Self) -> Vec<u32> {
        let n = self.limbs.len().max(rhs.limbs.len());
        let mut out = Vec::with_capacity(n + 1);
        let mut carry = 0u64;
        for i in 0..n {
            let a = self.limbs.get(i).copied().unwrap_or(0) as u64;
            let b = rhs.limbs.get(i).copied().unwrap_or(0) as u64;
            let sum = a + b + carry;
            out.push((sum % LIMB_BASE) as u32);
            carry = sum / LIMB_BASE;
        }
        if carry != 0 {
            out.push(carry as u32);
        }
        out
    }

    // Requires |a| >= |b|.
    fn abs_sub_limbs(a: &[u32], b: &[u32]) -> Vec<u32> {
        let mut out = Vec::with_capacity(a.len());
        let mut borrow = 0i64;
        for i in 0..a.len() {
            let av = a[i] as i64 - borrow;
            let bv = b.get(i).copied().unwrap_or(0) as i64;
            if av < bv {
                out.push((av + LIMB_BASE as i64 - bv) as u32);
                borrow = 1;
            } else {
                out.push((av - bv) as u32);
                borrow = 0;
            }
        }
        while out.len() > 1 && out.last() == Some(&0) {
            out.pop();
        }
        out
    }

    fn mul_small_assign(&mut self, multiplier: u32) {
        if multiplier == 0 {
            *self = Self::zero();
            return;
        }
        let mut carry = 0u64;
        for limb in &mut self.limbs {
            let value = *limb as u64 * multiplier as u64 + carry;
            *limb = (value % LIMB_BASE) as u32;
            carry = value / LIMB_BASE;
        }
        while carry != 0 {
            self.limbs.push((carry % LIMB_BASE) as u32);
            carry /= LIMB_BASE;
        }
    }

    fn add_small_assign(&mut self, addend: u32) {
        let mut carry = addend as u64;
        let mut i = 0;
        while carry != 0 {
            if i == self.limbs.len() {
                self.limbs.push(0);
            }
            let value = self.limbs[i] as u64 + carry;
            self.limbs[i] = (value % LIMB_BASE) as u32;
            carry = value / LIMB_BASE;
            i += 1;
        }
    }

    fn div_rem_small_abs(&self, divisor: u32) -> (Self, u32) {
        let mut quotient = vec![0u32; self.limbs.len()];
        let mut remainder = 0u64;
        for i in (0..self.limbs.len()).rev() {
            let value = remainder * LIMB_BASE + self.limbs[i] as u64;
            quotient[i] = (value / divisor as u64) as u32;
            remainder = value % divisor as u64;
        }
        let mut q = Self {
            negative: false,
            limbs: quotient,
        };
        q.normalize();
        (q, remainder as u32)
    }

    fn abs_value(&self) -> Self {
        let mut result = self.clone();
        result.negative = false;
        result
    }

    fn mul_abs(&self, rhs: &Self) -> Self {
        if self.is_zero() || rhs.is_zero() {
            return Self::zero();
        }
        let mut limbs = vec![0u32; self.limbs.len() + rhs.limbs.len()];
        for (i, &a) in self.limbs.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &b) in rhs.limbs.iter().enumerate() {
                let k = i + j;
                let value = limbs[k] as u64 + a as u64 * b as u64 + carry;
                limbs[k] = (value % LIMB_BASE) as u32;
                carry = value / LIMB_BASE;
            }
            let mut k = i + rhs.limbs.len();
            while carry != 0 {
                if k == limbs.len() {
                    limbs.push(0);
                }
                let value = limbs[k] as u64 + carry;
                limbs[k] = (value % LIMB_BASE) as u32;
                carry = value / LIMB_BASE;
                k += 1;
            }
        }
        let mut result = Self {
            negative: false,
            limbs,
        };
        result.normalize();
        result
    }

    fn abs_div_rem(&self, divisor: &Self) -> (Self, Self) {
        if self.abs_cmp(divisor) == Ordering::Less {
            return (Self::zero(), self.abs_value());
        }

        let mut quotient_be = Vec::with_capacity(self.limbs.len() * 2);
        let mut remainder = Self::zero();

        for &limb in self.limbs.iter().rev() {
            remainder.limbs.insert(0, limb);
            remainder.normalize();

            let mut lo = 0u64;
            let mut hi = LIMB_BASE - 1;
            let mut best = 0u64;
            while lo <= hi {
                let mid = lo + (hi - lo) / 2;
                let mut product = divisor.clone();
                product.negative = false;
                product.mul_small_assign(mid as u32);
                match product.abs_cmp(&remainder) {
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
                let mut product = divisor.clone();
                product.negative = false;
                product.mul_small_assign(best as u32);
                remainder.limbs = Self::abs_sub_limbs(&remainder.limbs, &product.limbs);
                remainder.normalize();
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

        let mut digit_values = Vec::with_capacity(bytes.len() - start);
        let mut invalid = None;
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte == b'_' {
                digit_values.push(None);
                continue;
            }
            let value = match byte {
                b'0'..=b'9' => Some((byte - b'0') as u32),
                b'a'..=b'z' => Some((byte - b'a' + 10) as u32),
                b'A'..=b'Z' => Some((byte - b'A' + 10) as u32),
                _ => None,
            };
            match value {
                Some(v) if v < radix => digit_values.push(Some(v)),
                _ => {
                    if invalid.is_none() {
                        invalid = Some(index);
                    }
                    digit_values.push(None);
                }
            }
        }
        if let Some(index) = invalid {
            return Err(Error::InvalidDigit { index });
        }

        let mut first_separator = None;
        for pos in 0..digit_values.len() {
            if bytes[start + pos] == b'_' {
                let valid_before = pos > 0 && digit_values[pos - 1].is_some();
                let valid_after = pos + 1 < digit_values.len() && digit_values[pos + 1].is_some();
                if !(valid_before && valid_after) && first_separator.is_none() {
                    first_separator = Some(start + pos);
                }
            }
        }
        if let Some(index) = first_separator {
            return Err(Error::Separator { index });
        }

        let digit_positions: Vec<(usize, u32)> = digit_values
            .iter()
            .enumerate()
            .filter_map(|(i, value)| value.map(|v| (start + i, v)))
            .collect();

        if digit_positions.len() > 1 && digit_positions[0].1 == 0 {
            return Err(Error::LeadingZero {
                index: digit_positions[0].0,
            });
        }

        if digit_positions.len() > max_digits {
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: digit_positions[max_digits].0,
            });
        }

        let mut result = Self::zero();
        for &(_, digit) in &digit_positions {
            result.mul_small_assign(radix);
            result.add_small_assign(digit);
        }
        result.negative = negative && !result.is_zero();
        Ok(result)
    }

    pub fn format(&self, radix: u32, min_digits: usize, group: usize) -> Result<String, Error> {
        if !(2..=36).contains(&radix) {
            return Err(Error::InvalidRadix { radix });
        }

        let mut value = self.abs_value();
        let mut digits = Vec::new();
        if value.is_zero() {
            digits.push(0u32);
        } else {
            while !value.is_zero() {
                let (quotient, remainder) = value.div_rem_small_abs(radix);
                digits.push(remainder);
                value = quotient;
            }
            digits.reverse();
        }

        if digits.len() < min_digits {
            let mut padded = vec![0; min_digits - digits.len()];
            padded.extend(digits);
            digits = padded;
        }

        let mut magnitude = String::new();
        for (i, digit) in digits.iter().enumerate() {
            if group > 0 && i > 0 && (digits.len() - i) % group == 0 {
                magnitude.push('_');
            }
            magnitude.push(if *digit < 10 {
                (b'0' + *digit as u8) as char
            } else {
                (b'a' + (*digit - 10) as u8) as char
            });
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
            (false, false) => self.abs_cmp(rhs),
            (true, true) => self.abs_cmp(rhs).reverse(),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            let mut result = Self {
                negative: self.negative,
                limbs: self.abs_add(rhs),
            };
            result.normalize();
            result
        } else {
            match self.abs_cmp(rhs) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => {
                    let mut result = Self {
                        negative: self.negative,
                        limbs: Self::abs_sub_limbs(&self.limbs, &rhs.limbs),
                    };
                    result.normalize();
                    result
                }
                Ordering::Less => {
                    let mut result = Self {
                        negative: rhs.negative,
                        limbs: Self::abs_sub_limbs(&rhs.limbs, &self.limbs),
                    };
                    result.normalize();
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
        let mut result = self.mul_abs(rhs);
        result.negative = !result.is_zero() && (self.negative != rhs.negative);
        result
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }
        let (mut quotient, mut remainder) = self.abs_div_rem(rhs);
        quotient.negative = !quotient.is_zero() && (self.negative != rhs.negative);
        remainder.negative = !remainder.is_zero() && self.negative;
        Ok((quotient, remainder))
    }

    pub fn pow(&self, exponent: u32) -> Result<Self, Error> {
        const LIMIT: u32 = 1024;
        if exponent > LIMIT {
            return Err(Error::ExponentTooLarge {
                exponent,
                limit: LIMIT,
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
        let mut a = self.abs_value();
        let mut b = rhs.abs_value();
        while !b.is_zero() {
            let (_, remainder) = a.abs_div_rem(&b);
            a = b;
            b = remainder;
        }
        a.negative = false;
        a
    }
}