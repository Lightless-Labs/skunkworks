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
    // Little-endian decimal digits. Zero is represented as [0] and is never negative.
    digits: Vec<u8>,
}

impl Big {
    pub fn parse(text: &str, max_digits: usize) -> Result<Self, Error> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(Error::Empty);
        }

        let mut start = 0;
        let mut negative = false;
        if bytes[0] == b'+' || bytes[0] == b'-' {
            negative = bytes[0] == b'-';
            start = 1;
            if start == bytes.len() {
                return Err(Error::SignOnly);
            }
        }

        if let Some(index) = (start..bytes.len())
            .find(|&i| bytes[i] != b'_' && !bytes[i].is_ascii_digit())
        {
            return Err(Error::InvalidDigit { index });
        }

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

        let digit_count = bytes[start..].iter().filter(|&&b| b != b'_').count();
        let first_digit_index = (start..bytes.len()).find(|&i| bytes[i] != b'_').unwrap();

        if digit_count > 1 && bytes[first_digit_index] == b'0' {
            return Err(Error::LeadingZero {
                index: first_digit_index,
            });
        }

        if digit_count > max_digits {
            let excess_number = max_digits + 1;
            let mut seen = 0;
            let index = (start..bytes.len())
                .find(|&i| {
                    if bytes[i] == b'_' {
                        false
                    } else {
                        seen += 1;
                        seen == excess_number
                    }
                })
                .unwrap();
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index,
            });
        }

        let mut digits: Vec<u8> = bytes[start..]
            .iter()
            .filter(|&&b| b != b'_')
            .map(|&b| b - b'0')
            .collect();
        digits.reverse();
        let mut value = Self { negative, digits };
        value.normalize();
        Ok(value)
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let width = min_digits.max(self.digits.len()).max(1);
        let mut magnitude = String::with_capacity(width + width / group.max(1));
        let padding = width - self.digits.len();

        for i in 0..width {
            if group > 0 && i > 0 && (width - i) % group == 0 {
                magnitude.push('_');
            }
            if i < padding {
                magnitude.push('0');
            } else {
                magnitude.push((b'0' + self.digits[width - 1 - i]) as char);
            }
        }

        if self.negative {
            format!("-{magnitude}")
        } else {
            magnitude
        }
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
            Self::from_parts(
                self.negative,
                add_magnitude(&self.digits, &rhs.digits),
            )
        } else {
            match cmp_magnitude(&self.digits, &rhs.digits) {
                Ordering::Greater => Self::from_parts(
                    self.negative,
                    sub_magnitude(&self.digits, &rhs.digits),
                ),
                Ordering::Less => Self::from_parts(
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

        let mut result = vec![0u8; self.digits.len() + rhs.digits.len()];
        for (i, &a) in self.digits.iter().enumerate() {
            let mut carry = 0u16;
            for (j, &b) in rhs.digits.iter().enumerate() {
                let k = i + j;
                let product = result[k] as u16 + (a as u16) * (b as u16) + carry;
                result[k] = (product % 10) as u8;
                carry = product / 10;
            }

            let mut k = i + rhs.digits.len();
            while carry > 0 {
                if k == result.len() {
                    result.push(0);
                }
                let sum = result[k] as u16 + carry;
                result[k] = (sum % 10) as u8;
                carry = sum / 10;
                k += 1;
            }
        }

        Self::from_parts(self.negative != rhs.negative, result)
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }

        let (quotient_digits, remainder_digits) =
            div_magnitude(&self.digits, &rhs.digits);

        let quotient = Self::from_parts(
            self.negative != rhs.negative,
            quotient_digits,
        );
        let remainder = Self::from_parts(self.negative, remainder_digits);
        Ok((quotient, remainder))
    }

    fn zero() -> Self {
        Self {
            negative: false,
            digits: vec![0],
        }
    }

    fn from_parts(negative: bool, digits: Vec<u8>) -> Self {
        let mut value = Self { negative, digits };
        value.normalize();
        value
    }

    fn normalize(&mut self) {
        while self.digits.len() > 1 && self.digits.last() == Some(&0) {
            self.digits.pop();
        }
        if self.digits.is_empty() {
            self.digits.push(0);
        }
        if self.is_zero() {
            self.negative = false;
        }
    }

    fn is_zero(&self) -> bool {
        self.digits.len() == 1 && self.digits[0] == 0
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

fn sub_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(a.len());
    let mut borrow = 0i16;
    for i in 0..a.len() {
        let mut digit = a[i] as i16 - b.get(i).copied().unwrap_or(0) as i16 - borrow;
        if digit < 0 {
            digit += 10;
            borrow = 1;
        } else {
            borrow = 0;
        }
        result.push(digit as u8);
    }
    while result.len() > 1 && result.last() == Some(&0) {
        result.pop();
    }
    result
}

fn mul_magnitude_digit(a: &[u8], digit: u8) -> Vec<u8> {
    if digit == 0 {
        return vec![0];
    }
    let mut result = Vec::with_capacity(a.len() + 1);
    let mut carry = 0u16;
    for &value in a {
        let product = value as u16 * digit as u16 + carry;
        result.push((product % 10) as u8);
        carry = product / 10;
    }
    while carry > 0 {
        result.push((carry % 10) as u8);
        carry /= 10;
    }
    result
}

fn div_magnitude(dividend: &[u8], divisor: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut quotient = vec![0u8; dividend.len()];
    let mut remainder = vec![0u8];

    for i in (0..dividend.len()).rev() {
        remainder.insert(0, dividend[i]);
        while remainder.len() > 1 && remainder.last() == Some(&0) {
            remainder.pop();
        }

        let mut qdigit = 0;
        for candidate in (1..=9).rev() {
            let product = mul_magnitude_digit(divisor, candidate);
            if cmp_magnitude(&product, &remainder) != Ordering::Greater {
                qdigit = candidate;
                remainder = sub_magnitude(&remainder, &product);
                break;
            }
        }
        quotient[i] = qdigit;
    }

    while quotient.len() > 1 && quotient.last() == Some(&0) {
        quotient.pop();
    }
    while remainder.len() > 1 && remainder.last() == Some(&0) {
        remainder.pop();
    }
    (quotient, remainder)
}