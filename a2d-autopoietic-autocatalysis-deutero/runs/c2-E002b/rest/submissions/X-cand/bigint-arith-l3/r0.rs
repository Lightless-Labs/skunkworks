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
    // Little-endian decimal digits. Zero is represented by [0] and is never negative.
    digits: Vec<u8>,
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
        if start == bytes.len() && start == 1 {
            return Err(Error::SignOnly);
        }

        // Invalid bytes take precedence over malformed separators.
        if let Some(index) = (start..bytes.len())
            .find(|&i| bytes[i] != b'_' && !bytes[i].is_ascii_digit())
        {
            return Err(Error::InvalidDigit { index });
        }

        // Find the leftmost underscore not surrounded by digits.
        if let Some(index) = (start..bytes.len()).find(|&i| {
            bytes[i] == b'_'
                && (i == start
                    || i + 1 == bytes.len()
                    || !bytes[i - 1].is_ascii_digit()
                    || !bytes[i + 1].is_ascii_digit())
        }) {
            return Err(Error::Separator { index });
        }

        let first_digit = (start..bytes.len()).find(|&i| bytes[i] != b'_').unwrap();
        let digit_count = bytes[start..].iter().filter(|&&b| b != b'_').count();

        if digit_count > 1 && bytes[first_digit] == b'0' {
            return Err(Error::LeadingZero { index: first_digit });
        }

        if digit_count > max_digits {
            let excess_number = max_digits + 1;
            let index = bytes[start..]
                .iter()
                .enumerate()
                .filter(|(_, b)| **b != b'_')
                .nth(excess_number - 1)
                .map(|(offset, _)| start + offset)
                .unwrap();
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index,
            });
        }

        let mut digits = Vec::with_capacity(digit_count);
        for &b in bytes[start..].iter().rev() {
            if b != b'_' {
                digits.push(b - b'0');
            }
        }
        if digits.is_empty() {
            digits.push(0);
        }

        let negative = start == 1 && bytes[0] == b'-' && !is_zero_digits(&digits);
        Ok(Self { negative, digits })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let width = min_digits.max(self.digits.len()).max(1);
        let padding = width - self.digits.len();
        let mut out = String::with_capacity(
            width + if group > 0 { (width - 1) / group } else { 0 } + usize::from(self.negative),
        );

        if self.negative {
            out.push('-');
        }
        for position in 0..width {
            if group > 0 && position > 0 && (width - position) % group == 0 {
                out.push('_');
            }
            let digit = if position < padding {
                0
            } else {
                self.digits[width - 1 - position]
            };
            out.push((b'0' + digit) as char);
        }
        out
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.negative, rhs.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => cmp_magnitude(&self.digits, &rhs.digits),
            (true, true) => cmp_magnitude(&rhs.digits, &self.digits),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            Self::from_magnitude(
                self.negative,
                add_magnitude(&self.digits, &rhs.digits),
            )
        } else {
            match cmp_magnitude(&self.digits, &rhs.digits) {
                Ordering::Greater => Self::from_magnitude(
                    self.negative,
                    sub_magnitude(&self.digits, &rhs.digits),
                ),
                Ordering::Less => Self::from_magnitude(
                    rhs.negative,
                    sub_magnitude(&rhs.digits, &self.digits),
                ),
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
        Self::from_magnitude(
            self.negative != rhs.negative,
            mul_magnitude(&self.digits, &rhs.digits),
        )
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }

        let (quotient, remainder) = div_rem_magnitude(&self.digits, &rhs.digits);
        Ok((
            Self::from_magnitude(self.negative != rhs.negative, quotient),
            Self::from_magnitude(self.negative, remainder),
        ))
    }

    fn zero() -> Self {
        Self {
            negative: false,
            digits: vec![0],
        }
    }

    fn from_magnitude(negative: bool, digits: Vec<u8>) -> Self {
        let digits = normalize(digits);
        let negative = negative && !is_zero_digits(&digits);
        Self { negative, digits }
    }

    fn is_zero(&self) -> bool {
        is_zero_digits(&self.digits)
    }
}

fn is_zero_digits(digits: &[u8]) -> bool {
    digits.len() == 1 && digits[0] == 0
}

fn normalize(mut digits: Vec<u8>) -> Vec<u8> {
    while digits.len() > 1 && digits.last() == Some(&0) {
        digits.pop();
    }
    if digits.is_empty() {
        digits.push(0);
    }
    digits
}

fn cmp_magnitude(a: &[u8], b: &[u8]) -> Ordering {
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

fn add_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(a.len().max(b.len()) + 1);
    let mut carry = 0u8;
    for i in 0..a.len().max(b.len()) {
        let sum = a.get(i).copied().unwrap_or(0) + b.get(i).copied().unwrap_or(0) + carry;
        result.push(sum % 10);
        carry = sum / 10;
    }
    if carry != 0 {
        result.push(carry);
    }
    result
}

// Requires a >= b.
fn sub_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(a.len());
    let mut borrow = 0i8;
    for i in 0..a.len() {
        let mut digit = a[i] as i8 - borrow - b.get(i).copied().unwrap_or(0) as i8;
        if digit < 0 {
            digit += 10;
            borrow = 1;
        } else {
            borrow = 0;
        }
        result.push(digit as u8);
    }
    normalize(result)
}

fn mul_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut result = vec![0u8; a.len() + b.len()];
    for (i, &da) in a.iter().enumerate() {
        let mut carry = 0u16;
        for (j, &db) in b.iter().enumerate() {
            let index = i + j;
            let value = result[index] as u16 + da as u16 * db as u16 + carry;
            result[index] = (value % 10) as u8;
            carry = value / 10;
        }
        let mut index = i + b.len();
        while carry > 0 {
            if index == result.len() {
                result.push(0);
            }
            let value = result[index] as u16 + carry;
            result[index] = (value % 10) as u8;
            carry = value / 10;
            index += 1;
        }
    }
    normalize(result)
}

fn mul_digit(a: &[u8], digit: u8) -> Vec<u8> {
    if digit == 0 {
        return vec![0];
    }
    let mut result = Vec::with_capacity(a.len() + 1);
    let mut carry = 0u16;
    for &d in a {
        let value = d as u16 * digit as u16 + carry;
        result.push((value % 10) as u8);
        carry = value / 10;
    }
    while carry > 0 {
        result.push((carry % 10) as u8);
        carry /= 10;
    }
    normalize(result)
}

// Long division of nonnegative magnitudes.
fn div_rem_magnitude(dividend: &[u8], divisor: &[u8]) -> (Vec<u8>, Vec<u8>) {
    if cmp_magnitude(dividend, divisor) == Ordering::Less {
        return (vec![0], dividend.to_vec());
    }

    let mut quotient_be = Vec::with_capacity(dividend.len());
    let mut remainder = vec![0u8];

    for &digit in dividend.iter().rev() {
        if is_zero_digits(&remainder) {
            remainder[0] = digit;
        } else {
            remainder.insert(0, digit);
        }
        remainder = normalize(remainder);

        let mut qdigit = 0u8;
        for candidate in (1..=9).rev() {
            let product = mul_digit(divisor, candidate);
            if cmp_magnitude(&product, &remainder) != Ordering::Greater {
                qdigit = candidate;
                remainder = sub_magnitude(&remainder, &product);
                break;
            }
        }
        quotient_be.push(qdigit);
    }

    quotient_be.reverse();
    (normalize(quotient_be), normalize(remainder))
}