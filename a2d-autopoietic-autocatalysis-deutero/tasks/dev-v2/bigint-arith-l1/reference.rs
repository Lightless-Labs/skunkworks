use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Empty,
    SignOnly,
    InvalidDigit { index: usize },
    Separator { index: usize },
    LeadingZero { index: usize },
    TooManyDigits { limit: usize, index: usize },
    Underflow,
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
    pub fn parse(text: &str, max_digits: usize) -> Result<Self, Error> {
        let radix = 10;
        let bytes = text.as_bytes();
        if bytes.is_empty() { return Err(Error::Empty); }
        let signed = false;
        let start = if bytes[0] == b'+' || (signed && bytes[0] == b'-') { 1 } else { 0 };
        if start == bytes.len() { return Err(Error::SignOnly); }
        for i in start..bytes.len() {
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
    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let radix = 10;
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
        output
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
    pub fn sub(&self, rhs: &Self) -> Result<Self, Error> {
        if self.cmp_abs(rhs) == Ordering::Less { return Err(Error::Underflow); }
        Ok(Self::new(self.diff_abs(rhs), false))
    }



}
