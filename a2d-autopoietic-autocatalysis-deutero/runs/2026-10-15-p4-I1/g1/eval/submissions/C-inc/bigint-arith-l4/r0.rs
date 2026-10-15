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
    // Little-endian base 10^9 limbs. Zero is represented by an empty vector.
    limbs: Vec<u32>,
}

impl Big {
    const BASE: u64 = 1_000_000_000;
    const POW_LIMIT: u32 = 1024;

    pub fn parse(text: &str, radix: u32, max_digits: usize) -> Result<Self, Error> {
        if !(2..=36).contains(&radix) {
            return Err(Error::InvalidRadix { radix });
        }
        if text.is_empty() {
            return Err(Error::Empty);
        }

        let bytes = text.as_bytes();
        let sign_len = usize::from(bytes[0] == b'+' || bytes[0] == b'-');
        let negative = sign_len == 1 && bytes[0] == b'-';
        if sign_len == bytes.len() {
            return Err(Error::SignOnly);
        }

        let magnitude = &bytes[sign_len..];
        let digit_value = |b: u8| -> Option<u32> {
            match b {
                b'0'..=b'9' => Some((b - b'0') as u32),
                b'a'..=b'z' => Some((b - b'a') as u32 + 10),
                b'A'..=b'Z' => Some((b - b'A') as u32 + 10),
                _ => None,
            }
        };

        for (offset, &byte) in magnitude.iter().enumerate() {
            if byte != b'_' && !digit_value(byte).is_some_and(|d| d < radix) {
                return Err(Error::InvalidDigit {
                    index: sign_len + offset,
                });
            }
        }

        let mut first_bad_separator = None;
        for i in 0..magnitude.len() {
            if magnitude[i] == b'_' {
                let left_valid = i > 0
                    && digit_value(magnitude[i - 1]).is_some_and(|d| d < radix);
                let right_valid = i + 1 < magnitude.len()
                    && digit_value(magnitude[i + 1]).is_some_and(|d| d < radix);
                if !(left_valid && right_valid) {
                    first_bad_separator = Some(sign_len + i);
                    break;
                }
            }
        }
        if let Some(index) = first_bad_separator {
            return Err(Error::Separator { index });
        }

        let digits: Vec<(usize, u32)> = magnitude
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| digit_value(b).map(|d| (sign_len + i, d)))
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

        let mut result = Self::zero();
        for &(_, digit) in &digits {
            result.mul_small_assign(radix);
            result.add_small_assign(digit);
        }
        if !result.is_zero() {
            result.negative = negative;
        }
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

        let mut n = self.abs();
        let mut digits = Vec::new();
        if n.is_zero() {
            digits.push(0);
        } else {
            while !n.is_zero() {
                digits.push(n.div_small_assign(radix));
            }
            digits.reverse();
        }

        if digits.len() < min_digits {
            let mut padded = vec![0; min_digits - digits.len()];
            padded.extend(digits);
            digits = padded;
        }

        let mut out = String::new();
        if self.negative && !self.is_zero() {
            out.push('-');
        }
        for (i, &digit) in digits.iter().enumerate() {
            if group > 0 && i > 0 && (digits.len() - i) % group == 0 {
                out.push('_');
            }
            out.push(match digit {
                0..=9 => (b'0' + digit as u8) as char,
                _ => (b'a' + (digit - 10) as u8) as char,
            });
        }
        Ok(out)
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.signum(), rhs.signum()) {
            (a, b) if a != b => a.cmp(&b),
            (0, 0) => Ordering::Equal,
            (-1, -1) => self.cmp_magnitude(rhs).reverse(),
            _ => self.cmp_magnitude(rhs),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            let mut result = Self::add_magnitudes(self, rhs);
            result.negative = self.negative && !result.is_zero();
            result
        } else {
            match self.cmp_magnitude(rhs) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => {
                    let mut result = Self::sub_magnitudes(self, rhs);
                    result.negative = self.negative;
                    result
                }
                Ordering::Less => {
                    let mut result = Self::sub_magnitudes(rhs, self);
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
                let value = limbs[k] as u64 + a as u64 * b as u64 + carry;
                limbs[k] = (value % Self::BASE) as u32;
                carry = value / Self::BASE;
            }
            let mut k = i + rhs.limbs.len();
            while carry > 0 {
                if k == limbs.len() {
                    limbs.push(0);
                }
                let value = limbs[k] as u64 + carry;
                limbs[k] = (value % Self::BASE) as u32;
                carry = value / Self::BASE;
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
        if self.is_zero() {
            return Ok((Self::zero(), Self::zero()));
        }

        let (mut quotient, mut remainder) =
            Self::div_rem_magnitudes(&self.abs(), &rhs.abs());
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
            let (_, remainder) = Self::div_rem_magnitudes(&a, &b);
            a = b;
            b = remainder;
        }
        a
    }

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

    fn abs(&self) -> Self {
        let mut result = self.clone();
        result.negative = false;
        result
    }

    fn normalize(&mut self) {
        while self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }
        if self.limbs.is_empty() {
            self.negative = false;
        }
    }

    fn cmp_magnitude(&self, rhs: &Self) -> Ordering {
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

    fn add_magnitudes(a: &Self, b: &Self) -> Self {
        let mut limbs = Vec::with_capacity(a.limbs.len().max(b.limbs.len()) + 1);
        let mut carry = 0u64;
        for i in 0..a.limbs.len().max(b.limbs.len()) {
            let value = a.limbs.get(i).copied().unwrap_or(0) as u64
                + b.limbs.get(i).copied().unwrap_or(0) as u64
                + carry;
            limbs.push((value % Self::BASE) as u32);
            carry = value / Self::BASE;
        }
        if carry > 0 {
            limbs.push(carry as u32);
        }
        Self {
            negative: false,
            limbs,
        }
    }

    // Requires |a| >= |b|.
    fn sub_magnitudes(a: &Self, b: &Self) -> Self {
        let mut limbs = Vec::with_capacity(a.limbs.len());
        let mut borrow = 0i64;
        for i in 0..a.limbs.len() {
            let mut value = a.limbs[i] as i64
                - b.limbs.get(i).copied().unwrap_or(0) as i64
                - borrow;
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

    fn mul_small_assign(&mut self, multiplier: u32) {
        if multiplier == 0 || self.is_zero() {
            self.limbs.clear();
            self.negative = false;
            return;
        }
        let mut carry = 0u64;
        for limb in &mut self.limbs {
            let value = *limb as u64 * multiplier as u64 + carry;
            *limb = (value % Self::BASE) as u32;
            carry = value / Self::BASE;
        }
        while carry > 0 {
            self.limbs.push((carry % Self::BASE) as u32);
            carry /= Self::BASE;
        }
    }

    fn add_small_assign(&mut self, addend: u32) {
        let mut carry = addend as u64;
        let mut i = 0;
        while carry > 0 {
            if i == self.limbs.len() {
                self.limbs.push(0);
            }
            let value = self.limbs[i] as u64 + carry;
            self.limbs[i] = (value % Self::BASE) as u32;
            carry = value / Self::BASE;
            i += 1;
        }
    }

    fn div_small_assign(&mut self, divisor: u32) -> u32 {
        let mut remainder = 0u64;
        for limb in self.limbs.iter_mut().rev() {
            let value = remainder * Self::BASE + *limb as u64;
            *limb = (value / divisor as u64) as u32;
            remainder = value % divisor as u64;
        }
        self.normalize();
        remainder as u32
    }

    fn mul_small(a: &Self, multiplier: u32) -> Self {
        let mut result = a.clone();
        result.negative = false;
        result.mul_small_assign(multiplier);
        result
    }

    fn add_small(a: &Self, addend: u32) -> Self {
        let mut result = a.clone();
        result.negative = false;
        result.add_small_assign(addend);
        result
    }

    fn div_rem_magnitudes(dividend: &Self, divisor: &Self) -> (Self, Self) {
        if dividend.cmp_magnitude(divisor) == Ordering::Less {
            return (Self::zero(), dividend.abs());
        }
        let mut quotient_be = Vec::with_capacity(dividend.limbs.len());
        let mut remainder = Self::zero();
        for &limb in dividend.limbs.iter().rev() {
            remainder = Self::mul_small(&remainder, Self::BASE as u32);
            remainder = Self::add_small(&remainder, limb);
            let mut low = 0u32;
            let mut high = Self::BASE as u32 - 1;
            while low < high {
                let mid = low + ((high - low) / 2) + 1;
                let product = Self::mul_small(divisor, mid);
                if product.cmp_magnitude(&remainder) != Ordering::Greater {
                    low = mid;
                } else {
                    high = mid - 1;
                }
            }
            quotient_be.push(low);
            if low != 0 {
                let product = Self::mul_small(divisor, low);
                remainder = Self::sub_magnitudes(&remainder, &product);
            }
        }
        quotient_be.reverse();
        let mut quotient = Self {
            negative: false,
            limbs: quotient_be,
        };
        quotient.normalize();
        (quotient, remainder)
    }
}

fn main() {}