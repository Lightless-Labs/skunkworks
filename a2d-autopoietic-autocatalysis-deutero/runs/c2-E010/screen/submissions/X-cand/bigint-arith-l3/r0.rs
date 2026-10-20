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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    negative: bool,
    // Little-endian base-1,000,000,000 limbs.
    limbs: Vec<u32>,
}

impl Big {
    const BASE: u64 = 1_000_000_000;

    pub fn parse(text: &str, max_digits: usize) -> Result<Self, Error> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(Error::Empty);
        }

        let start = match bytes[0] {
            b'+' | b'-' => 1,
            _ => 0,
        };
        if start == bytes.len() {
            return Err(Error::SignOnly);
        }

        for (i, &b) in bytes.iter().enumerate().skip(start) {
            if b != b'_' && !b.is_ascii_digit() {
                return Err(Error::InvalidDigit { index: i });
            }
        }

        let mut bad_separator = None;
        for i in start..bytes.len() {
            if bytes[i] == b'_'
                && !(i > start
                    && i + 1 < bytes.len()
                    && bytes[i - 1].is_ascii_digit()
                    && bytes[i + 1].is_ascii_digit())
            {
                bad_separator.get_or_insert(i);
            }
        }
        if let Some(index) = bad_separator {
            return Err(Error::Separator { index });
        }

        let digit_positions: Vec<usize> = (start..bytes.len())
            .filter(|&i| bytes[i].is_ascii_digit())
            .collect();

        if digit_positions.len() > 1 && bytes[digit_positions[0]] == b'0' {
            return Err(Error::LeadingZero { index: digit_positions[0] });
        }
        if digit_positions.len() > max_digits {
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: digit_positions[max_digits],
            });
        }

        let mut limbs = vec![0u32];
        let mut digits = 0usize;
        for &b in &bytes[start..] {
            if b.is_ascii_digit() {
                mul_small_in_place(&mut limbs, 10);
                add_small_in_place(&mut limbs, u32::from(b - b'0'));
                digits += 1;
            }
        }
        debug_assert_eq!(digits, digit_positions.len());
        let negative = bytes[0] == b'-' && !(limbs.len() == 1 && limbs[0] == 0);
        Ok(Self { negative, limbs })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let mut body = self.limbs.last().copied().unwrap_or(0).to_string();
        for limb in self.limbs.iter().rev().skip(1) {
            body.push_str(&format!("{:09}", limb));
        }
        if body.len() < min_digits {
            let mut padded = String::with_capacity(min_digits);
            padded.extend(std::iter::repeat_n('0', min_digits - body.len()));
            padded.push_str(&body);
            body = padded;
        }

        let mut out = String::with_capacity(body.len().saturating_add(
            if group > 0 { (body.len() - 1) / group } else { 0 }
                + usize::from(self.negative),
        ));
        if self.negative {
            out.push('-');
        }
        for (i, ch) in body.chars().enumerate() {
            if group > 0 && i > 0 && (body.len() - i) % group == 0 {
                out.push('_');
            }
            out.push(ch);
        }
        out
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.negative, rhs.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => cmp_mag(&self.limbs, &rhs.limbs),
            (true, true) => cmp_mag(&rhs.limbs, &self.limbs),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            Self::from_parts(
                self.negative,
                add_mag(&self.limbs, &rhs.limbs),
            )
        } else {
            match cmp_mag(&self.limbs, &rhs.limbs) {
                Ordering::Greater => Self::from_parts(
                    self.negative,
                    sub_mag(&self.limbs, &rhs.limbs),
                ),
                Ordering::Less => Self::from_parts(
                    rhs.negative,
                    sub_mag(&rhs.limbs, &self.limbs),
                ),
                Ordering::Equal => Self::zero(),
            }
        }
    }

    pub fn sub(&self, rhs: &Self) -> Self {
        self.add(&rhs.neg())
    }

    pub fn neg(&self) -> Self {
        if self.is_zero() {
            self.clone()
        } else {
            Self { negative: !self.negative, limbs: self.limbs.clone() }
        }
    }

    pub fn signum(&self) -> i8 {
        if self.is_zero() { 0 } else if self.negative { -1 } else { 1 }
    }

    pub fn mul(&self, rhs: &Self) -> Self {
        if self.is_zero() || rhs.is_zero() {
            return Self::zero();
        }
        let mut out = vec![0u64; self.limbs.len() + rhs.limbs.len()];
        for (i, &a) in self.limbs.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &b) in rhs.limbs.iter().enumerate() {
                let k = i + j;
                let n = out[k] + u64::from(a) * u64::from(b) + carry;
                out[k] = n % Self::BASE;
                carry = n / Self::BASE;
            }
            let mut k = i + rhs.limbs.len();
            while carry > 0 {
                if k == out.len() {
                    out.push(0);
                }
                let n = out[k] + carry;
                out[k] = n % Self::BASE;
                carry = n / Self::BASE;
                k += 1;
            }
        }
        Self::from_parts(
            self.negative ^ rhs.negative,
            out.into_iter().map(|x| x as u32).collect(),
        )
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }
        match cmp_mag(&self.limbs, &rhs.limbs) {
            Ordering::Less => return Ok((Self::zero(), self.clone())),
            Ordering::Equal => {
                return Ok((
                    Self::from_parts(self.negative ^ rhs.negative, vec![1]),
                    Self::zero(),
                ));
            }
            Ordering::Greater => {}
        }

        let mut quotient_be = Vec::with_capacity(self.limbs.len());
        let mut remainder = vec![0u32];
        for &limb in self.limbs.iter().rev() {
            remainder = mul_base_add(&remainder, limb);
            let mut low = 0u64;
            let mut high = Self::BASE - 1;
            let mut best = 0u32;
            while low <= high {
                let mid = low + (high - low) / 2;
                match cmp_mag(&mul_small(&rhs.limbs, mid), &remainder) {
                    Ordering::Greater => {
                        if mid == 0 { break; }
                        high = mid - 1;
                    }
                    _ => {
                        best = mid as u32;
                        low = mid + 1;
                    }
                }
            }
            quotient_be.push(best);
            remainder = sub_mag(&remainder, &mul_small(&rhs.limbs, u64::from(best)));
        }
        quotient_be.reverse();
        Ok((
            Self::from_parts(self.negative ^ rhs.negative, quotient_be),
            Self::from_parts(self.negative, remainder),
        ))
    }

    fn zero() -> Self {
        Self { negative: false, limbs: vec![0] }
    }

    fn from_parts(negative: bool, mut limbs: Vec<u32>) -> Self {
        normalize(&mut limbs);
        let zero = limbs.len() == 1 && limbs[0] == 0;
        Self { negative: negative && !zero, limbs }
    }

    fn is_zero(&self) -> bool {
        self.limbs.len() == 1 && self.limbs[0] == 0
    }
}

fn normalize(limbs: &mut Vec<u32>) {
    while limbs.len() > 1 && limbs.last() == Some(&0) {
        limbs.pop();
    }
    if limbs.is_empty() {
        limbs.push(0);
    }
}

fn cmp_mag(a: &[u32], b: &[u32]) -> Ordering {
    match a.len().cmp(&b.len()) {
        Ordering::Equal => {
            for i in (0..a.len()).rev() {
                match a[i].cmp(&b[i]) {
                    Ordering::Equal => {}
                    other => return other,
                }
            }
            Ordering::Equal
        }
        other => other,
    }
}

fn add_mag(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(a.len().max(b.len()) + 1);
    let mut carry = 0u64;
    for i in 0..a.len().max(b.len()) {
        let n = u64::from(a.get(i).copied().unwrap_or(0))
            + u64::from(b.get(i).copied().unwrap_or(0))
            + carry;
        out.push((n % Big::BASE) as u32);
        carry = n / Big::BASE;
    }
    if carry != 0 {
        out.push(carry as u32);
    }
    out
}

fn sub_mag(a: &[u32], b: &[u32]) -> Vec<u32> {
    debug_assert!(cmp_mag(a, b) != Ordering::Less);
    let mut out = Vec::with_capacity(a.len());
    let mut borrow = 0i64;
    for i in 0..a.len() {
        let n = i64::from(a[i]) - i64::from(b.get(i).copied().unwrap_or(0)) - borrow;
        if n < 0 {
            out.push((n + Big::BASE as i64) as u32);
            borrow = 1;
        } else {
            out.push(n as u32);
            borrow = 0;
        }
    }
    normalize(&mut out);
    out
}

fn mul_small_in_place(a: &mut Vec<u32>, factor: u32) {
    let mut carry = 0u64;
    for limb in a.iter_mut() {
        let n = u64::from(*limb) * u64::from(factor) + carry;
        *limb = (n % Big::BASE) as u32;
        carry = n / Big::BASE;
    }
    if carry != 0 {
        a.push(carry as u32);
    }
}

fn add_small_in_place(a: &mut Vec<u32>, value: u32) {
    let mut carry = u64::from(value);
    let mut i = 0;
    while carry > 0 {
        if i == a.len() {
            a.push(0);
        }
        let n = u64::from(a[i]) + carry;
        a[i] = (n % Big::BASE) as u32;
        carry = n / Big::BASE;
        i += 1;
    }
}

fn mul_small(a: &[u32], factor: u64) -> Vec<u32> {
    if factor == 0 {
        return vec![0];
    }
    let mut out = Vec::with_capacity(a.len() + 1);
    let mut carry = 0u128;
    for &limb in a {
        let n = u128::from(limb) * u128::from(factor) + carry;
        out.push((n % u128::from(Big::BASE)) as u32);
        carry = n / u128::from(Big::BASE);
    }
    while carry > 0 {
        out.push((carry % u128::from(Big::BASE)) as u32);
        carry /= u128::from(Big::BASE);
    }
    normalize(&mut out);
    out
}

fn mul_base_add(a: &[u32], limb: u32) -> Vec<u32> {
    let mut out = Vec::with_capacity(a.len() + 1);
    out.push(limb);
    out.extend_from_slice(a);
    normalize(&mut out);
    out
}
