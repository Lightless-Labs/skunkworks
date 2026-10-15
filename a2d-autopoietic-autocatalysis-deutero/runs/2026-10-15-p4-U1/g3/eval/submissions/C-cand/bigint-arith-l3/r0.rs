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
    // Little-endian base-10^9 limbs. Zero is represented by an empty vector.
    limbs: Vec<u32>,
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

        // First scan all magnitude bytes, ignoring underscores, so invalid
        // bytes take precedence over every separator error.
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' && !byte.is_ascii_digit() {
                return Err(Error::InvalidDigit { index });
            }
        }

        let mut first_separator_error = None;
        for index in start..bytes.len() {
            if bytes[index] == b'_' {
                let left_is_digit =
                    index > start && bytes[index - 1].is_ascii_digit();
                let right_is_digit =
                    index + 1 < bytes.len() && bytes[index + 1].is_ascii_digit();
                if !(left_is_digit && right_is_digit) {
                    first_separator_error = Some(index);
                    break;
                }
            }
        }
        if let Some(index) = first_separator_error {
            return Err(Error::Separator { index });
        }

        let first_digit = (start..bytes.len())
            .find(|&index| bytes[index].is_ascii_digit())
            .expect("nonempty valid magnitude");

        let digit_count = bytes[start..]
            .iter()
            .filter(|&&byte| byte.is_ascii_digit())
            .count();

        if digit_count > 1 && bytes[first_digit] == b'0' {
            return Err(Error::LeadingZero { index: first_digit });
        }

        if digit_count > max_digits {
            let excess_number = max_digits + 1;
            let mut seen = 0usize;
            let mut excess_index = first_digit;
            for index in start..bytes.len() {
                if bytes[index].is_ascii_digit() {
                    seen += 1;
                    if seen == excess_number {
                        excess_index = index;
                        break;
                    }
                }
            }
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: excess_index,
            });
        }

        let mut value = Self::zero();
        for &byte in &bytes[start..] {
            if byte != b'_' {
                value.mul_small_assign(10);
                value.add_small_assign((byte - b'0') as u32);
            }
        }
        value.negative = negative && !value.is_zero();
        Ok(value)
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let mut magnitude = self.magnitude_string();
        let target = min_digits.max(1);
        if magnitude.len() < target {
            let mut padded = String::with_capacity(target);
            padded.extend(std::iter::repeat_n('0', target - magnitude.len()));
            padded.push_str(&magnitude);
            magnitude = padded;
        }

        let mut grouped = String::with_capacity(
            magnitude.len() + magnitude.len().saturating_sub(1) / group.max(1),
        );
        if group == 0 {
            grouped.push_str(&magnitude);
        } else {
            let first_group = {
                let remainder = magnitude.len() % group;
                if remainder == 0 { group } else { remainder }
            };
            grouped.push_str(&magnitude[..first_group]);
            let mut index = first_group;
            while index < magnitude.len() {
                grouped.push('_');
                grouped.push_str(&magnitude[index..index + group]);
                index += group;
            }
        }

        if self.negative && !self.is_zero() {
            let mut result = String::with_capacity(grouped.len() + 1);
            result.push('-');
            result.push_str(&grouped);
            result
        } else {
            grouped
        }
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.signum(), rhs.signum()) {
            (-1, 0 | 1) => Ordering::Less,
            (0 | 1, -1) => Ordering::Greater,
            (0, 0) => Ordering::Equal,
            (1, 1) => self.cmp_abs(rhs),
            (-1, -1) => self.cmp_abs(rhs).reverse(),
            _ => Ordering::Equal,
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            let mut result = Self {
                negative: self.negative,
                limbs: Self::add_abs(&self.limbs, &rhs.limbs),
            };
            result.normalize();
            result
        } else {
            match self.cmp_abs(rhs) {
                Ordering::Greater => {
                    let mut result = Self {
                        negative: self.negative,
                        limbs: Self::sub_abs(&self.limbs, &rhs.limbs),
                    };
                    result.normalize();
                    result
                }
                Ordering::Less => {
                    let mut result = Self {
                        negative: rhs.negative,
                        limbs: Self::sub_abs(&rhs.limbs, &self.limbs),
                    };
                    result.normalize();
                    result
                }
                Ordering::Equal => Self::zero(),
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

        let mut result = vec![0u64; self.limbs.len() + rhs.limbs.len()];
        for (i, &left) in self.limbs.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &right) in rhs.limbs.iter().enumerate() {
                let index = i + j;
                let total = result[index] + left as u64 * right as u64 + carry;
                result[index] = total % BASE;
                carry = total / BASE;
            }

            let mut index = i + rhs.limbs.len();
            while carry != 0 {
                if index == result.len() {
                    result.push(0);
                }
                let total = result[index] + carry;
                result[index] = total % BASE;
                carry = total / BASE;
                index += 1;
            }
        }

        let mut value = Self {
            negative: self.negative != rhs.negative,
            limbs: result.into_iter().map(|limb| limb as u32).collect(),
        };
        value.normalize();
        value
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }
        if self.is_zero() {
            return Ok((Self::zero(), Self::zero()));
        }

        let (quotient_limbs, remainder_limbs) =
            Self::div_abs(&self.limbs, &rhs.limbs);

        let mut quotient = Self {
            negative: self.negative != rhs.negative,
            limbs: quotient_limbs,
        };
        quotient.normalize();

        let mut remainder = Self {
            negative: self.negative,
            limbs: remainder_limbs,
        };
        remainder.normalize();

        Ok((quotient, remainder))
    }

    fn zero() -> Self {
        Self {
            negative: false,
            limbs: Vec::new(),
        }
    }

    fn is_zero(&self) -> bool {
        self.limbs.is_empty()
    }

    fn normalize(&mut self) {
        while self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }
        if self.limbs.is_empty() {
            self.negative = false;
        }
    }

    fn cmp_abs(&self, rhs: &Self) -> Ordering {
        Self::cmp_abs_limbs(&self.limbs, &rhs.limbs)
    }

    fn cmp_abs_limbs(left: &[u32], right: &[u32]) -> Ordering {
        match left.len().cmp(&right.len()) {
            Ordering::Equal => {
                for (&a, &b) in left.iter().rev().zip(right.iter().rev()) {
                    match a.cmp(&b) {
                        Ordering::Equal => {}
                        ordering => return ordering,
                    }
                }
                Ordering::Equal
            }
            ordering => ordering,
        }
    }

    fn add_abs(left: &[u32], right: &[u32]) -> Vec<u32> {
        let count = left.len().max(right.len());
        let mut result = Vec::with_capacity(count + 1);
        let mut carry = 0u64;
        for index in 0..count {
            let a = left.get(index).copied().unwrap_or(0) as u64;
            let b = right.get(index).copied().unwrap_or(0) as u64;
            let total = a + b + carry;
            result.push((total % BASE) as u32);
            carry = total / BASE;
        }
        if carry != 0 {
            result.push(carry as u32);
        }
        result
    }

    // Requires left >= right in magnitude.
    fn sub_abs(left: &[u32], right: &[u32]) -> Vec<u32> {
        let mut result = Vec::with_capacity(left.len());
        let mut borrow = 0i64;
        for (index, &limb) in left.iter().enumerate() {
            let a = limb as i64 - borrow;
            let b = right.get(index).copied().unwrap_or(0) as i64;
            if a < b {
                result.push((a + BASE as i64 - b) as u32);
                borrow = 1;
            } else {
                result.push((a - b) as u32);
                borrow = 0;
            }
        }
        while result.last() == Some(&0) {
            result.pop();
        }
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
            let total = *limb as u64 * multiplier as u64 + carry;
            *limb = (total % BASE) as u32;
            carry = total / BASE;
        }
        while carry != 0 {
            self.limbs.push((carry % BASE) as u32);
            carry /= BASE;
        }
    }

    fn add_small_assign(&mut self, addend: u32) {
        let mut carry = addend as u64;
        let mut index = 0;
        while carry != 0 {
            if index == self.limbs.len() {
                self.limbs.push(0);
            }
            let total = self.limbs[index] as u64 + carry;
            self.limbs[index] = (total % BASE) as u32;
            carry = total / BASE;
            index += 1;
        }
    }

    fn magnitude_string(&self) -> String {
        if self.is_zero() {
            return "0".to_owned();
        }
        let mut result = self.limbs.last().unwrap().to_string();
        for limb in self.limbs.iter().rev().skip(1) {
            use std::fmt::Write;
            write!(&mut result, "{limb:09}").unwrap();
        }
        result
    }

    fn mul_abs_small(limbs: &[u32], multiplier: u64) -> Vec<u32> {
        if multiplier == 0 || limbs.is_empty() {
            return Vec::new();
        }
        let mut result = Vec::with_capacity(limbs.len() + 1);
        let mut carry = 0u64;
        for &limb in limbs {
            let total = limb as u64 * multiplier + carry;
            result.push((total % BASE) as u32);
            carry = total / BASE;
        }
        while carry != 0 {
            result.push((carry % BASE) as u32);
            carry /= BASE;
        }
        result
    }

    fn div_abs(dividend: &[u32], divisor: &[u32]) -> (Vec<u32>, Vec<u32>) {
        if Self::cmp_abs_limbs(dividend, divisor) == Ordering::Less {
            return (Vec::new(), dividend.to_vec());
        }

        let mut quotient = vec![0u32; dividend.len()];
        let mut remainder = Vec::<u32>::new();

        for &limb in dividend.iter().rev() {
            remainder.insert(0, limb);
            while remainder.last() == Some(&0) {
                remainder.pop();
            }

            let mut low = 0u64;
            let mut high = BASE - 1;
            while low < high {
                let mid = low + (high - low + 1) / 2;
                let product = Self::mul_abs_small(divisor, mid);
                if Self::cmp_abs_limbs(&product, &remainder) != Ordering::Greater {
                    low = mid;
                } else {
                    high = mid - 1;
                }
            }

            quotient[remainder.len().min(dividend.len()) - 1] = low as u32;
            if low != 0 {
                let product = Self::mul_abs_small(divisor, low);
                remainder = Self::sub_abs(&remainder, &product);
            }
        }

        while quotient.last() == Some(&0) {
            quotient.pop();
        }
        while remainder.last() == Some(&0) {
            remainder.pop();
        }
        (quotient, remainder)
    }
}

fn main() {}