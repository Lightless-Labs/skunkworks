use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    negative: bool,
    // Base-10 digits, least significant first. Zero is represented by [0].
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

        let (negative, magnitude_start) = match bytes[0] {
            b'+' => (false, 1),
            b'-' => (true, 1),
            _ => (false, 0),
        };

        if magnitude_start == bytes.len() {
            return Err(Error::SignOnly);
        }

        // Invalid bytes take precedence over every separator error.
        for (index, &byte) in bytes.iter().enumerate().skip(magnitude_start) {
            if !byte.is_ascii_digit() && byte != b'_' {
                return Err(Error::InvalidDigit { index });
            }
        }

        // An underscore must have a digit immediately on both sides.
        for index in magnitude_start..bytes.len() {
            if bytes[index] == b'_'
                && (index == magnitude_start
                    || index + 1 == bytes.len()
                    || !bytes[index - 1].is_ascii_digit()
                    || !bytes[index + 1].is_ascii_digit())
            {
                return Err(Error::Separator { index });
            }
        }

        let mut digit_count = 0usize;
        let mut first_digit = None;
        let mut digits = Vec::new();

        for (index, &byte) in bytes.iter().enumerate().skip(magnitude_start) {
            if byte.is_ascii_digit() {
                first_digit.get_or_insert((byte, index));
                digit_count += 1;
                digits.push(byte - b'0');
            }
        }

        if digit_count > 1 && first_digit.is_some_and(|(digit, _)| digit == b'0') {
            return Err(Error::LeadingZero {
                index: first_digit.expect("a nonempty magnitude has a digit").1,
            });
        }

        if digit_count > max_digits {
            let excess_digit_number = max_digits + 1;
            let mut seen = 0usize;
            for (index, &byte) in bytes.iter().enumerate().skip(magnitude_start) {
                if byte.is_ascii_digit() {
                    seen += 1;
                    if seen == excess_digit_number {
                        return Err(Error::TooManyDigits {
                            limit: max_digits,
                            index,
                        });
                    }
                }
            }
        }

        digits.reverse();
        let mut result = Self { negative, digits };
        result.normalize();
        Ok(result)
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let natural_width = self.digits.len().max(1);
        let width = natural_width.max(min_digits);
        let padding = width - self.digits.len();

        let mut out = String::with_capacity(width + width / 2 + usize::from(self.negative));
        if self.negative && !self.is_zero() {
            out.push('-');
        }

        for position in 0..width {
            if group > 0 && position > 0 && (width - position) % group == 0 {
                out.push('_');
            }

            if position < padding {
                out.push('0');
            } else {
                let digit_index = width - 1 - position;
                out.push(char::from(b'0' + self.digits[digit_index]));
            }
        }
        out
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.signum(), rhs.signum()) {
            (a, b) if a != b => a.cmp(&b),
            (0, 0) => Ordering::Equal,
            (-1, -1) => self.cmp_magnitude(rhs).reverse(),
            _ => self.cmp_magnitude(rhs),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            let mut result = Self {
                negative: self.negative,
                digits: add_magnitudes(&self.digits, &rhs.digits),
            };
            result.normalize();
            result
        } else {
            match self.cmp_magnitude(rhs) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => {
                    let mut result = Self {
                        negative: self.negative,
                        digits: subtract_magnitudes(&self.digits, &rhs.digits),
                    };
                    result.normalize();
                    result
                }
                Ordering::Less => {
                    let mut result = Self {
                        negative: rhs.negative,
                        digits: subtract_magnitudes(&rhs.digits, &self.digits),
                    };
                    result.normalize();
                    result
                }
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

        let mut result_digits = vec![0u8; self.digits.len() + rhs.digits.len()];
        for (i, &left) in self.digits.iter().enumerate() {
            let mut carry = 0u16;
            for (j, &right) in rhs.digits.iter().enumerate() {
                let index = i + j;
                let product = result_digits[index] as u16
                    + left as u16 * right as u16
                    + carry;
                result_digits[index] = (product % 10) as u8;
                carry = product / 10;
            }

            let mut index = i + rhs.digits.len();
            while carry > 0 {
                if index == result_digits.len() {
                    result_digits.push(0);
                }
                let sum = result_digits[index] as u16 + carry;
                result_digits[index] = (sum % 10) as u8;
                carry = sum / 10;
                index += 1;
            }
        }

        let mut result = Self {
            negative: self.negative != rhs.negative,
            digits: result_digits,
        };
        result.normalize();
        result
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }

        let (quotient_digits, remainder_digits) =
            divide_magnitudes(&self.digits, &rhs.digits);

        let mut quotient = Self {
            negative: self.negative != rhs.negative,
            digits: quotient_digits,
        };
        quotient.normalize();

        let mut remainder = Self {
            negative: self.negative,
            digits: remainder_digits,
        };
        remainder.normalize();

        Ok((quotient, remainder))
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

    fn cmp_magnitude(&self, rhs: &Self) -> Ordering {
        match self.digits.len().cmp(&rhs.digits.len()) {
            Ordering::Equal => self.digits.iter().rev().cmp(rhs.digits.iter().rev()),
            ordering => ordering,
        }
    }
}

fn add_magnitudes(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(left.len().max(right.len()) + 1);
    let mut carry = 0u16;

    for index in 0..left.len().max(right.len()) {
        let sum = left.get(index).copied().unwrap_or(0) as u16
            + right.get(index).copied().unwrap_or(0) as u16
            + carry;
        result.push((sum % 10) as u8);
        carry = sum / 10;
    }

    if carry > 0 {
        result.push(carry as u8);
    }
    result
}

fn subtract_magnitudes(larger: &[u8], smaller: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(larger.len());
    let mut borrow = 0i16;

    for index in 0..larger.len() {
        let mut digit = larger[index] as i16
            - smaller.get(index).copied().unwrap_or(0) as i16
            - borrow;
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

// Multiply a magnitude by one decimal digit.
fn multiply_magnitude_digit(value: &[u8], digit: u8) -> Vec<u8> {
    if digit == 0 {
        return vec![0];
    }

    let mut result = Vec::with_capacity(value.len() + 1);
    let mut carry = 0u16;
    for &current in value {
        let product = current as u16 * digit as u16 + carry;
        result.push((product % 10) as u8);
        carry = product / 10;
    }
    while carry > 0 {
        result.push((carry % 10) as u8);
        carry /= 10;
    }
    result
}

fn normalize_magnitude(value: &mut Vec<u8>) {
    while value.len() > 1 && value.last() == Some(&0) {
        value.pop();
    }
    if value.is_empty() {
        value.push(0);
    }
}

fn compare_magnitudes(left: &[u8], right: &[u8]) -> Ordering {
    match left.len().cmp(&right.len()) {
        Ordering::Equal => left.iter().rev().cmp(right.iter().rev()),
        ordering => ordering,
    }
}

fn divide_magnitudes(dividend: &[u8], divisor: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut quotient_msd = Vec::with_capacity(dividend.len());
    let mut remainder = vec![0u8];

    for &digit in dividend.iter().rev() {
        remainder.insert(0, digit);
        normalize_magnitude(&mut remainder);

        let mut quotient_digit = 0u8;
        for candidate in (1..=9).rev() {
            let product = multiply_magnitude_digit(divisor, candidate);
            if compare_magnitudes(&product, &remainder) != Ordering::Greater {
                quotient_digit = candidate;
                remainder = subtract_magnitudes(&remainder, &product);
                break;
            }
        }
        quotient_msd.push(quotient_digit);
    }

    quotient_msd.reverse();
    normalize_magnitude(&mut quotient_msd);
    normalize_magnitude(&mut remainder);
    (quotient_msd, remainder)
}

fn main() {}