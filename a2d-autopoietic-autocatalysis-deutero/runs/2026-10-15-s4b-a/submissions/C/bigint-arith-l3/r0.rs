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
}

impl Big {
    pub fn parse(text: &str, max_digits: usize) -> Result<Self, Error> {
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

        for (i, &b) in bytes.iter().enumerate().skip(start) {
            if b != b'_' && !b.is_ascii_digit() {
                return Err(Error::InvalidDigit { index: i });
            }
        }

        let mut malformed_separator = None;
        for i in start..bytes.len() {
            if bytes[i] == b'_'
                && (i == start
                    || i + 1 == bytes.len()
                    || !bytes[i - 1].is_ascii_digit()
                    || !bytes[i + 1].is_ascii_digit())
            {
                malformed_separator = Some(i);
                break;
            }
        }
        if let Some(index) = malformed_separator {
            return Err(Error::Separator { index });
        }

        let digit_count = bytes[start..].iter().filter(|&&b| b != b'_').count();
        let first_digit_index = (start..bytes.len()).find(|&i| bytes[i] != b'_').unwrap();

        if digit_count > 1 && bytes[first_digit_index] == b'0' {
            return Err(Error::LeadingZero {
                index: first_digit_index,
            });
        }

        if digit_count > max_digits {
            let excess_number = max_digits + 1;
            let mut seen = 0usize;
            for i in start..bytes.len() {
                if bytes[i] != b'_' {
                    seen += 1;
                    if seen == excess_number {
                        return Err(Error::TooManyDigits {
                            limit: max_digits,
                            index: i,
                        });
                    }
                }
            }
        }

        let mut digits: Vec<u8> = bytes[start..]
            .iter()
            .filter(|&&b| b != b'_')
            .map(|&b| b - b'0')
            .collect();
        while digits.len() > 1 && digits[0] == 0 {
            digits.remove(0);
        }
        let is_zero = digits.iter().all(|&d| d == 0);
        Ok(Self {
            negative: negative && !is_zero,
            digits,
        })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let width = min_digits.max(self.digits.len()).max(1);
        let pad = width - self.digits.len();
        let mut magnitude = String::with_capacity(width + width / 2);
        for _ in 0..pad {
            magnitude.push('0');
        }

        let mut count = 0usize;
        for &digit in &self.digits {
            if group > 0 && count > 0 && (width - count) % group == 0 {
                magnitude.push('_');
            }
            magnitude.push((b'0' + digit) as char);
            count += 1;
        }

        if self.negative {
            format!("-{magnitude}")
        } else {
            magnitude
        }
    }

    pub fn cmp_value(&self, rhs: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (self.negative, rhs.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => cmp_mag(&self.digits, &rhs.digits),
            (true, true) => cmp_mag(&rhs.digits, &self.digits),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            let digits = add_mag(&self.digits, &rhs.digits);
            Self::from_parts(self.negative, digits)
        } else {
            match cmp_mag(&self.digits, &rhs.digits) {
                std::cmp::Ordering::Equal => Self::zero(),
                std::cmp::Ordering::Greater => Self::from_parts(
                    self.negative,
                    sub_mag(&self.digits, &rhs.digits),
                ),
                std::cmp::Ordering::Less => Self::from_parts(
                    rhs.negative,
                    sub_mag(&rhs.digits, &self.digits),
                ),
            }
        }
    }

    pub fn sub(&self, rhs: &Self) -> Self {
        self.add(&rhs.neg())
    }

    pub fn neg(&self) -> Self {
        Self::from_parts(!self.negative, self.digits.clone())
    }

    pub fn signum(&self) -> i8 {
        if is_zero_mag(&self.digits) {
            0
        } else if self.negative {
            -1
        } else {
            1
        }
    }

    pub fn mul(&self, rhs: &Self) -> Self {
        if is_zero_mag(&self.digits) || is_zero_mag(&rhs.digits) {
            return Self::zero();
        }

        let mut accum = vec![0u32; self.digits.len() + rhs.digits.len()];
        for (i, &a) in self.digits.iter().rev().enumerate() {
            for (j, &b) in rhs.digits.iter().rev().enumerate() {
                accum[i + j] += (a as u32) * (b as u32);
            }
        }
        for i in 0..accum.len() - 1 {
            let carry = accum[i] / 10;
            accum[i] %= 10;
            accum[i + 1] += carry;
        }

        let mut digits: Vec<u8> = accum.into_iter().rev().map(|x| x as u8).collect();
        normalize(&mut digits);
        Self::from_parts(self.negative ^ rhs.negative, digits)
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if is_zero_mag(&rhs.digits) {
            return Err(Error::DivisionByZero);
        }

        let (q_digits, r_digits) = div_mag(&self.digits, &rhs.digits);
        let quotient = Self::from_parts(self.negative ^ rhs.negative, q_digits);
        let remainder = Self::from_parts(self.negative, r_digits);
        Ok((quotient, remainder))
    }

    fn zero() -> Self {
        Self {
            negative: false,
            digits: vec![0],
        }
    }

    fn from_parts(negative: bool, mut digits: Vec<u8>) -> Self {
        normalize(&mut digits);
        let zero = is_zero_mag(&digits);
        Self {
            negative: negative && !zero,
            digits,
        }
    }
}

fn normalize(digits: &mut Vec<u8>) {
    let first_nonzero = digits.iter().position(|&d| d != 0);
    match first_nonzero {
        Some(i) if i > 0 => {
            digits.drain(0..i);
        }
        None => {
            digits.clear();
            digits.push(0);
        }
        _ => {}
    }
}

fn is_zero_mag(digits: &[u8]) -> bool {
    digits.iter().all(|&d| d == 0)
}

fn cmp_mag(a: &[u8], b: &[u8]) -> std::cmp::Ordering {
    match a.len().cmp(&b.len()) {
        std::cmp::Ordering::Equal => a.cmp(b),
        other => other,
    }
}

fn add_mag(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(a.len().max(b.len()) + 1);
    let mut carry = 0u8;
    let mut i = a.len();
    let mut j = b.len();

    while i > 0 || j > 0 || carry > 0 {
        let av = if i > 0 {
            i -= 1;
            a[i]
        } else {
            0
        };
        let bv = if j > 0 {
            j -= 1;
            b[j]
        } else {
            0
        };
        let sum = av + bv + carry;
        out.push(sum % 10);
        carry = sum / 10;
    }

    out.reverse();
    out
}

fn sub_mag(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(a.len());
    let mut borrow = 0i16;
    let mut i = a.len();
    let mut j = b.len();

    while i > 0 {
        i -= 1;
        let av = a[i] as i16 - borrow;
        let bv = if j > 0 {
            j -= 1;
            b[j] as i16
        } else {
            0
        };
        if av < bv {
            out.push((av + 10 - bv) as u8);
            borrow = 1;
        } else {
            out.push((av - bv) as u8);
            borrow = 0;
        }
    }

    out.reverse();
    normalize(&mut out);
    out
}

fn mul_mag_digit(a: &[u8], digit: u8) -> Vec<u8> {
    if digit == 0 {
        return vec![0];
    }
    let mut out = Vec::with_capacity(a.len() + 1);
    let mut carry = 0u16;
    for &d in a.iter().rev() {
        let value = d as u16 * digit as u16 + carry;
        out.push((value % 10) as u8);
        carry = value / 10;
    }
    while carry > 0 {
        out.push((carry % 10) as u8);
        carry /= 10;
    }
    out.reverse();
    out
}

fn div_mag(dividend: &[u8], divisor: &[u8]) -> (Vec<u8>, Vec<u8>) {
    if cmp_mag(dividend, divisor) == std::cmp::Ordering::Less {
        return (vec![0], dividend.to_vec());
    }

    let mut quotient = Vec::with_capacity(dividend.len());
    let mut remainder = vec![0];

    for &digit in dividend {
        if is_zero_mag(&remainder) {
            remainder[0] = digit;
        } else {
            remainder.push(digit);
        }
        normalize(&mut remainder);

        let mut q_digit = 0u8;
        for candidate in (1..=9).rev() {
            let product = mul_mag_digit(divisor, candidate);
            if cmp_mag(&product, &remainder) != std::cmp::Ordering::Greater {
                q_digit = candidate;
                remainder = sub_mag(&remainder, &product);
                break;
            }
        }
        quotient.push(q_digit);
    }

    normalize(&mut quotient);
    normalize(&mut remainder);
    (quotient, remainder)
}

fn main() {}