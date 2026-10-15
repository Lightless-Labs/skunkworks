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

        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' && !byte.is_ascii_digit() {
                return Err(Error::InvalidDigit { index });
            }
        }

        for index in start..bytes.len() {
            if bytes[index] == b'_'
                && (index == start
                    || index + 1 == bytes.len()
                    || bytes[index - 1] == b'_'
                    || bytes[index + 1] == b'_')
            {
                return Err(Error::Separator { index });
            }
        }

        let digit_count = bytes[start..]
            .iter()
            .filter(|&&byte| byte != b'_')
            .count();

        let first_digit_index = (start..bytes.len())
            .find(|&index| bytes[index] != b'_')
            .expect("nonempty magnitude");

        if digit_count > 1 && bytes[first_digit_index] == b'0' {
            return Err(Error::LeadingZero {
                index: first_digit_index,
            });
        }

        if digit_count > max_digits {
            let excess_digit = (start..bytes.len())
                .filter(|&index| bytes[index] != b'_')
                .nth(max_digits)
                .expect("digit count exceeds limit");
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: excess_digit,
            });
        }

        let digits: Vec<u8> = bytes[start..]
            .iter()
            .filter(|&&byte| byte != b'_')
            .map(|&byte| byte - b'0')
            .collect();

        Ok(Self::new(negative, digits))
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let magnitude_len = self.digits.len().max(min_digits).max(1);
        let padding = magnitude_len - self.digits.len();
        let mut result = String::with_capacity(
            magnitude_len + magnitude_len.saturating_sub(1) / group.max(1) + 1,
        );

        if self.negative {
            result.push('-');
        }

        for position in 0..magnitude_len {
            if group > 0 && position > 0 && (magnitude_len - position) % group == 0 {
                result.push('_');
            }
            if position < padding {
                result.push('0');
            } else {
                result.push(char::from(b'0' + self.digits[position - padding]));
            }
        }

        result
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.signum(), rhs.signum()) {
            (a, b) if a != b => a.cmp(&b),
            (0, 0) => Ordering::Equal,
            (-1, -1) => cmp_magnitude(&rhs.digits, &self.digits),
            _ => cmp_magnitude(&self.digits, &rhs.digits),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        match (self.negative, rhs.negative) {
            (a, b) if a == b => Self::new(a, add_magnitude(&self.digits, &rhs.digits)),
            _ => match cmp_magnitude(&self.digits, &rhs.digits) {
                Ordering::Greater => Self::new(
                    self.negative,
                    sub_magnitude(&self.digits, &rhs.digits),
                ),
                Ordering::Less => Self::new(
                    rhs.negative,
                    sub_magnitude(&rhs.digits, &self.digits),
                ),
                Ordering::Equal => Self::zero(),
            },
        }
    }

    pub fn sub(&self, rhs: &Self) -> Self {
        self.add(&rhs.neg())
    }

    pub fn neg(&self) -> Self {
        if self.is_zero() {
            Self::zero()
        } else {
            Self {
                negative: !self.negative,
                digits: self.digits.clone(),
            }
        }
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
        Self::new(
            self.negative != rhs.negative,
            mul_magnitude(&self.digits, &rhs.digits),
        )
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }

        let (quotient, remainder) = div_magnitude(&self.digits, &rhs.digits);
        Ok((
            Self::new(self.negative != rhs.negative, quotient),
            Self::new(self.negative, remainder),
        ))
    }

    fn new(negative: bool, digits: Vec<u8>) -> Self {
        let digits = normalize(digits);
        let is_zero = digits.len() == 1 && digits[0] == 0;
        Self {
            negative: negative && !is_zero,
            digits,
        }
    }

    fn zero() -> Self {
        Self {
            negative: false,
            digits: vec![0],
        }
    }

    fn is_zero(&self) -> bool {
        self.digits.len() == 1 && self.digits[0] == 0
    }
}

fn normalize(mut digits: Vec<u8>) -> Vec<u8> {
    let first_nonzero = digits.iter().position(|&digit| digit != 0);
    match first_nonzero {
        Some(index) => {
            digits.drain(..index);
            digits
        }
        None => vec![0],
    }
}

fn cmp_magnitude(lhs: &[u8], rhs: &[u8]) -> Ordering {
    lhs.len().cmp(&rhs.len()).then_with(|| lhs.cmp(rhs))
}

fn add_magnitude(lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(lhs.len().max(rhs.len()) + 1);
    let mut i = lhs.len();
    let mut j = rhs.len();
    let mut carry = 0u8;

    while i > 0 || j > 0 || carry > 0 {
        let a = if i > 0 {
            i -= 1;
            lhs[i]
        } else {
            0
        };
        let b = if j > 0 {
            j -= 1;
            rhs[j]
        } else {
            0
        };
        let sum = a + b + carry;
        result.push(sum % 10);
        carry = sum / 10;
    }

    result.reverse();
    result
}

fn sub_magnitude(lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
    debug_assert!(cmp_magnitude(lhs, rhs) != Ordering::Less);
    let mut result = Vec::with_capacity(lhs.len());
    let mut i = lhs.len();
    let mut j = rhs.len();
    let mut borrow = 0i8;

    while i > 0 {
        i -= 1;
        let a = lhs[i] as i8 - borrow;
        let b = if j > 0 {
            j -= 1;
            rhs[j] as i8
        } else {
            0
        };

        if a < b {
            result.push((a + 10 - b) as u8);
            borrow = 1;
        } else {
            result.push((a - b) as u8);
            borrow = 0;
        }
    }

    result.reverse();
    normalize(result)
}

fn mul_magnitude(lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
    let mut result = vec![0u32; lhs.len() + rhs.len()];

    for i in (0..lhs.len()).rev() {
        for j in (0..rhs.len()).rev() {
            result[i + j + 1] += (lhs[i] as u32) * (rhs[j] as u32);
        }
    }

    for index in (1..result.len()).rev() {
        let carry = result[index] / 10;
        result[index] %= 10;
        result[index - 1] += carry;
    }

    normalize(result.into_iter().map(|digit| digit as u8).collect())
}

fn div_magnitude(dividend: &[u8], divisor: &[u8]) -> (Vec<u8>, Vec<u8>) {
    if cmp_magnitude(dividend, divisor) == Ordering::Less {
        return (vec![0], dividend.to_vec());
    }

    let mut quotient = Vec::with_capacity(dividend.len());
    let mut remainder = vec![0];

    for &digit in dividend {
        if remainder.len() == 1 && remainder[0] == 0 {
            remainder[0] = digit;
        } else {
            remainder.push(digit);
        }
        remainder = normalize(remainder);

        let mut quotient_digit = 0u8;
        while cmp_magnitude(&remainder, divisor) != Ordering::Less {
            remainder = sub_magnitude(&remainder, divisor);
            quotient_digit += 1;
        }
        quotient.push(quotient_digit);
    }

    (normalize(quotient), normalize(remainder))
}

fn main() {}