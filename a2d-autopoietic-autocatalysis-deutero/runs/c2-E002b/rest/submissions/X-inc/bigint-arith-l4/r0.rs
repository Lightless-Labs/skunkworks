use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    negative: bool,
    digits: Vec<u8>,
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
        Self { negative: false, digits: vec![0] }
    }

    fn one() -> Self {
        Self { negative: false, digits: vec![1] }
    }

    fn from_parts(negative: bool, digits: Vec<u8>) -> Self {
        let mut value = Self { negative, digits };
        value.normalize();
        value
    }

    fn normalize(&mut self) {
        let first = self.digits.iter().position(|&d| d != 0);
        if let Some(i) = first {
            if i > 0 {
                self.digits.drain(..i);
            }
        } else {
            self.digits.clear();
            self.digits.push(0);
            self.negative = false;
        }
    }

    fn is_zero(&self) -> bool {
        self.digits.len() == 1 && self.digits[0] == 0
    }

    fn magnitude_cmp(&self, rhs: &Self) -> Ordering {
        self.digits.len().cmp(&rhs.digits.len())
            .then_with(|| self.digits.cmp(&rhs.digits))
    }

    fn add_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(a.len().max(b.len()) + 1);
        let (mut i, mut j, mut carry) = (a.len(), b.len(), 0u8);
        while i > 0 || j > 0 || carry > 0 {
            let av = if i > 0 { i -= 1; a[i] } else { 0 };
            let bv = if j > 0 { j -= 1; b[j] } else { 0 };
            let sum = av + bv + carry;
            out.push(sum % 10);
            carry = sum / 10;
        }
        out.reverse();
        out
    }

    // Requires a >= b.
    fn sub_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(a.len());
        let (mut i, mut j, mut borrow) = (a.len(), b.len(), 0i8);
        while i > 0 {
            i -= 1;
            let av = a[i] as i8 - borrow;
            let bv = if j > 0 { j -= 1; b[j] as i8 } else { 0 };
            if av < bv {
                out.push((av + 10 - bv) as u8);
                borrow = 1;
            } else {
                out.push((av - bv) as u8);
                borrow = 0;
            }
        }
        out.reverse();
        let first = out.iter().position(|&d| d != 0).unwrap_or(out.len() - 1);
        out.drain(..first);
        out
    }

    fn mul_small_mag(a: &[u8], n: u8) -> Vec<u8> {
        if n == 0 { return vec![0]; }
        let mut out = Vec::with_capacity(a.len() + 1);
        let mut carry = 0u16;
        for &d in a.iter().rev() {
            let p = d as u16 * n as u16 + carry;
            out.push((p % 10) as u8);
            carry = p / 10;
        }
        while carry > 0 {
            out.push((carry % 10) as u8);
            carry /= 10;
        }
        out.reverse();
        out
    }

    fn div_rem_magnitude(a: &[u8], b: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let mut quotient = Vec::with_capacity(a.len());
        let mut remainder = vec![0];
        for &digit in a {
            if remainder.len() == 1 && remainder[0] == 0 {
                remainder[0] = digit;
            } else {
                remainder.push(digit);
            }
            let first = remainder.iter().position(|&d| d != 0).unwrap_or(remainder.len() - 1);
            if first > 0 { remainder.drain(..first); }

            let mut qdigit = 0u8;
            while Self::cmp_mag_slices(&remainder, b) != Ordering::Less {
                remainder = Self::sub_magnitude(&remainder, b);
                qdigit += 1;
            }
            quotient.push(qdigit);
        }
        let first = quotient.iter().position(|&d| d != 0).unwrap_or(quotient.len().saturating_sub(1));
        if first > 0 { quotient.drain(..first); }
        if quotient.is_empty() { quotient.push(0); }
        (quotient, remainder)
    }

    fn cmp_mag_slices(a: &[u8], b: &[u8]) -> Ordering {
        let ai = a.iter().position(|&d| d != 0).unwrap_or(a.len().saturating_sub(1));
        let bi = b.iter().position(|&d| d != 0).unwrap_or(b.len().saturating_sub(1));
        let aa = &a[ai..];
        let bb = &b[bi..];
        aa.len().cmp(&bb.len()).then_with(|| aa.cmp(bb))
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
        let mut digit_values = vec![None; bytes.len()];
        for i in start..bytes.len() {
            if bytes[i] == b'_' { continue; }
            let v = match bytes[i] {
                b'0'..=b'9' => Some(bytes[i] - b'0'),
                b'a'..=b'z' => Some(bytes[i] - b'a' + 10),
                b'A'..=b'Z' => Some(bytes[i] - b'A' + 10),
                _ => None,
            };
            match v {
                Some(d) if (d as u32) < radix => digit_values[i] = Some(d),
                _ => if invalid.is_none() { invalid = Some(i); },
            }
        }
        if let Some(index) = invalid {
            return Err(Error::InvalidDigit { index });
        }

        let mut separator = None;
        for i in start..bytes.len() {
            if bytes[i] == b'_' {
                let prev_ok = i > start && digit_values[i - 1].is_some();
                let next_ok = i + 1 < bytes.len() && digit_values[i + 1].is_some();
                if (!prev_ok || !next_ok) && separator.is_none() {
                    separator = Some(i);
                }
            }
        }
        if let Some(index) = separator {
            return Err(Error::Separator { index });
        }

        let digit_indices: Vec<usize> = (start..bytes.len())
            .filter(|&i| bytes[i] != b'_')
            .collect();

        if digit_indices.len() > 1 && digit_values[digit_indices[0]] == Some(0) {
            return Err(Error::LeadingZero { index: digit_indices[0] });
        }
        if digit_indices.len() > max_digits {
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: digit_indices[max_digits],
            });
        }

        let mut value = Self::zero();
        for i in digit_indices {
            value.digits = Self::add_magnitude(
                &Self::mul_small_mag(&value.digits, radix as u8),
                &[digit_values[i].unwrap()],
            );
        }
        value.negative = negative && !value.is_zero();
        Ok(value)
    }

    pub fn format(&self, radix: u32, min_digits: usize, group: usize) -> Result<String, Error> {
        if !(2..=36).contains(&radix) {
            return Err(Error::InvalidRadix { radix });
        }
        let mut n = self.digits.clone();
        let mut encoded = Vec::new();

        if n.len() == 1 && n[0] == 0 {
            encoded.push(0);
        } else {
            while !(n.len() == 1 && n[0] == 0) {
                let mut quotient = Vec::with_capacity(n.len());
                let mut rem = 0u32;
                for &d in &n {
                    let cur = rem * 10 + d as u32;
                    let q = cur / radix;
                    rem = cur % radix;
                    if !quotient.is_empty() || q != 0 {
                        quotient.push(q as u8);
                    }
                }
                encoded.push(rem as u8);
                n = if quotient.is_empty() { vec![0] } else { quotient };
            }
            encoded.reverse();
        }

        while encoded.len() < min_digits.max(1) {
            encoded.insert(0, 0);
        }
        let mut body = String::new();
        for (i, &d) in encoded.iter().enumerate() {
            if group > 0 && i > 0 && (encoded.len() - i) % group == 0 {
                body.push('_');
            }
            body.push(if d < 10 { (b'0' + d) as char } else { (b'a' + d - 10) as char });
        }
        if self.negative && !self.is_zero() {
            body.insert(0, '-');
        }
        Ok(body)
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.negative, rhs.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => self.magnitude_cmp(rhs),
            (true, true) => rhs.magnitude_cmp(self),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            Self::from_parts(self.negative, Self::add_magnitude(&self.digits, &rhs.digits))
        } else {
            match self.magnitude_cmp(rhs) {
                Ordering::Greater => Self::from_parts(self.negative, Self::sub_magnitude(&self.digits, &rhs.digits)),
                Ordering::Less => Self::from_parts(rhs.negative, Self::sub_magnitude(&rhs.digits, &self.digits)),
                Ordering::Equal => Self::zero(),
            }
        }
    }

    pub fn sub(&self, rhs: &Self) -> Self {
        self.add(&rhs.neg())
    }

    pub fn neg(&self) -> Self {
        let mut out = self.clone();
        if !out.is_zero() {
            out.negative = !out.negative;
        }
        out
    }

    pub fn signum(&self) -> i8 {
        if self.is_zero() { 0 } else if self.negative { -1 } else { 1 }
    }

    pub fn mul(&self, rhs: &Self) -> Self {
        if self.is_zero() || rhs.is_zero() { return Self::zero(); }
        let mut out = vec![0u8; self.digits.len() + rhs.digits.len()];
        for i in (0..self.digits.len()).rev() {
            let mut carry = 0u32;
            for j in (0..rhs.digits.len()).rev() {
                let k = i + j + 1;
                let total = out[k] as u32 + self.digits[i] as u32 * rhs.digits[j] as u32 + carry;
                out[k] = (total % 10) as u8;
                carry = total / 10;
            }
            let mut k = i;
            while carry > 0 {
                let total = out[k] as u32 + carry;
                out[k] = (total % 10) as u8;
                carry = total / 10;
                if k == 0 { break; }
                k -= 1;
            }
        }
        Self::from_parts(self.negative ^ rhs.negative, out)
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() { return Err(Error::DivisionByZero); }
        let (q, r) = Self::div_rem_magnitude(&self.digits, &rhs.digits);
        Ok((
            Self::from_parts(self.negative ^ rhs.negative, q),
            Self::from_parts(self.negative, r),
        ))
    }

    pub fn pow(&self, exponent: u32) -> Result<Self, Error> {
        const LIMIT: u32 = 1024;
        if exponent > LIMIT {
            return Err(Error::ExponentTooLarge { exponent, limit: LIMIT });
        }
        let mut e = exponent;
        let mut base = self.clone();
        let mut result = Self::one();
        while e > 0 {
            if e & 1 == 1 {
                result = result.mul(&base);
            }
            e >>= 1;
            if e > 0 {
                base = base.mul(&base);
            }
        }
        Ok(result)
    }

    pub fn gcd(&self, rhs: &Self) -> Self {
        let mut a = Self::from_parts(false, self.digits.clone());
        let mut b = Self::from_parts(false, rhs.digits.clone());
        while !b.is_zero() {
            let r = a.div_rem(&b).expect("nonzero divisor").1;
            a = b;
            b = r;
        }
        a
    }
}