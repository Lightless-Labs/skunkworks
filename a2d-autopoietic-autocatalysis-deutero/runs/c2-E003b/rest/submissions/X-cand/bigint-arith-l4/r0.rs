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
    // Little-endian base-2^32 limbs. Zero is represented by an empty vector.
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
        Self {
            negative: false,
            limbs: self.limbs.clone(),
        }
    }

    fn magnitude_cmp(&self, rhs: &Self) -> Ordering {
        match self.limbs.len().cmp(&rhs.limbs.len()) {
            Ordering::Equal => {
                for (a, b) in self.limbs.iter().rev().zip(rhs.limbs.iter().rev()) {
                    match a.cmp(b) {
                        Ordering::Equal => {}
                        ord => return ord,
                    }
                }
                Ordering::Equal
            }
            ord => ord,
        }
    }

    fn magnitude_add(a: &[u32], b: &[u32]) -> Vec<u32> {
        let n = a.len().max(b.len());
        let mut out = Vec::with_capacity(n.saturating_add(1));
        let mut carry = 0u64;
        for i in 0..n {
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

    // Requires a >= b.
    fn magnitude_sub(a: &[u32], b: &[u32]) -> Vec<u32> {
        let mut out = Vec::with_capacity(a.len());
        let mut borrow = 0u64;
        for (i, &av) in a.iter().enumerate() {
            let bv = b.get(i).copied().unwrap_or(0) as u64;
            let lhs = av as u64;
            let subtrahend = bv + borrow;
            if lhs >= subtrahend {
                out.push((lhs - subtrahend) as u32);
                borrow = 0;
            } else {
                out.push(((1u64 << 32) + lhs - subtrahend) as u32);
                borrow = 1;
            }
        }
        while out.last() == Some(&0) {
            out.pop();
        }
        out
    }

    fn mul_small(&mut self, n: u32) {
        if n == 0 || self.is_zero() {
            self.limbs.clear();
            self.negative = false;
            return;
        }
        let mut carry = 0u64;
        for limb in &mut self.limbs {
            let v = (*limb as u64) * (n as u64) + carry;
            *limb = v as u32;
            carry = v >> 32;
        }
        if carry != 0 {
            self.limbs.push(carry as u32);
        }
    }

    fn add_small(&mut self, n: u32) {
        let mut carry = n as u64;
        let mut i = 0;
        while carry != 0 {
            if i == self.limbs.len() {
                self.limbs.push(0);
            }
            let v = self.limbs[i] as u64 + carry;
            self.limbs[i] = v as u32;
            carry = v >> 32;
            i += 1;
        }
    }

    fn div_rem_small(&self, divisor: u32) -> (Self, u32) {
        let mut quotient = vec![0; self.limbs.len()];
        let mut rem = 0u64;
        for i in (0..self.limbs.len()).rev() {
            let value = (rem << 32) | self.limbs[i] as u64;
            quotient[i] = (value / divisor as u64) as u32;
            rem = value % divisor as u64;
        }
        let mut q = Self {
            negative: false,
            limbs: quotient,
        };
        q.normalize();
        (q, rem as u32)
    }

    fn div_rem_magnitude(dividend: &Self, divisor: &Self) -> (Self, Self) {
        debug_assert!(!divisor.is_zero());

        if dividend.magnitude_cmp(divisor) == Ordering::Less {
            return (Self::zero(), dividend.abs());
        }

        if divisor.limbs.len() == 1 {
            let (q, r) = dividend.div_rem_small(divisor.limbs[0]);
            return (
                q,
                Self {
                    negative: false,
                    limbs: if r == 0 { Vec::new() } else { vec![r] },
                },
            );
        }

        let mut quotient = vec![0u32; dividend.limbs.len()];
        let mut remainder = Self::zero();

        for i in (0..dividend.limbs.len()).rev() {
            if !remainder.is_zero() {
                remainder.limbs.insert(0, dividend.limbs[i]);
            } else if dividend.limbs[i] != 0 {
                remainder.limbs.push(dividend.limbs[i]);
            }
            remainder.normalize();

            let mut low = 0u64;
            let mut high = u32::MAX as u64 + 1;
            while low + 1 < high {
                let mid = low + (high - low) / 2;
                let product = Self::magnitude_mul_small(&divisor.limbs, mid as u32);
                if Self::slice_cmp(&product, &remainder.limbs) != Ordering::Greater {
                    low = mid;
                } else {
                    high = mid;
                }
            }

            if low != 0 {
                quotient[i] = low as u32;
                let product = Self::magnitude_mul_small(&divisor.limbs, low as u32);
                remainder.limbs = Self::magnitude_sub(&remainder.limbs, &product);
                remainder.normalize();
            }
        }

        let mut q = Self {
            negative: false,
            limbs: quotient,
        };
        q.normalize();
        (q, remainder)
    }

    fn slice_cmp(a: &[u32], b: &[u32]) -> Ordering {
        match a.len().cmp(&b.len()) {
            Ordering::Equal => {
                for (x, y) in a.iter().rev().zip(b.iter().rev()) {
                    match x.cmp(y) {
                        Ordering::Equal => {}
                        ord => return ord,
                    }
                }
                Ordering::Equal
            }
            ord => ord,
        }
    }

    fn magnitude_mul_small(a: &[u32], n: u32) -> Vec<u32> {
        if n == 0 || a.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(a.len() + 1);
        let mut carry = 0u64;
        for &limb in a {
            let v = limb as u64 * n as u64 + carry;
            out.push(v as u32);
            carry = v >> 32;
        }
        if carry != 0 {
            out.push(carry as u32);
        }
        out
    }

    fn magnitude_mul(a: &[u32], b: &[u32]) -> Vec<u32> {
        if a.is_empty() || b.is_empty() {
            return Vec::new();
        }
        let mut out = vec![0u32; a.len() + b.len()];
        for (i, &av) in a.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &bv) in b.iter().enumerate() {
                let k = i + j;
                let value = out[k] as u64 + av as u64 * bv as u64 + carry;
                out[k] = value as u32;
                carry = value >> 32;
            }
            let mut k = i + b.len();
            while carry != 0 {
                if k == out.len() {
                    out.push(0);
                }
                let value = out[k] as u64 + carry;
                out[k] = value as u32;
                carry = value >> 32;
                k += 1;
            }
        }
        while out.last() == Some(&0) {
            out.pop();
        }
        out
    }

    fn digit_value(byte: u8) -> Option<u32> {
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

        let mut first_invalid = None;
        for i in start..bytes.len() {
            if bytes[i] == b'_' {
                continue;
            }
            if Self::digit_value(bytes[i]).is_none_or(|d| d >= radix) {
                first_invalid = Some(i);
                break;
            }
        }
        if let Some(index) = first_invalid {
            return Err(Error::InvalidDigit { index });
        }

        for i in start..bytes.len() {
            if bytes[i] == b'_' {
                let prev_valid = i > start
                    && bytes[i - 1] != b'_'
                    && Self::digit_value(bytes[i - 1]).is_some_and(|d| d < radix);
                let next_valid = i + 1 < bytes.len()
                    && bytes[i + 1] != b'_'
                    && Self::digit_value(bytes[i + 1]).is_some_and(|d| d < radix);
                if !prev_valid || !next_valid {
                    return Err(Error::Separator { index: i });
                }
            }
        }

        let digit_count = bytes[start..].iter().filter(|&&b| b != b'_').count();
        let first_digit_index = start;
        let first_digit = bytes[start..].iter().copied().find(|&b| b != b'_');

        if digit_count > 1 && first_digit == Some(b'0') {
            return Err(Error::LeadingZero {
                index: first_digit_index,
            });
        }

        if digit_count > max_digits {
            let excess_number = max_digits.saturating_add(1);
            let mut seen = 0usize;
            let mut index = start;
            for i in start..bytes.len() {
                if bytes[i] != b'_' {
                    seen += 1;
                    if seen == excess_number {
                        index = i;
                        break;
                    }
                }
            }
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index,
            });
        }

        let mut value = Self::zero();
        for &byte in &bytes[start..] {
            if byte == b'_' {
                continue;
            }
            value.mul_small(radix);
            value.add_small(Self::digit_value(byte).unwrap());
        }
        value.negative = negative && !value.is_zero();
        Ok(value)
    }

    pub fn format(&self, radix: u32, min_digits: usize, group: usize) -> Result<String, Error> {
        if !(2..=36).contains(&radix) {
            return Err(Error::InvalidRadix { radix });
        }

        let alphabet = b"0123456789abcdefghijklmnopqrstuvwxyz";
        let mut n = self.abs();
        let mut digits = Vec::new();

        if n.is_zero() {
            digits.push(b'0');
        } else {
            while !n.is_zero() {
                let (q, r) = n.div_rem_small(radix);
                digits.push(alphabet[r as usize]);
                n = q;
            }
            digits.reverse();
        }

        if digits.len() < min_digits {
            let padding = min_digits - digits.len();
            let mut padded = Vec::with_capacity(min_digits);
            padded.resize(padding, b'0');
            padded.extend_from_slice(&digits);
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
            out.push(digit as char);
        }
        Ok(out)
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.negative, rhs.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => self.magnitude_cmp(rhs),
            (true, true) => self.magnitude_cmp(rhs).reverse(),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            let mut result = Self {
                negative: self.negative,
                limbs: Self::magnitude_add(&self.limbs, &rhs.limbs),
            };
            result.normalize();
            result
        } else {
            match self.magnitude_cmp(rhs) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => {
                    let mut result = Self {
                        negative: self.negative,
                        limbs: Self::magnitude_sub(&self.limbs, &rhs.limbs),
                    };
                    result.normalize();
                    result
                }
                Ordering::Less => {
                    let mut result = Self {
                        negative: rhs.negative,
                        limbs: Self::magnitude_sub(&rhs.limbs, &self.limbs),
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
        let mut result = Self {
            negative: self.negative ^ rhs.negative,
            limbs: Self::magnitude_mul(&self.limbs, &rhs.limbs),
        };
        result.normalize();
        result
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }
        let (mut quotient, mut remainder) =
            Self::div_rem_magnitude(&self.abs(), &rhs.abs());
        quotient.negative = (self.negative ^ rhs.negative) && !quotient.is_zero();
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

        let mut result = Self::one();
        let mut base = self.clone();
        let mut exp = exponent;

        while exp != 0 {
            if exp & 1 == 1 {
                result = result.mul(&base);
            }
            exp >>= 1;
            if exp != 0 {
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