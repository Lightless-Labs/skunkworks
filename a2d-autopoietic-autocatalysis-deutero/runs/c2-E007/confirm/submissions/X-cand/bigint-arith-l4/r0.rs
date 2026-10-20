use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    negative: bool,
    // Little-endian base-1,000,000,000 limbs.
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
        let mut out = Vec::with_capacity(a.len().max(b.len()) + 1);
        let mut carry = 0u64;
        for i in 0..a.len().max(b.len()) {
            let sum = a.get(i).copied().unwrap_or(0) as u64
                + b.get(i).copied().unwrap_or(0) as u64
                + carry;
            out.push((sum % BASE) as u32);
            carry = sum / BASE;
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
            let mut value = a[i] as i64
                - b.get(i).copied().unwrap_or(0) as i64
                - borrow;
            if value < 0 {
                value += BASE as i64;
                borrow = 1;
            } else {
                borrow = 0;
            }
            out.push(value as u32);
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
        let mut out = Vec::with_capacity(a.len() + 1);
        let mut carry = 0u64;
        for &limb in a {
            let value = limb as u64 * m as u64 + carry;
            out.push((value % BASE) as u32);
            carry = value / BASE;
        }
        while carry != 0 {
            out.push((carry % BASE) as u32);
            carry /= BASE;
        }
        out
    }

    fn add_small_abs(a: &mut Vec<u32>, value: u32) {
        if value == 0 {
            return;
        }
        let mut carry = value as u64;
        let mut i = 0;
        while carry != 0 {
            if i == a.len() {
                a.push(0);
            }
            let sum = a[i] as u64 + carry;
            a[i] = (sum % BASE) as u32;
            carry = sum / BASE;
            i += 1;
        }
    }

    fn div_small_abs(a: &[u32], divisor: u32) -> (Vec<u32>, u32) {
        let mut quotient = vec![0; a.len()];
        let mut remainder = 0u64;
        for i in (0..a.len()).rev() {
            let value = remainder * BASE + a[i] as u64;
            quotient[i] = (value / divisor as u64) as u32;
            remainder = value % divisor as u64;
        }
        while quotient.last() == Some(&0) {
            quotient.pop();
        }
        (quotient, remainder as u32)
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

        let mut invalid = None;
        let mut digit_values = Vec::new();
        let mut digit_offsets = Vec::new();
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte == b'_' {
                continue;
            }
            match Self::digit_value(byte) {
                Some(value) if value < radix => {
                    digit_values.push(value);
                    digit_offsets.push(index);
                }
                _ => {
                    if invalid.is_none() {
                        invalid = Some(index);
                    }
                }
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

        if digit_values.len() > 1 && digit_values[0] == 0 {
            return Err(Error::LeadingZero {
                index: digit_offsets[0],
            });
        }
        if digit_values.len() > max_digits {
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: digit_offsets[max_digits],
            });
        }

        let mut result = Self::zero();
        for digit in digit_values {
            result.limbs = Self::mul_small_abs(&result.limbs, radix);
            Self::add_small_abs(&mut result.limbs, digit);
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
            digits.push(0u32);
        } else {
            while !value.is_zero() {
                let (quotient, remainder) = Self::div_small_abs(&value.limbs, radix);
                value.limbs = quotient;
                digits.push(remainder);
            }
            digits.reverse();
        }

        while digits.len() < min_digits {
            digits.insert(0, 0);
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
        let mut result = if self.negative == rhs.negative {
            Self {
                negative: self.negative,
                limbs: Self::add_abs(&self.limbs, &rhs.limbs),
            }
        } else {
            match self.abs_cmp(rhs) {
                Ordering::Greater => Self {
                    negative: self.negative,
                    limbs: Self::sub_abs(&self.limbs, &rhs.limbs),
                },
                Ordering::Less => Self {
                    negative: rhs.negative,
                    limbs: Self::sub_abs(&rhs.limbs, &self.limbs),
                },
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
                let value = limbs[index] as u64 + a as u64 * b as u64 + carry;
                limbs[index] = (value % BASE) as u32;
                carry = value / BASE;
            }
            let mut index = i + rhs.limbs.len();
            while carry != 0 {
                if index == limbs.len() {
                    limbs.push(0);
                }
                let value = limbs[index] as u64 + carry;
                limbs[index] = (value % BASE) as u32;
                carry = value / BASE;
                index += 1;
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
        if self.is_zero() {
            return Ok((Self::zero(), Self::zero()));
        }

        let dividend = self.abs();
        let divisor = rhs.abs();
        if dividend.abs_cmp(&divisor) == Ordering::Less {
            return Ok((Self::zero(), self.clone()));
        }

        let mut quotient_be = Vec::with_capacity(dividend.limbs.len());
        let mut remainder = Vec::<u32>::new();
        for &limb in dividend.limbs.iter().rev() {
            remainder.insert(0, limb);
            while remainder.last() == Some(&0) {
                remainder.pop();
            }
            let mut low = 0u32;
            let mut high = (BASE - 1) as u32;
            while low < high {
                let mid = low + ((high - low) / 2) + 1;
                let product = Self::mul_small_abs(&divisor.limbs, mid);
                let cmp = Self {
                    negative: false,
                    limbs: remainder.clone(),
                }
                .abs_cmp(&Self {
                    negative: false,
                    limbs: product,
                });
                if cmp != Ordering::Less {
                    low = mid;
                } else {
                    high = mid - 1;
                }
            }
            quotient_be.push(low);
            if low != 0 {
                let product = Self::mul_small_abs(&divisor.limbs, low);
                remainder = Self::sub_abs(&remainder, &product);
            }
        }
        quotient_be.reverse();
        while quotient_be.last() == Some(&0) {
            quotient_be.pop();
        }

        let mut quotient = Self {
            negative: self.negative != rhs.negative,
            limbs: quotient_be,
        };
        quotient.normalize();
        let mut rem = Self {
            negative: self.negative,
            limbs: remainder,
        };
        rem.normalize();
        Ok((quotient, rem))
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
        a.negative = false;
        a
    }
}