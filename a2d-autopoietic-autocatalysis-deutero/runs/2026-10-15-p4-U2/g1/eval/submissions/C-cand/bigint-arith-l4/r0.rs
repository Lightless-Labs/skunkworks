use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    negative: bool,
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

    fn from_u32(value: u32) -> Self {
        if value == 0 {
            Self::zero()
        } else {
            Self {
                negative: false,
                limbs: vec![value],
            }
        }
    }

    fn mul_small_assign(&mut self, value: u32) {
        if value == 0 || self.is_zero() {
            self.limbs.clear();
            self.negative = false;
            return;
        }
        let mut carry = 0u64;
        for limb in &mut self.limbs {
            let product = (*limb as u64) * (value as u64) + carry;
            *limb = product as u32;
            carry = product >> 32;
        }
        if carry != 0 {
            self.limbs.push(carry as u32);
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
            self.limbs[i] = sum as u32;
            carry = sum >> 32;
            i += 1;
        }
    }

    fn div_rem_small_assign(&mut self, divisor: u32) -> u32 {
        let mut rem = 0u64;
        for limb in self.limbs.iter_mut().rev() {
            let cur = (rem << 32) | (*limb as u64);
            *limb = (cur / divisor as u64) as u32;
            rem = cur % divisor as u64;
        }
        self.normalize();
        rem as u32
    }

    fn cmp_abs(&self, rhs: &Self) -> Ordering {
        match self.limbs.len().cmp(&rhs.limbs.len()) {
            Ordering::Equal => {}
            other => return other,
        }
        for (a, b) in self.limbs.iter().rev().zip(rhs.limbs.iter().rev()) {
            match a.cmp(b) {
                Ordering::Equal => {}
                other => return other,
            }
        }
        Ordering::Equal
    }

    fn add_abs(&self, rhs: &Self) -> Self {
        let n = self.limbs.len().max(rhs.limbs.len());
        let mut out = Vec::with_capacity(n + 1);
        let mut carry = 0u64;
        for i in 0..n {
            let a = self.limbs.get(i).copied().unwrap_or(0) as u64;
            let b = rhs.limbs.get(i).copied().unwrap_or(0) as u64;
            let sum = a + b + carry;
            out.push(sum as u32);
            carry = sum >> 32;
        }
        if carry != 0 {
            out.push(carry as u32);
        }
        Self {
            negative: false,
            limbs: out,
        }
    }

    // Requires |self| >= |rhs|.
    fn sub_abs(&self, rhs: &Self) -> Self {
        let mut out = Vec::with_capacity(self.limbs.len());
        let mut borrow = 0u64;
        for i in 0..self.limbs.len() {
            let a = self.limbs[i] as u64;
            let b = rhs.limbs.get(i).copied().unwrap_or(0) as u64 + borrow;
            if a >= b {
                out.push((a - b) as u32);
                borrow = 0;
            } else {
                out.push(((1u64 << 32) + a - b) as u32);
                borrow = 1;
            }
        }
        let mut result = Self {
            negative: false,
            limbs: out,
        };
        result.normalize();
        result
    }

    fn shl1_assign(&mut self) {
        if self.is_zero() {
            return;
        }
        let mut carry = 0u64;
        for limb in &mut self.limbs {
            let v = ((*limb as u64) << 1) | carry;
            *limb = v as u32;
            carry = v >> 32;
        }
        if carry != 0 {
            self.limbs.push(carry as u32);
        }
    }

    fn bit_len(&self) -> usize {
        match self.limbs.last() {
            None => 0,
            Some(last) => (self.limbs.len() - 1) * 32 + (32 - last.leading_zeros() as usize),
        }
    }

    fn bit(&self, index: usize) -> bool {
        self.limbs
            .get(index / 32)
            .map(|limb| ((limb >> (index % 32)) & 1) != 0)
            .unwrap_or(false)
    }

    fn set_bit(&mut self, index: usize) {
        let word = index / 32;
        if self.limbs.len() <= word {
            self.limbs.resize(word + 1, 0);
        }
        self.limbs[word] |= 1u32 << (index % 32);
    }

    fn div_rem_abs(&self, divisor: &Self) -> (Self, Self) {
        let mut quotient = Self::zero();
        let mut remainder = Self::zero();
        for index in (0..self.bit_len()).rev() {
            remainder.shl1_assign();
            if self.bit(index) {
                remainder.add_small_assign(1);
            }
            if remainder.cmp_abs(divisor) != Ordering::Less {
                remainder = remainder.sub_abs(divisor);
                quotient.set_bit(index);
            }
        }
        quotient.normalize();
        remainder.normalize();
        (quotient, remainder)
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
        let mut digit_values = vec![None; bytes.len()];
        for index in start..bytes.len() {
            let byte = bytes[index];
            if byte == b'_' {
                continue;
            }
            let value = match byte {
                b'0'..=b'9' => Some((byte - b'0') as u32),
                b'a'..=b'z' => Some((byte - b'a' + 10) as u32),
                b'A'..=b'Z' => Some((byte - b'A' + 10) as u32),
                _ => None,
            };
            match value {
                Some(value) if value < radix => digit_values[index] = Some(value),
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

        let mut malformed_separator = None;
        for index in start..bytes.len() {
            if bytes[index] == b'_'
                && (index == start
                    || index + 1 == bytes.len()
                    || digit_values[index - 1].is_none()
                    || digit_values[index + 1].is_none())
            {
                malformed_separator = Some(index);
                break;
            }
        }
        if let Some(index) = malformed_separator {
            return Err(Error::Separator { index });
        }

        let digit_indices: Vec<usize> = (start..bytes.len())
            .filter(|&index| bytes[index] != b'_')
            .collect();

        if digit_indices.len() > 1 && digit_values[digit_indices[0]] == Some(0) {
            return Err(Error::LeadingZero {
                index: digit_indices[0],
            });
        }

        if digit_indices.len() > max_digits {
            let index = if max_digits < digit_indices.len() {
                digit_indices[max_digits]
            } else {
                unreachable!()
            };
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index,
            });
        }

        let mut result = Self::zero();
        for index in digit_indices {
            result.mul_small_assign(radix);
            result.add_small_assign(digit_values[index].unwrap());
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

        let mut magnitude = self.clone();
        magnitude.negative = false;
        let mut digits = Vec::new();
        if magnitude.is_zero() {
            digits.push('0');
        } else {
            while !magnitude.is_zero() {
                let digit = magnitude.div_rem_small_assign(radix);
                digits.push(if digit < 10 {
                    (b'0' + digit as u8) as char
                } else {
                    (b'a' + (digit - 10) as u8) as char
                });
            }
            digits.reverse();
        }
        while digits.len() < min_digits {
            digits.insert(0, '0');
        }

        let mut out = String::new();
        if self.negative && !self.is_zero() {
            out.push('-');
        }
        for (index, digit) in digits.iter().enumerate() {
            if group > 0 && index > 0 && (digits.len() - index) % group == 0 {
                out.push('_');
            }
            out.push(*digit);
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
            let mut result = self.add_abs(rhs);
            result.negative = self.negative && !result.is_zero();
            result
        } else {
            match self.cmp_abs(rhs) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => {
                    let mut result = self.sub_abs(rhs);
                    result.negative = self.negative;
                    result
                }
                Ordering::Less => {
                    let mut result = rhs.sub_abs(self);
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
                let k = i + j;
                let value = limbs[k] as u64 + (a as u64) * (b as u64) + carry;
                limbs[k] = value as u32;
                carry = value >> 32;
            }
            let mut k = i + rhs.limbs.len();
            while carry != 0 {
                if k == limbs.len() {
                    limbs.push(0);
                }
                let value = limbs[k] as u64 + carry;
                limbs[k] = value as u32;
                carry = value >> 32;
                k += 1;
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
        let (mut quotient, mut remainder) = self.div_rem_abs(rhs);
        quotient.negative = !quotient.is_zero() && (self.negative ^ rhs.negative);
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
        let mut a = self.clone();
        let mut b = rhs.clone();
        a.negative = false;
        b.negative = false;
        while !b.is_zero() {
            let (_, remainder) = a.div_rem_abs(&b);
            a = b;
            b = remainder;
        }
        a.negative = false;
        a
    }
}

fn main() {}