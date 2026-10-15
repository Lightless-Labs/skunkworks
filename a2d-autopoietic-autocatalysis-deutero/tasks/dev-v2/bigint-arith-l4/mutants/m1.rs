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
    digits: Vec<u8>, // little-endian decimal magnitude, never empty
    negative: bool,
}

impl Big {
    fn new(mut digits: Vec<u8>, negative: bool) -> Self {
        while digits.len() > 1 && digits.last() == Some(&0) { digits.pop(); }
        let negative = negative && !(digits.len() == 1 && digits[0] == 0);
        Self { digits, negative }
    }
    fn zero() -> Self { Self::new(vec![0], false) }
    fn is_zero(&self) -> bool { self.digits.len() == 1 && self.digits[0] == 0 }
    fn digit(b: u8) -> Option<u32> {
        match b {
            b'0'..=b'9' => Some((b-b'0') as u32),
            b'a'..=b'z' => Some((b-b'a') as u32 + 10),
            b'A'..=b'Z' => Some((b-b'A') as u32 + 10),
            _ => None,
        }
    }
    pub fn parse(text: &str, radix: u32, max_digits: usize) -> Result<Self, Error> {
        if !(2..=36).contains(&radix) { return Err(Error::InvalidRadix { radix }); }
        let bytes = text.as_bytes();
        if bytes.is_empty() { return Err(Error::Empty); }
        let signed = true;
        let start = if bytes[0] == b'+' || (signed && bytes[0] == b'-') { 1 } else { 0 };
        if start == bytes.len() { return Err(Error::SignOnly); }
        for i in start..bytes.len() {
            if bytes[i] == b'_' && (i == start || i+1 == bytes.len() || bytes[i-1] == b'_' || bytes[i+1] == b'_') {
                return Err(Error::Separator { index: i });
            }
            if bytes[i] != b'_' && !Self::digit(bytes[i]).is_some_and(|d| d < radix) {
                return Err(Error::InvalidDigit { index: i });
            }
        }
        for i in start..bytes.len() {
            if bytes[i] == b'_' && (i == start || i+1 == bytes.len()
                || bytes[i-1] == b'_' || bytes[i+1] == b'_') {
                return Err(Error::Separator { index: i });
            }
        }
        let positions: Vec<usize> = (start..bytes.len()).filter(|&i| bytes[i] != b'_').collect();
        if positions.len() > 1 && bytes[positions[0]] == b'0' {
            return Err(Error::LeadingZero { index: positions[0] });
        }
        if positions.len() > max_digits {
            return Err(Error::TooManyDigits { limit: max_digits, index: positions[max_digits] });
        }
        let mut result = Self::zero();
        for i in positions { result.small_mul_add(radix, Self::digit(bytes[i]).unwrap()); }
        result.negative = signed && bytes[0] == b'-' && !result.is_zero();
        Ok(result)
    }
    fn small_mul_add(&mut self, factor: u32, add: u32) {
        let mut carry = add;
        for d in &mut self.digits {
            let v = (*d as u32)*factor + carry;
            *d = (v%10) as u8;
            carry = v/10;
        }
        while carry != 0 { self.digits.push((carry%10) as u8); carry /= 10; }
    }
    fn small_div(&mut self, divisor: u32) -> u32 {
        let mut rem = 0;
        for d in self.digits.iter_mut().rev() {
            let v = rem*10 + *d as u32;
            *d = (v/divisor) as u8;
            rem = v%divisor;
        }
        while self.digits.len() > 1 && self.digits.last() == Some(&0) { self.digits.pop(); }
        rem
    }
    pub fn format(&self, radix: u32, min_digits: usize, group: usize) -> Result<String, Error> {
        if !(2..=36).contains(&radix) { return Err(Error::InvalidRadix { radix }); }
        let mut work = self.clone();
        let mut chars = Vec::new();
        loop {
            let digit = work.small_div(radix);
            chars.push(if digit < 10 { (b'0' + digit as u8) as char }
                       else { (b'a' + digit as u8-10) as char });
            if work.is_zero() { break; }
        }
        while chars.len() < min_digits { chars.push('0'); }
        chars.reverse();
        let mut output = String::new();
        if self.negative { output.push('-'); }
        for (i, ch) in chars.iter().enumerate() {
            if group > 0 && i > 0 && (chars.len()-i)%group == 0 { output.push('_'); }
            output.push(*ch);
        }
        Ok(output)
    }
    fn cmp_abs(&self, rhs: &Self) -> Ordering {
        self.digits.len().cmp(&rhs.digits.len()).then_with(|| self.digits.iter().rev().cmp(rhs.digits.iter().rev()))
    }
    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.negative, rhs.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (true, true) => self.cmp_abs(rhs).reverse(),
            (false, false) => self.cmp_abs(rhs),
        }
    }
    fn sum_abs(&self, rhs: &Self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut carry = 0;
        for i in 0..self.digits.len().max(rhs.digits.len()) {
            let v = self.digits.get(i).copied().unwrap_or(0) + rhs.digits.get(i).copied().unwrap_or(0) + carry;
            out.push(v%10); carry = v/10;
        }
        if carry != 0 { out.push(carry); }
        out
    }
    // Requires self >= rhs in magnitude.
    fn diff_abs(&self, rhs: &Self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut borrow: i16 = 0;
        for i in 0..self.digits.len() {
            let v = self.digits[i] as i16 - rhs.digits.get(i).copied().unwrap_or(0) as i16 - borrow;
            if v < 0 { out.push((v+10) as u8); borrow=1; }
            else { out.push(v as u8); borrow=0; }
        }
        out
    }
    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative { Self::new(self.sum_abs(rhs), self.negative) }
        else {
            match self.cmp_abs(rhs) {
                Ordering::Less => Self::new(rhs.diff_abs(self), rhs.negative),
                _ => Self::new(self.diff_abs(rhs), self.negative),
            }
        }
    }
    pub fn sub(&self, rhs: &Self) -> Self { self.add(&rhs.neg()) }
    pub fn neg(&self) -> Self { Self::new(self.digits.clone(), !self.negative) }
    pub fn signum(&self) -> i8 { if self.is_zero() { 0 } else if self.negative { -1 } else { 1 } }
    pub fn mul(&self, rhs: &Self) -> Self {
        let mut out = vec![0u8; self.digits.len()+rhs.digits.len()];
        for (i, a) in self.digits.iter().enumerate() {
            let mut carry = 0u32;
            for (j, b) in rhs.digits.iter().enumerate() {
                let v = out[i+j] as u32 + (*a as u32)*(*b as u32) + carry;
                out[i+j] = (v%10) as u8;
                carry = v/10;
            }
            let mut k = i+rhs.digits.len();
            while carry != 0 {
                if k == out.len() { out.push(0); }
                let v = out[k] as u32 + carry;
                out[k] = (v%10) as u8; carry = v/10; k += 1;
            }
        }
        Self::new(out, self.negative != rhs.negative)
    }
    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() { return Err(Error::DivisionByZero); }
        let divisor = Self::new(rhs.digits.clone(), false);
        let mut remainder = Self::zero();
        let mut quotient = Vec::new();
        for d in self.digits.iter().rev() {
            remainder.small_mul_add(10, *d as u32);
            let mut q = 0;
            while remainder.cmp_abs(&divisor) != Ordering::Less {
                remainder = Self::new(remainder.diff_abs(&divisor), false);
                q += 1;
            }
            quotient.push(q);
        }
        quotient.reverse();
        let quotient = Self::new(quotient, self.negative != rhs.negative);
        remainder = Self::new(remainder.digits, self.negative);
        Ok((quotient, remainder))
    }
    pub fn pow(&self, exponent: u32) -> Result<Self, Error> {
        if exponent > 1024 { return Err(Error::ExponentTooLarge { exponent, limit: 1024 }); }
        let mut result = Self::new(vec![1], false);
        let mut base = self.clone();
        let mut n = exponent;
        while n != 0 {
            if n%2 == 1 { result = result.mul(&base); }
            n /= 2;
            if n != 0 { base = base.mul(&base); }
        }
        Ok(result)
    }
    pub fn gcd(&self, rhs: &Self) -> Self {
        let mut a = Self::new(self.digits.clone(), false);
        let mut b = Self::new(rhs.digits.clone(), false);
        while !b.is_zero() {
            let (_, r) = a.div_rem(&b).unwrap();
            a = b; b = r;
        }
        a
    }
}
