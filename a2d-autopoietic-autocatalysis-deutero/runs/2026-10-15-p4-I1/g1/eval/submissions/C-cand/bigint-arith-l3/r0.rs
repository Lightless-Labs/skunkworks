use std::cmp::Ordering;

const BASE: u64 = 1_000_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Empty,
    SignOnly,
    InvalidDigit { index: usize },
    Separator { index: usize },
    LeadingZero { index: usize },
    TooManyDigits { limit: usize, index: usize },
    DivisionByZero,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    negative: bool,
    // Little-endian base-1,000,000,000 limbs. Zero is represented by [0].
    limbs: Vec<u32>,
}

impl Big {
    pub fn parse(text: &str, max_digits: usize) -> Result<Self, Error> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(Error::Empty);
        }

        let start = if bytes[0] == b'+' || bytes[0] == b'-' {
            1
        } else {
            0
        };

        if start == bytes.len() {
            return Err(Error::SignOnly);
        }

        // Invalid bytes take precedence over malformed separators.
        if let Some(index) = (start..bytes.len())
            .find(|&i| bytes[i] != b'_' && !bytes[i].is_ascii_digit())
        {
            return Err(Error::InvalidDigit { index });
        }

        // Underscores must have a digit on each side within the magnitude.
        if let Some(index) = (start..bytes.len()).find(|&i| {
            bytes[i] == b'_'
                && (i == start
                    || i + 1 == bytes.len()
                    || !bytes[i - 1].is_ascii_digit()
                    || !bytes[i + 1].is_ascii_digit())
        }) {
            return Err(Error::Separator { index });
        }

        let digits: Vec<(usize, u8)> = (start..bytes.len())
            .filter(|&i| bytes[i] != b'_')
            .map(|i| (i, bytes[i]))
            .collect();

        if digits.len() > 1 && digits[0].1 == b'0' {
            return Err(Error::LeadingZero { index: digits[0].0 });
        }

        if digits.len() > max_digits {
            let index = digits[max_digits].0;
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index,
            });
        }

        let mut result = Self::zero();
        for &(_, digit) in &digits {
            result.mul_small_assign(10);
            result.add_small_assign((digit - b'0') as u32);
        }

        result.negative = bytes[0] == b'-' && !result.is_zero();
        Ok(result)
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let mut magnitude = self.magnitude_string();
        let width = min_digits.max(magnitude.len());
        if width > magnitude.len() {
            let mut padded = String::with_capacity(width);
            padded.extend(std::iter::repeat_n('0', width - magnitude.len()));
            padded.push_str(&magnitude);
            magnitude = padded;
        }

        let mut output = String::new();
        if self.negative && !self.is_zero() {
            output.push('-');
        }

        if group == 0 || magnitude.len() <= group {
            output.push_str(&magnitude);
            return output;
        }

        let first_group = {
            let remainder = magnitude.len() % group;
            if remainder == 0 { group } else { remainder }
        };
        output.push_str(&magnitude[..first_group]);
        for chunk in magnitude[first_group..].as_bytes().chunks(group) {
            output.push('_');
            output.push_str(std::str::from_utf8(chunk).expect("ASCII digits"));
        }
        output
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.negative, rhs.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => self.cmp_magnitude(rhs),
            (true, true) => self.cmp_magnitude(rhs).reverse(),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            let mut result = Self {
                negative: self.negative,
                limbs: Self::add_magnitudes(&self.limbs, &rhs.limbs),
            };
            result.normalize();
            result
        } else {
            match self.cmp_magnitude(rhs) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => {
                    let mut result = Self {
                        negative: self.negative,
                        limbs: Self::sub_magnitudes(&self.limbs, &rhs.limbs),
                    };
                    result.normalize();
                    result
                }
                Ordering::Less => {
                    let mut result = Self {
                        negative: rhs.negative,
                        limbs: Self::sub_magnitudes(&rhs.limbs, &self.limbs),
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

        let mut limbs = vec![0u32; self.limbs.len() + rhs.limbs.len()];
        for (i, &a) in self.limbs.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &b) in rhs.limbs.iter().enumerate() {
                let index = i + j;
                let value = limbs[index] as u64 + (a as u64) * (b as u64) + carry;
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

        let (mut quotient, mut remainder) =
            Self::div_rem_magnitudes(&self.limbs, &rhs.limbs);

        let mut q = Self {
            negative: self.negative != rhs.negative,
            limbs: std::mem::take(&mut quotient),
        };
        q.normalize();

        let mut r = Self {
            negative: self.negative,
            limbs: remainder,
        };
        r.normalize();
        Ok((q, r))
    }

    fn zero() -> Self {
        Self {
            negative: false,
            limbs: vec![0],
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

    fn cmp_magnitude(&self, rhs: &Self) -> Ordering {
        Self::cmp_magnitude_limbs(&self.limbs, &rhs.limbs)
    }

    fn cmp_magnitude_limbs(a: &[u32], b: &[u32]) -> Ordering {
        match a.len().cmp(&b.len()) {
            Ordering::Equal => {
                for (x, y) in a.iter().rev().zip(b.iter().rev()) {
                    match x.cmp(y) {
                        Ordering::Equal => {}
                        other => return other,
                    }
                }
                Ordering::Equal
            }
            other => other,
        }
    }

    fn add_magnitudes(a: &[u32], b: &[u32]) -> Vec<u32> {
        let mut result = Vec::with_capacity(a.len().max(b.len()) + 1);
        let mut carry = 0u64;
        for i in 0..a.len().max(b.len()) {
            let sum = a.get(i).copied().unwrap_or(0) as u64
                + b.get(i).copied().unwrap_or(0) as u64
                + carry;
            result.push((sum % BASE) as u32);
            carry = sum / BASE;
        }
        if carry != 0 {
            result.push(carry as u32);
        }
        result
    }

    // Requires a >= b.
    fn sub_magnitudes(a: &[u32], b: &[u32]) -> Vec<u32> {
        let mut result = Vec::with_capacity(a.len());
        let mut borrow = 0i64;
        for (i, &limb) in a.iter().enumerate() {
            let mut value = limb as i64 - b.get(i).copied().unwrap_or(0) as i64 - borrow;
            if value < 0 {
                value += BASE as i64;
                borrow = 1;
            } else {
                borrow = 0;
            }
            result.push(value as u32);
        }
        while result.len() > 1 && result.last() == Some(&0) {
            result.pop();
        }
        result
    }

    fn mul_small_assign(&mut self, factor: u32) {
        let mut carry = 0u64;
        for limb in &mut self.limbs {
            let value = *limb as u64 * factor as u64 + carry;
            *limb = (value % BASE) as u32;
            carry = value / BASE;
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
            self.limbs[i] = (sum % BASE) as u32;
            carry = sum / BASE;
            i += 1;
        }
    }

    fn mul_small(a: &[u32], factor: u32) -> Vec<u32> {
        let mut result = Vec::with_capacity(a.len() + 1);
        let mut carry = 0u64;
        for &limb in a {
            let value = limb as u64 * factor as u64 + carry;
            result.push((value % BASE) as u32);
            carry = value / BASE;
        }
        if carry != 0 {
            result.push(carry as u32);
        }
        result
    }

    fn add_small(a: &[u32], value: u32) -> Vec<u32> {
        let mut result = a.to_vec();
        let mut carry = value as u64;
        let mut i = 0;
        while carry != 0 {
            if i == result.len() {
                result.push(0);
            }
            let sum = result[i] as u64 + carry;
            result[i] = (sum % BASE) as u32;
            carry = sum / BASE;
            i += 1;
        }
        result
    }

    fn sub_assign_limbs(a: &mut Vec<u32>, b: &[u32]) {
        let mut borrow = 0i64;
        for i in 0..a.len() {
            let mut value = a[i] as i64 - b.get(i).copied().unwrap_or(0) as i64 - borrow;
            if value < 0 {
                value += BASE as i64;
                borrow = 1;
            } else {
                borrow = 0;
            }
            a[i] = value as u32;
        }
        while a.len() > 1 && a.last() == Some(&0) {
            a.pop();
        }
    }

    fn div_rem_magnitudes(dividend: &[u32], divisor: &[u32]) -> (Vec<u32>, Vec<u32>) {
        if Self::cmp_magnitude_limbs(dividend, divisor) == Ordering::Less {
            return (vec![0], dividend.to_vec());
        }

        let mut quotient = vec![0u32; dividend.len()];
        let mut remainder = vec![0u32];

        for i in (0..dividend.len()).rev() {
            remainder = Self::mul_small(&remainder, BASE as u32);
            remainder = Self::add_small(&remainder, dividend[i]);

            let mut low = 0u64;
            let mut high = BASE - 1;
            let mut best = 0u32;
            while low <= high {
                let mid = (low + high) / 2;
                let product = Self::mul_small(divisor, mid as u32);
                match Self::cmp_magnitude_limbs(&product, &remainder) {
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

            quotient[i] = best;
            if best != 0 {
                let product = Self::mul_small(divisor, best);
                Self::sub_assign_limbs(&mut remainder, &product);
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

    fn magnitude_string(&self) -> String {
        let mut iter = self.limbs.iter().rev();
        let mut result = iter
            .next()
            .copied()
            .unwrap_or(0)
            .to_string();
        for limb in iter {
            use std::fmt::Write;
            write!(&mut result, "{limb:09}").expect("writing to String");
        }
        result
    }
}

fn main() {}