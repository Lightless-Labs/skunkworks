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
    // Little-endian base-10 digits. Zero is represented as [0] with negative=false.
    digits: Vec<u8>,
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

        // Scan all magnitude bytes for invalid bytes before considering separators.
        if let Some(index) = (start..bytes.len())
            .find(|&i| bytes[i] != b'_' && !bytes[i].is_ascii_digit())
        {
            return Err(Error::InvalidDigit { index });
        }

        // Report the leftmost underscore that lacks a digit on either side.
        for i in start..bytes.len() {
            if bytes[i] == b'_'
                && (i == start
                    || i + 1 == bytes.len()
                    || !bytes[i - 1].is_ascii_digit()
                    || !bytes[i + 1].is_ascii_digit())
            {
                return Err(Error::Separator { index: i });
            }
        }

        let digit_positions: Vec<usize> = (start..bytes.len())
            .filter(|&i| bytes[i] != b'_')
            .collect();

        if digit_positions.len() > 1 && bytes[digit_positions[0]] == b'0' {
            return Err(Error::LeadingZero {
                index: digit_positions[0],
            });
        }

        if digit_positions.len() > max_digits {
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: digit_positions[max_digits],
            });
        }

        let mut digits: Vec<u8> = digit_positions
            .iter()
            .rev()
            .map(|&i| bytes[i] - b'0')
            .collect();

        while digits.len() > 1 && digits.last() == Some(&0) {
            digits.pop();
        }

        let is_zero = digits.len() == 1 && digits[0] == 0;
        Ok(Self {
            negative: negative && !is_zero,
            digits,
        })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let magnitude_len = self.digits.len().max(1).max(min_digits);
        let padding = magnitude_len.saturating_sub(self.digits.len());

        let mut out = String::new();
        if self.negative {
            out.push('-');
        }

        for pos in 0..magnitude_len {
            if group > 0 && pos > 0 && (magnitude_len - pos) % group == 0 {
                out.push('_');
            }

            if pos < padding {
                out.push('0');
            } else {
                let digit_index = magnitude_len - 1 - pos;
                out.push(char::from(b'0' + self.digits[digit_index]));
            }
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
            let digits = add_magnitudes(&self.digits, &rhs.digits);
            Self::from_parts(self.negative, digits)
        } else {
            match cmp_magnitude(&self.digits, &rhs.digits) {
                Ordering::Greater => Self::from_parts(
                    self.negative,
                    sub_magnitudes(&self.digits, &rhs.digits),
                ),
                Ordering::Less => Self::from_parts(
                    rhs.negative,
                    sub_magnitudes(&rhs.digits, &self.digits),
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

        let mut result = vec![0u8; self.digits.len() + rhs.digits.len()];
        for (i, &a) in self.digits.iter().enumerate() {
            let mut carry = 0u16;
            for (j, &b) in rhs.digits.iter().enumerate() {
                let index = i + j;
                let total = result[index] as u16 + (a as u16) * (b as u16) + carry;
                result[index] = (total % 10) as u8;
                carry = total / 10;
            }

            let mut index = i + rhs.digits.len();
            while carry > 0 {
                if index == result.len() {
                    result.push(0);
                }
                let total = result[index] as u16 + carry;
                result[index] = (total % 10) as u8;
                carry = total / 10;
                index += 1;
            }
        }

        Self::from_parts(self.negative != rhs.negative, result)
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }

        let (quotient, remainder) = div_rem_magnitudes(&self.digits, &rhs.digits);
        Ok((
            Self::from_parts(self.negative != rhs.negative, quotient),
            Self::from_parts(self.negative, remainder),
        ))
    }

    fn zero() -> Self {
        Self {
            negative: false,
            digits: vec![0],
        }
    }

    fn from_parts(negative: bool, mut digits: Vec<u8>) -> Self {
        normalize(&mut digits);
        let is_zero = digits.len() == 1 && digits[0] == 0;
        Self {
            negative: negative && !is_zero,
            digits,
        }
    }

    fn is_zero(&self) -> bool {
        self.digits.len() == 1 && self.digits[0] == 0
    }
}

fn normalize(digits: &mut Vec<u8>) {
    while digits.len() > 1 && digits.last() == Some(&0) {
        digits.pop();
    }
    if digits.is_empty() {
        digits.push(0);
    }
}

fn cmp_magnitude(a: &[u8], b: &[u8]) -> Ordering {
    match a.len().cmp(&b.len()) {
        Ordering::Equal => {
            for i in (0..a.len()).rev() {
                match a[i].cmp(&b[i]) {
                    Ordering::Equal => {}
                    order => return order,
                }
            }
            Ordering::Equal
        }
        order => order,
    }
}

fn add_magnitudes(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(a.len().max(b.len()) + 1);
    let mut carry = 0u16;

    for i in 0..a.len().max(b.len()) {
        let total = a.get(i).copied().unwrap_or(0) as u16
            + b.get(i).copied().unwrap_or(0) as u16
            + carry;
        result.push((total % 10) as u8);
        carry = total / 10;
    }

    if carry > 0 {
        result.push(carry as u8);
    }

    result
}

fn sub_magnitudes(a: &[u8], b: &[u8]) -> Vec<u8> {
    // Requires a >= b.
    let mut result = Vec::with_capacity(a.len());
    let mut borrow = 0i16;

    for i in 0..a.len() {
        let mut digit = a[i] as i16 - borrow - b.get(i).copied().unwrap_or(0) as i16;
        if digit < 0 {
            digit += 10;
            borrow = 1;
        } else {
            borrow = 0;
        }
        result.push(digit as u8);
    }

    normalize(&mut result);
    result
}

fn mul_magnitude_digit(a: &[u8], digit: u8) -> Vec<u8> {
    if digit == 0 {
        return vec![0];
    }

    let mut result = Vec::with_capacity(a.len() + 1);
    let mut carry = 0u16;

    for &value in a {
        let total = value as u16 * digit as u16 + carry;
        result.push((total % 10) as u8);
        carry = total / 10;
    }

    while carry > 0 {
        result.push((carry % 10) as u8);
        carry /= 10;
    }

    result
}

fn div_rem_magnitudes(dividend: &[u8], divisor: &[u8]) -> (Vec<u8>, Vec<u8>) {
    if cmp_magnitude(dividend, divisor) == Ordering::Less {
        return (vec![0], dividend.to_vec());
    }

    let mut quotient_be = Vec::with_capacity(dividend.len());
    let mut remainder = vec![0];

    for &digit in dividend.iter().rev() {
        if remainder.len() == 1 && remainder[0] == 0 {
            remainder[0] = digit;
        } else {
            remainder.insert(0, digit);
        }
        normalize(&mut remainder);

        let mut q_digit = 0u8;
        for candidate in (1..=9).rev() {
            let product = mul_magnitude_digit(divisor, candidate);
            if cmp_magnitude(&product, &remainder) != Ordering::Greater {
                q_digit = candidate;
                remainder = sub_magnitudes(&remainder, &product);
                break;
            }
        }
        quotient_be.push(q_digit);
    }

    quotient_be.reverse();
    normalize(&mut quotient_be);
    normalize(&mut remainder);
    (quotient_be, remainder)
}