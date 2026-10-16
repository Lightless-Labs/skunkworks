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
    // Little-endian decimal digits. Zero is represented as [0] with negative=false.
    digits: Vec<u8>,
}

impl Big {
    pub fn parse(text: &str, max_digits: usize) -> Result<Self, Error> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(Error::Empty);
        }

        let (negative, start) = match bytes[0] {
            b'-' => (true, 1),
            b'+' => (false, 1),
            _ => (false, 0),
        };
        if start == bytes.len() {
            return Err(Error::SignOnly);
        }

        let magnitude = &bytes[start..];

        // Invalid bytes take precedence over malformed separators.
        if let Some(offset) = magnitude
            .iter()
            .position(|&byte| byte != b'_' && !byte.is_ascii_digit())
        {
            return Err(Error::InvalidDigit { index: start + offset });
        }

        // Record digit positions and find the first malformed separator.
        let mut digit_positions = Vec::new();
        let mut separator_error = None;
        for i in 0..magnitude.len() {
            if magnitude[i].is_ascii_digit() {
                digit_positions.push(i);
            } else if magnitude[i] == b'_' {
                let valid_before = i > 0 && magnitude[i - 1].is_ascii_digit();
                let valid_after = i + 1 < magnitude.len() && magnitude[i + 1].is_ascii_digit();
                if !(valid_before && valid_after) && separator_error.is_none() {
                    separator_error = Some(start + i);
                }
            }
        }
        if let Some(index) = separator_error {
            return Err(Error::Separator { index });
        }

        let digit_count = digit_positions.len();
        if digit_count > 1 {
            let first = digit_positions[0];
            if magnitude[first] == b'0' {
                return Err(Error::LeadingZero { index: start + first });
            }
        }

        if digit_count > max_digits {
            let excess_position = digit_positions[max_digits];
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: start + excess_position,
            });
        }

        let mut digits = Vec::with_capacity(digit_count);
        for &position in digit_positions.iter().rev() {
            digits.push(magnitude[position] - b'0');
        }
        normalize(&mut digits);

        Ok(Self {
            negative: negative && !is_zero_digits(&digits),
            digits,
        })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let actual_len = self.digits.len().max(1);
        let width = actual_len.max(min_digits);
        let padding = width - actual_len;

        let mut result = String::with_capacity(width + width / 2 + usize::from(self.negative));
        if self.negative {
            result.push('-');
        }

        for position in (0..width).rev() {
            if group > 0 && position < width - 1 && (width - position) % group == 0 {
                result.push('_');
            }
            let digit = if position < padding {
                0
            } else {
                self.digits[position - padding]
            };
            result.push(char::from(b'0' + digit));
        }
        result
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.negative, rhs.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => compare_magnitude(&self.digits, &rhs.digits),
            (true, true) => compare_magnitude(&rhs.digits, &self.digits),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            let digits = add_magnitude(&self.digits, &rhs.digits);
            Self::from_parts(self.negative, digits)
        } else {
            match compare_magnitude(&self.digits, &rhs.digits) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => {
                    let digits = subtract_magnitude(&self.digits, &rhs.digits);
                    Self::from_parts(self.negative, digits)
                }
                Ordering::Less => {
                    let digits = subtract_magnitude(&rhs.digits, &self.digits);
                    Self::from_parts(rhs.negative, digits)
                }
            }
        }
    }

    pub fn sub(&self, rhs: &Self) -> Self {
        self.add(&rhs.neg())
    }

    pub fn neg(&self) -> Self {
        Self {
            negative: !self.negative && !self.is_zero(),
            digits: self.digits.clone(),
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
        for (i, &left) in self.digits.iter().enumerate() {
            let mut carry = 0u16;
            for (j, &right) in rhs.digits.iter().enumerate() {
                let index = i + j;
                let total = u16::from(result[index]) + u16::from(left) * u16::from(right) + carry;
                result[index] = (total % 10) as u8;
                carry = total / 10;
            }

            let mut index = i + rhs.digits.len();
            while carry > 0 {
                if index == result.len() {
                    result.push(0);
                }
                let total = u16::from(result[index]) + carry;
                result[index] = (total % 10) as u8;
                carry = total / 10;
                index += 1;
            }
        }

        Self::from_parts(self.negative != rhs.negative, result)
    }

    fn from_parts(negative: bool, mut digits: Vec<u8>) -> Self {
        normalize(&mut digits);
        let zero = is_zero_digits(&digits);
        Self {
            negative: negative && !zero,
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
        is_zero_digits(&self.digits)
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

fn is_zero_digits(digits: &[u8]) -> bool {
    digits.len() == 1 && digits[0] == 0
}

fn compare_magnitude(left: &[u8], right: &[u8]) -> Ordering {
    match left.len().cmp(&right.len()) {
        Ordering::Equal => {
            for (&a, &b) in left.iter().rev().zip(right.iter().rev()) {
                match a.cmp(&b) {
                    Ordering::Equal => {}
                    other => return other,
                }
            }
            Ordering::Equal
        }
        other => other,
    }
}

fn add_magnitude(left: &[u8], right: &[u8]) -> Vec<u8> {
    let length = left.len().max(right.len());
    let mut result = Vec::with_capacity(length + 1);
    let mut carry = 0u8;

    for i in 0..length {
        let a = left.get(i).copied().unwrap_or(0);
        let b = right.get(i).copied().unwrap_or(0);
        let sum = a + b + carry;
        result.push(sum % 10);
        carry = sum / 10;
    }

    if carry != 0 {
        result.push(carry);
    }
    result
}

// Requires left magnitude to be at least right magnitude.
fn subtract_magnitude(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(left.len());
    let mut borrow = 0i8;

    for (i, &left_digit) in left.iter().enumerate() {
        let right_digit = right.get(i).copied().unwrap_or(0);
        let mut difference = i16::from(left_digit) - i16::from(right_digit) - i16::from(borrow);
        if difference < 0 {
            difference += 10;
            borrow = 1;
        } else {
            borrow = 0;
        }
        result.push(difference as u8);
    }

    normalize(&mut result);
    result
}