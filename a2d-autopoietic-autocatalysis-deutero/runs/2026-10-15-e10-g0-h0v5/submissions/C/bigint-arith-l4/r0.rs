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
    // Canonical magnitude in base 2^32, least-significant limb first.
    limbs: Vec<u32>,
}

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

    fn cmp_abs(&self, rhs: &Self) -> Ordering {
        self.limbs
            .len()
            .cmp(&rhs.limbs.len())
            .then_with(|| self.limbs.iter().rev().cmp(rhs.limbs.iter().rev()))
    }

    fn mul_small_assign(&mut self, factor: u32) {
        if factor == 0 || self.is_zero() {
            *self = Self::zero();
            return;
        }
        let mut carry = 0u64;
        for limb in &mut self.limbs {
            let n = *limb as u64 * factor as u64 + carry;
            *limb = n as u32;
            carry = n >> 32;
        }
        if carry != 0 {
            self.limbs.push(carry as u32);
        }
    }

    fn add_small_assign(&mut self, addend: u32) {
        let mut carry = addend as u64;
        let mut i = 0;
        while carry != 0 {
            if i == self.limbs.len() {
                self.limbs.push(0);
            }
            let n = self.limbs[i] as u64 + carry;
            self.limbs[i] = n as u32;
            carry = n >> 32;
            i += 1;
        }
    }

    fn div_small_assign(&mut self, divisor: u32) -> u32 {
        let mut remainder = 0u64;
        for limb in self.limbs.iter_mut().rev() {
            let n = (remainder << 32) | *limb as u64;
            *limb = (n / divisor as u64) as u32;
            remainder = n % divisor as u64;
        }
        self.normalize();
        remainder as u32
    }

    pub fn parse(text: &str, radix: u32, max_digits: usize) -> Result<Self, Error> {
        if !(2..=36).contains(&radix) {
            return Err(Error::InvalidRadix { radix });
        }

        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(Error::Empty);
        }

        let mut start = 0;
        let mut negative = false;
        if bytes[0] == b'+' || bytes[0] == b'-' {
            negative = bytes[0] == b'-';
            start = 1;
            if start == bytes.len() {
                return Err(Error::SignOnly);
            }
        }

        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' && digit_value(byte).is_none_or(|d| d as u32 >= radix) {
                return Err(Error::InvalidDigit { index });
            }
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

        let positions: Vec<usize> = (start..bytes.len())
            .filter(|&i| bytes[i] != b'_')
            .collect();

        if positions.len() > 1 && digit_value(bytes[positions[0]]) == Some(0) {
            return Err(Error::LeadingZero {
                index: positions[0],
            });
        }

        if positions.len() > max_digits {
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: positions[max_digits],
            });
        }

        let mut result = Self::zero();
        for &index in &positions {
            result.mul_small_assign(radix);
            result.add_small_assign(digit_value(bytes[index]).unwrap() as u32);
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

        let mut value = self.clone();
        value.negative = false;
        let mut digits = Vec::new();
        while !value.is_zero() {
            digits.push(value.div_small_assign(radix));
        }
        if digits.is_empty() {
            digits.push(0);
        }
        while digits.len() < min_digits {
            digits.push(0);
        }
        digits.reverse();

        let mut out = String::new();
        if self.negative && !self.is_zero() {
            out.push('-');
        }
        for (i, &digit) in digits.iter().enumerate() {
            if group > 0 && i > 0 && (digits.len() - i) % group == 0 {
                out.push('_');
            }
            out.push(digit_char(digit as u8));
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
            let mut result = Self {
                negative: self.negative,
                limbs: add_abs(&self.limbs, &rhs.limbs),
            };
            result.normalize();
            result
        } else {
            match self.cmp_abs(rhs) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => {
                    let mut result = Self {
                        negative: self.negative,
                        limbs: sub_abs(&self.limbs, &rhs.limbs),
                    };
                    result.normalize();
                    result
                }
                Ordering::Less => {
                    let mut result = Self {
                        negative: rhs.negative,
                        limbs: sub_abs(&rhs.limbs, &self.limbs),
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
        if self.is_zero() || rhs.is_zero() {
            return Self::zero();
        }
        let mut result = Self {
            negative: self.negative ^ rhs.negative,
            limbs: mul_abs(&self.limbs, &rhs.limbs),
        };
        result.normalize();
        result
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }
        let (q, r) = div_abs(&self.limbs, &rhs.limbs);
        let mut quotient = Self {
            negative: self.negative ^ rhs.negative,
            limbs: q,
        };
        let mut remainder = Self {
            negative: self.negative,
            limbs: r,
        };
        quotient.normalize();
        remainder.normalize();
        Ok((quotient, remainder))
    }

    pub fn pow(&self, exponent: u32) -> Result<Self, Error> {
        if exponent > 1024 {
            return Err(Error::ExponentTooLarge {
                exponent,
                limit: 1024,
            });
        }
        let mut n = exponent;
        let mut base = self.clone();
        let mut result = Self::one();
        while n > 0 {
            if n & 1 == 1 {
                result = result.mul(&base);
            }
            n >>= 1;
            if n > 0 {
                base = base.mul(&base);
            }
        }
        Ok(result)
    }

    pub fn gcd(&self, rhs: &Self) -> Self {
        let mut a = self.clone();
        let mut b = rhs.clone();
        a.negative = false;
        b.negative = false;
        while !b.is_zero() {
            let (_, r) = a.div_rem(&b).unwrap();
            a = b;
            b = r;
            b.negative = false;
        }
        a.negative = false;
        a
    }
}

fn digit_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'z' => Some(byte - b'a' + 10),
        b'A'..=b'Z' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn digit_char(digit: u8) -> char {
    match digit {
        0..=9 => (b'0' + digit) as char,
        _ => (b'a' + digit - 10) as char,
    }
}

fn add_abs(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(a.len().max(b.len()) + 1);
    let mut carry = 0u64;
    for i in 0..a.len().max(b.len()) {
        let av = a.get(i).copied().unwrap_or(0) as u64;
        let bv = b.get(i).copied().unwrap_or(0) as u64;
        let sum = av + bv + carry;
        out.push(sum as u32);
        carry = sum >> 32;
    }
    if carry != 0 {
        out.push(carry as u32);
    }
    out
}

fn sub_abs(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(a.len());
    let mut borrow = 0i64;
    for i in 0..a.len() {
        let av = a[i] as i64;
        let bv = b.get(i).copied().unwrap_or(0) as i64;
        let mut value = av - bv - borrow;
        if value < 0 {
            value += 1i64 << 32;
            borrow = 1;
        } else {
            borrow = 0;
        }
        out.push(value as u32);
    }
    while out.len() > 1 && out.last() == Some(&0) {
        out.pop();
    }
    out
}

fn mul_abs(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut out = vec![0u32; a.len() + b.len()];
    for (i, &av) in a.iter().enumerate() {
        let mut carry = 0u128;
        for (j, &bv) in b.iter().enumerate() {
            let k = i + j;
            let n = out[k] as u128 + av as u128 * bv as u128 + carry;
            out[k] = n as u32;
            carry = n >> 32;
        }
        let mut k = i + b.len();
        while carry != 0 {
            if k == out.len() {
                out.push(0);
            }
            let n = out[k] as u128 + carry;
            out[k] = n as u32;
            carry = n >> 32;
            k += 1;
        }
    }
    while out.len() > 1 && out.last() == Some(&0) {
        out.pop();
    }
    out
}

fn cmp_limbs(a: &[u32], b: &[u32]) -> Ordering {
    a.len()
        .cmp(&b.len())
        .then_with(|| a.iter().rev().cmp(b.iter().rev()))
}

fn div_abs(a: &[u32], b: &[u32]) -> (Vec<u32>, Vec<u32>) {
    let mut quotient = vec![0u32; a.len()];
    let mut remainder = vec![0u32];

    for bit_index in (0..a.len() * 32).rev() {
        let bit = (a[bit_index / 32] >> (bit_index % 32)) & 1;
        let mut carry = bit as u64;
        for limb in &mut remainder {
            let n = ((*limb as u64) << 1) | carry;
            *limb = n as u32;
            carry = n >> 32;
        }
        if carry != 0 {
            remainder.push(carry as u32);
        }
        while remainder.len() > 1 && remainder.last() == Some(&0) {
            remainder.pop();
        }

        if cmp_limbs(&remainder, b) != Ordering::Less {
            remainder = sub_abs(&remainder, b);
            quotient[bit_index / 32] |= 1u32 << (bit_index % 32);
        }
    }

    while quotient.len() > 1 && quotient.last() == Some(&0) {
        quotient.pop();
    }
    while remainder.len() > 1 && remainder.last() == Some(&0) {
        remainder.pop();
    }
    (quotient, remainder)
}

fn main() {}