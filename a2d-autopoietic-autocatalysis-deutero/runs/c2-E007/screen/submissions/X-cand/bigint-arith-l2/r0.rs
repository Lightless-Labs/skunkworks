use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Empty,
    SignOnly,
    InvalidDigit { index: usize },
    Separator { index: usize },
    LeadingZero { index: usize },
    TooManyDigits { limit: usize, index: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    negative: bool,
    // Little-endian base-10 digits; zero is represented as [0].
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

        let mut invalid = None;
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' && !byte.is_ascii_digit() {
                invalid = Some(index);
                break;
            }
        }
        if let Some(index) = invalid {
            return Err(Error::InvalidDigit { index });
        }

        let mut digit_count = 0usize;
        let mut first_digit_index = None;
        let mut first_digit = 0u8;
        let mut first_bad_separator = None;

        for index in start..bytes.len() {
            match bytes[index] {
                b'_' => {
                    let before_is_digit =
                        index > start && bytes[index - 1].is_ascii_digit();
                    let after_is_digit =
                        index + 1 < bytes.len() && bytes[index + 1].is_ascii_digit();
                    if (!before_is_digit || !after_is_digit)
                        && first_bad_separator.is_none()
                    {
                        first_bad_separator = Some(index);
                    }
                }
                byte => {
                    if first_digit_index.is_none() {
                        first_digit_index = Some(index);
                        first_digit = byte - b'0';
                    }
                    digit_count += 1;
                }
            }
        }

        if let Some(index) = first_bad_separator {
            return Err(Error::Separator { index });
        }

        if digit_count > 1 && first_digit == 0 {
            return Err(Error::LeadingZero {
                index: first_digit_index.expect("a nonempty magnitude has a digit"),
            });
        }

        if digit_count > max_digits {
            let excess_number = max_digits + 1;
            let mut seen = 0usize;
            for index in start..bytes.len() {
                if bytes[index].is_ascii_digit() {
                    seen += 1;
                    if seen == excess_number {
                        return Err(Error::TooManyDigits {
                            limit: max_digits,
                            index,
                        });
                    }
                }
            }
        }

        let mut digits = Vec::with_capacity(digit_count);
        for &byte in bytes[start..].iter().rev() {
            if byte.is_ascii_digit() {
                digits.push(byte - b'0');
            }
        }
        if digits.is_empty() {
            digits.push(0);
        }

        let mut value = Self { negative, digits };
        value.normalize();
        Ok(value)
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let magnitude_len = self.digits.len().max(1);
        let total_len = magnitude_len.max(min_digits);
        let mut output = String::new();

        if self.negative && !self.is_zero() {
            output.push('-');
        }

        for position in 0..total_len {
            let remaining = total_len - position;
            if group > 0 && position > 0 && remaining % group == 0 {
                output.push('_');
            }

            let digit = if position < total_len - magnitude_len {
                0
            } else {
                let magnitude_position = position - (total_len - magnitude_len);
                self.digits[magnitude_len - 1 - magnitude_position]
            };
            output.push(char::from(b'0' + digit));
        }

        output
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
        match (self.signum(), rhs.signum()) {
            (0, _) => rhs.clone(),
            (_, 0) => self.clone(),
            (a, b) if a == b => {
                let mut result = Self {
                    negative: a < 0,
                    digits: Self::add_magnitudes(&self.digits, &rhs.digits),
                };
                result.normalize();
                result
            }
            _ => match self.cmp_magnitude(rhs) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => {
                    let mut result = Self {
                        negative: self.negative,
                        digits: Self::subtract_magnitudes(&self.digits, &rhs.digits),
                    };
                    result.normalize();
                    result
                }
                Ordering::Less => {
                    let mut result = Self {
                        negative: rhs.negative,
                        digits: Self::subtract_magnitudes(&rhs.digits, &self.digits),
                    };
                    result.normalize();
                    result
                }
            },
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

        let mut product = vec![0u8; self.digits.len() + rhs.digits.len()];
        for (i, &left) in self.digits.iter().enumerate() {
            let mut carry = 0u16;
            for (j, &right) in rhs.digits.iter().enumerate() {
                let position = i + j;
                let value = product[position] as u16 + left as u16 * right as u16 + carry;
                product[position] = (value % 10) as u8;
                carry = value / 10;
            }

            let mut position = i + rhs.digits.len();
            while carry > 0 {
                if position == product.len() {
                    product.push(0);
                }
                let value = product[position] as u16 + carry;
                product[position] = (value % 10) as u8;
                carry = value / 10;
                position += 1;
            }
        }

        let mut result = Self {
            negative: self.negative != rhs.negative,
            digits: product,
        };
        result.normalize();
        result
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
            Ordering::Equal => {
                for (left, right) in self.digits.iter().rev().zip(rhs.digits.iter().rev()) {
                    match left.cmp(right) {
                        Ordering::Equal => {}
                        ordering => return ordering,
                    }
                }
                Ordering::Equal
            }
            ordering => ordering,
        }
    }

    fn add_magnitudes(left: &[u8], right: &[u8]) -> Vec<u8> {
        let mut result = Vec::with_capacity(left.len().max(right.len()) + 1);
        let mut carry = 0u8;

        for index in 0..left.len().max(right.len()) {
            let sum = left.get(index).copied().unwrap_or(0)
                + right.get(index).copied().unwrap_or(0)
                + carry;
            result.push(sum % 10);
            carry = sum / 10;
        }

        if carry > 0 {
            result.push(carry);
        }
        result
    }

    // Requires left magnitude to be at least right magnitude.
    fn subtract_magnitudes(left: &[u8], right: &[u8]) -> Vec<u8> {
        let mut result = Vec::with_capacity(left.len());
        let mut borrow = 0i8;

        for index in 0..left.len() {
            let mut difference =
                left[index] as i8 - right.get(index).copied().unwrap_or(0) as i8 - borrow;
            if difference < 0 {
                difference += 10;
                borrow = 1;
            } else {
                borrow = 0;
            }
            result.push(difference as u8);
        }

        while result.len() > 1 && result.last() == Some(&0) {
            result.pop();
        }
        result
    }
}