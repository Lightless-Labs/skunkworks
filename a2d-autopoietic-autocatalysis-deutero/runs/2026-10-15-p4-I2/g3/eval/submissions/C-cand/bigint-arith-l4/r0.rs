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
    // Little-endian base-2^32 limbs.
    limbs: Vec<u32>,
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

    fn abs(&self) -> Self {
        let mut result = self.clone();
        result.negative = false;
        result
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

    fn mul_small_assign(&mut self, factor: u32) {
        if factor == 0 || self.is_zero() {
            self.limbs.clear();
            self.negative = false;
            return;
        }

        let mut carry = 0u64;
        for limb in &mut self.limbs {
            let value = (*limb as u64) * factor as u64 + carry;
            *limb = value as u32;
            carry = value >> 32;
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
            let value = self.limbs[i] as u64 + carry;
            self.limbs[i] = value as u32;
            carry = value >> 32;
            i += 1;
        }
    }

    fn div_rem_small_assign(&mut self, divisor: u32) -> u32 {
        let mut remainder = 0u64;
        for limb in self.limbs.iter_mut().rev() {
            let value = (remainder << 32) | *limb as u64;
            *limb = (value / divisor as u64) as u32;
            remainder = value % divisor as u64;
        }
        self.normalize();
        remainder as u32
    }

    fn add_abs(lhs: &Self, rhs: &Self) -> Self {
        let count = lhs.limbs.len().max(rhs.limbs.len());
        let mut limbs = Vec::with_capacity(count + 1);
        let mut carry = 0u64;

        for i in 0..count {
            let a = lhs.limbs.get(i).copied().unwrap_or(0) as u64;
            let b = rhs.limbs.get(i).copied().unwrap_or(0) as u64;
            let sum = a + b + carry;
            limbs.push(sum as u32);
            carry = sum >> 32;
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

    // Requires |lhs| >= |rhs|.
    fn sub_abs(lhs: &Self, rhs: &Self) -> Self {
        let mut limbs = Vec::with_capacity(lhs.limbs.len());
        let mut borrow = 0i64;

        for i in 0..lhs.limbs.len() {
            let a = lhs.limbs[i] as i64;
            let b = rhs.limbs.get(i).copied().unwrap_or(0) as i64;
            let value = a - b - borrow;
            if value < 0 {
                limbs.push((value + (1i64 << 32)) as u32);
                borrow = 1;
            } else {
                limbs.push(value as u32);
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

    fn mul_abs(lhs: &Self, rhs: &Self) -> Self {
        if lhs.is_zero() || rhs.is_zero() {
            return Self::zero();
        }

        let mut limbs = vec![0u32; lhs.limbs.len() + rhs.limbs.len()];
        for (i, &a) in lhs.limbs.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &b) in rhs.limbs.iter().enumerate() {
                let index = i + j;
                let value = (a as u64) * (b as u64) + limbs[index] as u64 + carry;
                limbs[index] = value as u32;
                carry = value >> 32;
            }
            let mut index = i + rhs.limbs.len();
            while carry != 0 {
                if index == limbs.len() {
                    limbs.push(0);
                }
                let value = limbs[index] as u64 + carry;
                limbs[index] = value as u32;
                carry = value >> 32;
                index += 1;
            }
        }

        let mut result = Self {
            negative: false,
            limbs,
        };
        result.normalize();
        result
    }

    fn set_bit(&mut self, bit: usize) {
        let limb = bit / 32;
        if self.limbs.len() <= limb {
            self.limbs.resize(limb + 1, 0);
        }
        self.limbs[limb] |= 1u32 << (bit % 32);
    }

    fn bit_len(&self) -> usize {
        match self.limbs.last() {
            None => 0,
            Some(&top) => (self.limbs.len() - 1) * 32 + (32 - top.leading_zeros() as usize),
        }
    }

    fn bit(&self, bit: usize) -> bool {
        let limb = bit / 32;
        limb < self.limbs.len() && ((self.limbs[limb] >> (bit % 32)) & 1) != 0
    }

    fn shl1_add_bit(&mut self, bit: bool) {
        let mut carry = bit as u64;
        for limb in &mut self.limbs {
            let value = ((*limb as u64) << 1) | carry;
            *limb = value as u32;
            carry = value >> 32;
        }
        if carry != 0 {
            self.limbs.push(carry as u32);
        } else if self.limbs.is_empty() && bit {
            self.limbs.push(1);
        }
    }

    fn div_rem_abs(lhs: &Self, rhs: &Self) -> (Self, Self) {
        let mut quotient = Self::zero();
        let mut remainder = Self::zero();

        for bit in (0..lhs.bit_len()).rev() {
            remainder.shl1_add_bit(lhs.bit(bit));
            if remainder.abs_cmp(rhs) != Ordering::Less {
                remainder = Self::sub_abs(&remainder, rhs);
                quotient.set_bit(bit);
            }
        }

        quotient.normalize();
        remainder.normalize();
        (quotient, remainder)
    }

    fn parse_digit(byte: u8) -> Option<u32> {
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
            b'+' => (false, 1),
            b'-' => (true, 1),
            _ => (false, 0),
        };
        if start == bytes.len() {
            return Err(Error::SignOnly);
        }

        // Scan every magnitude byte for invalid digits before considering separators.
        let mut invalid = None;
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte == b'_' {
                continue;
            }
            match Self::parse_digit(byte) {
                Some(value) if value < radix => {}
                _ => {
                    invalid = Some(index);
                    break;
                }
            }
        }
        if let Some(index) = invalid {
            return Err(Error::InvalidDigit { index });
        }

        let mut separator = None;
        for index in start..bytes.len() {
            if bytes[index] == b'_' {
                let prev_is_digit = index > start
                    && Self::parse_digit(bytes[index - 1])
                        .is_some_and(|value| value < radix);
                let next_is_digit = index + 1 < bytes.len()
                    && Self::parse_digit(bytes[index + 1])
                        .is_some_and(|value| value < radix);
                if !prev_is_digit || !next_is_digit {
                    separator = Some(index);
                    break;
                }
            }
        }
        if let Some(index) = separator {
            return Err(Error::Separator { index });
        }

        let first_digit_index = start;
        let first_digit = Self::parse_digit(bytes[first_digit_index]).unwrap();
        let digit_count = bytes[start..].iter().filter(|&&byte| byte != b'_').count();
        if digit_count > 1 && first_digit == 0 {
            return Err(Error::LeadingZero {
                index: first_digit_index,
            });
        }

        if digit_count > max_digits {
            let mut seen = 0usize;
            for (index, &byte) in bytes.iter().enumerate().skip(start) {
                if byte != b'_' {
                    seen += 1;
                    if seen > max_digits {
                        return Err(Error::TooManyDigits {
                            limit: max_digits,
                            index,
                        });
                    }
                }
            }
        }

        let mut result = Self::zero();
        for &byte in &bytes[start..] {
            if byte == b'_' {
                continue;
            }
            result.mul_small_assign(radix);
            result.add_small_assign(Self::parse_digit(byte).unwrap());
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

        let mut magnitude = self.abs();
        let mut digits = Vec::new();
        if magnitude.is_zero() {
            digits.push('0');
        } else {
            while !magnitude.is_zero() {
                let digit = magnitude.div_rem_small_assign(radix);
                digits.push(Self::digit_char(digit));
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

        let mut output = String::new();
        if self.negative && !self.is_zero() {
            output.push('-');
        }
        for (index, ch) in digits.iter().enumerate() {
            if group > 0 && index > 0 && (digits.len() - index) % group == 0 {
                output.push('_');
            }
            output.push(*ch);
        }
        Ok(output)
    }

    fn digit_char(digit: u32) -> char {
        match digit {
            0..=9 => (b'0' + digit as u8) as char,
            _ => (b'a' + (digit - 10) as u8) as char,
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
            let mut result = Self::add_abs(self, rhs);
            result.negative = self.negative && !result.is_zero();
            result
        } else {
            match self.abs_cmp(rhs) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => {
                    let mut result = Self::sub_abs(self, rhs);
                    result.negative = self.negative && !result.is_zero();
                    result
                }
                Ordering::Less => {
                    let mut result = Self::sub_abs(rhs, self);
                    result.negative = rhs.negative && !result.is_zero();
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
        let mut result = Self::mul_abs(self, rhs);
        result.negative = !result.is_zero() && (self.negative != rhs.negative);
        result
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }

        let (mut quotient, mut remainder) = Self::div_rem_abs(&self.abs(), &rhs.abs());
        quotient.negative =
            !quotient.is_zero() && (self.negative != rhs.negative);
        remainder.negative = !remainder.is_zero() && self.negative;
        Ok((quotient, remainder))
    }

    pub fn pow(&self, exponent: u32) -> Result<Self, Error> {
        if exponent > 1024 {
            return Err(Error::ExponentTooLarge {
                exponent,
                limit: 1024,
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
            let remainder = Self::div_rem_abs(&a, &b).1;
            a = b;
            b = remainder;
        }
        a.negative = false;
        a
    }
}

fn main() {}