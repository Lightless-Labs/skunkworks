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

impl Big {
    const BASE: u64 = 1_000_000_000;

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

    fn abs_add(&self, rhs: &Self) -> Self {
        let n = self.limbs.len().max(rhs.limbs.len());
        let mut limbs = Vec::with_capacity(n + 1);
        let mut carry = 0u64;

        for i in 0..n {
            let a = u64::from(*self.limbs.get(i).unwrap_or(&0));
            let b = u64::from(*rhs.limbs.get(i).unwrap_or(&0));
            let sum = a + b + carry;
            limbs.push((sum % Self::BASE) as u32);
            carry = sum / Self::BASE;
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

    // Requires |self| >= |rhs|.
    fn abs_sub(&self, rhs: &Self) -> Self {
        let mut limbs = Vec::with_capacity(self.limbs.len());
        let mut borrow = 0i64;

        for i in 0..self.limbs.len() {
            let a = i64::from(self.limbs[i]);
            let b = i64::from(*rhs.limbs.get(i).unwrap_or(&0));
            let mut value = a - b - borrow;
            if value < 0 {
                value += Self::BASE as i64;
                borrow = 1;
            } else {
                borrow = 0;
            }
            limbs.push(value as u32);
        }

        let mut result = Self {
            negative: false,
            limbs,
        };
        result.normalize();
        result
    }

    fn mul_small_abs(&self, factor: u32) -> Self {
        if factor == 0 || self.is_zero() {
            return Self::zero();
        }

        let mut limbs = Vec::with_capacity(self.limbs.len() + 1);
        let mut carry = 0u64;
        for &limb in &self.limbs {
            let product = u64::from(limb) * u64::from(factor) + carry;
            limbs.push((product % Self::BASE) as u32);
            carry = product / Self::BASE;
        }
        while carry != 0 {
            limbs.push((carry % Self::BASE) as u32);
            carry /= Self::BASE;
        }

        let mut result = Self {
            negative: false,
            limbs,
        };
        result.normalize();
        result
    }

    fn add_small_abs(&mut self, value: u32) {
        let mut carry = u64::from(value);
        let mut i = 0;
        while carry != 0 {
            if i == self.limbs.len() {
                self.limbs.push(0);
            }
            let sum = u64::from(self.limbs[i]) + carry;
            self.limbs[i] = (sum % Self::BASE) as u32;
            carry = sum / Self::BASE;
            i += 1;
        }
        self.normalize();
    }

    fn div_rem_small_abs(&self, divisor: u32) -> (Self, u32) {
        let mut quotient = vec![0u32; self.limbs.len()];
        let mut remainder = 0u64;
        for i in (0..self.limbs.len()).rev() {
            let current = remainder * Self::BASE + u64::from(self.limbs[i]);
            quotient[i] = (current / u64::from(divisor)) as u32;
            remainder = current % u64::from(divisor);
        }
        let mut result = Self {
            negative: false,
            limbs: quotient,
        };
        result.normalize();
        (result, remainder as u32)
    }

    fn abs(&self) -> Self {
        let mut result = self.clone();
        result.negative = false;
        result
    }

    pub fn parse(text: &str, radix: u32, max_digits: usize) -> Result<Self, Error> {
        if !(2..=36).contains(&radix) {
            return Err(Error::InvalidRadix { radix });
        }
        if text.is_empty() {
            return Err(Error::Empty);
        }

        let bytes = text.as_bytes();
        let mut start = 0;
        let mut negative = false;
        if bytes[0] == b'+' || bytes[0] == b'-' {
            negative = bytes[0] == b'-';
            start = 1;
            if start == bytes.len() {
                return Err(Error::SignOnly);
            }
        }

        let mut invalid = None;
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte == b'_' {
                continue;
            }
            let digit = match byte {
                b'0'..=b'9' => u32::from(byte - b'0'),
                b'a'..=b'z' => u32::from(byte - b'a') + 10,
                b'A'..=b'Z' => u32::from(byte - b'A') + 10,
                _ => {
                    invalid = Some(index);
                    break;
                }
            };
            if digit >= radix {
                invalid = Some(index);
                break;
            }
        }
        if let Some(index) = invalid {
            return Err(Error::InvalidDigit { index });
        }

        let mut first_separator = None;
        for index in start..bytes.len() {
            if bytes[index] == b'_' {
                let preceding_valid = index > start && bytes[index - 1] != b'_';
                let following_valid = index + 1 < bytes.len() && bytes[index + 1] != b'_';
                if !preceding_valid || !following_valid {
                    first_separator = Some(index);
                    break;
                }
            }
        }
        if let Some(index) = first_separator {
            return Err(Error::Separator { index });
        }

        let digit_count = bytes[start..].iter().filter(|&&b| b != b'_').count();
        let first_digit = (start..bytes.len()).find(|&i| bytes[i] != b'_').unwrap();
        if digit_count > 1 && bytes[first_digit] == b'0' {
            return Err(Error::LeadingZero { index: first_digit });
        }
        if digit_count > max_digits {
            let excess = max_digits;
            let index = bytes[start..]
                .iter()
                .enumerate()
                .filter(|(_, b)| **b != b'_')
                .nth(excess)
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
            let digit = match byte {
                b'0'..=b'9' => u32::from(byte - b'0'),
                b'a'..=b'z' => u32::from(byte - b'a') + 10,
                b'A'..=b'Z' => u32::from(byte - b'A') + 10,
                _ => unreachable!(),
            };
            result = result.mul_small_abs(radix);
            result.add_small_abs(digit);
        }
        result.negative = negative && !result.is_zero();
        Ok(result)
    }

    pub fn format(&self, radix: u32, min_digits: usize, group: usize) -> Result<String, Error> {
        if !(2..=36).contains(&radix) {
            return Err(Error::InvalidRadix { radix });
        }

        let alphabet = b"0123456789abcdefghijklmnopqrstuvwxyz";
        let mut value = self.abs();
        let mut digits = Vec::new();
        if value.is_zero() {
            digits.push(b'0');
        } else {
            while !value.is_zero() {
                let (quotient, remainder) = value.div_rem_small_abs(radix);
                digits.push(alphabet[remainder as usize]);
                value = quotient;
            }
            digits.reverse();
        }

        while digits.len() < min_digits {
            digits.insert(0, b'0');
        }

        let mut output = String::new();
        if self.negative && !self.is_zero() {
            output.push('-');
        }
        for (i, &digit) in digits.iter().enumerate() {
            if group > 0 && i > 0 && (digits.len() - i) % group == 0 {
                output.push('_');
            }
            output.push(char::from(digit));
        }
        Ok(output)
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.negative, rhs.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => self.abs_cmp(rhs),
            (true, true) => rhs.abs_cmp(self),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            let mut result = self.abs_add(rhs);
            result.negative = self.negative && !result.is_zero();
            result
        } else {
            match self.abs_cmp(rhs) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => {
                    let mut result = self.abs_sub(rhs);
                    result.negative = self.negative && !result.is_zero();
                    result
                }
                Ordering::Less => {
                    let mut result = rhs.abs_sub(self);
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
        if self.is_zero() || rhs.is_zero() {
            return Self::zero();
        }

        let mut limbs = vec![0u32; self.limbs.len() + rhs.limbs.len()];
        for (i, &a) in self.limbs.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &b) in rhs.limbs.iter().enumerate() {
                let index = i + j;
                let current =
                    u64::from(limbs[index]) + u64::from(a) * u64::from(b) + carry;
                limbs[index] = (current % Self::BASE) as u32;
                carry = current / Self::BASE;
            }
            let mut index = i + rhs.limbs.len();
            while carry != 0 {
                if index == limbs.len() {
                    limbs.push(0);
                }
                let current = u64::from(limbs[index]) + carry;
                limbs[index] = (current % Self::BASE) as u32;
                carry = current / Self::BASE;
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

    fn shift_base_add(&mut self, limb: u32) {
        if self.is_zero() {
            self.limbs[0] = limb;
        } else {
            self.limbs.insert(0, limb);
        }
        self.normalize();
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }

        let dividend = self.abs();
        let divisor = rhs.abs();
        if dividend.abs_cmp(&divisor) == Ordering::Less {
            return Ok((Self::zero(), self.clone()));
        }

        let mut quotient_limbs = vec![0u32; dividend.limbs.len()];
        let mut remainder = Self::zero();

        for i in (0..dividend.limbs.len()).rev() {
            remainder.shift_base_add(dividend.limbs[i]);

            let mut low = 0u64;
            let mut high = Self::BASE - 1;
            let mut best = 0u32;
            while low <= high {
                let mid = (low + high) / 2;
                let product = divisor.mul_small_abs(mid as u32);
                match product.abs_cmp(&remainder) {
                    Ordering::Greater => {
                        if mid == 0 {
                            break;
                        }
                        high = mid - 1;
                    }
                    _ => {
                        best = mid as u32;
                        low = mid + 1;
                    }
                }
            }
            quotient_limbs[i] = best;
            if best != 0 {
                remainder = remainder.abs_sub(&divisor.mul_small_abs(best));
            }
        }

        let mut quotient = Self {
            negative: self.negative != rhs.negative,
            limbs: quotient_limbs,
        };
        quotient.normalize();
        remainder.negative = self.negative && !remainder.is_zero();
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
        let mut a = self.abs();
        let mut b = rhs.abs();
        while !b.is_zero() {
            let (_, remainder) = a.div_rem(&b).expect("nonzero divisor");
            a = b;
            b = remainder.abs();
        }
        a
    }
}

fn main() {}