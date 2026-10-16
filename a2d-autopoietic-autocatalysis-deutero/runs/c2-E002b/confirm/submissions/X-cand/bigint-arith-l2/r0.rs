#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    negative: bool,
    // Little-endian decimal digits. Zero is represented as [0] with negative=false.
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

        let mut digit_count = 0usize;
        let mut first_digit_index = None;
        let mut first_digit = None;
        let mut invalid_index = None;

        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte == b'_' {
                continue;
            }
            if byte.is_ascii_digit() {
                digit_count += 1;
                if first_digit_index.is_none() {
                    first_digit_index = Some(index);
                    first_digit = Some(byte);
                }
            } else if invalid_index.is_none() {
                invalid_index = Some(index);
            }
        }

        if let Some(index) = invalid_index {
            return Err(Error::InvalidDigit { index });
        }

        for index in start..bytes.len() {
            if bytes[index] == b'_'
                && (index == start
                    || index + 1 == bytes.len()
                    || !bytes[index - 1].is_ascii_digit()
                    || !bytes[index + 1].is_ascii_digit())
            {
                return Err(Error::Separator { index });
            }
        }

        if digit_count > 1 && first_digit == Some(b'0') {
            return Err(Error::LeadingZero {
                index: first_digit_index.expect("a nonempty magnitude has a digit"),
            });
        }

        if digit_count > max_digits {
            let excess_number = max_digits + 1;
            let mut seen = 0usize;
            for (index, &byte) in bytes.iter().enumerate().skip(start) {
                if byte.is_ascii_digit() {
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
        for &byte in bytes.iter().skip(start) {
            if byte.is_ascii_digit() {
                digits.push(byte - b'0');
            }
        }
        digits.reverse();

        let is_zero = digits.iter().all(|&digit| digit == 0);
        Ok(Self {
            negative: negative && !is_zero,
            digits,
        })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let width = self.digits.len().max(min_digits).max(1);
        let padding = width - self.digits.len();
        let mut out = String::with_capacity(width + width / 3 + usize::from(self.negative));

        if self.negative {
            out.push('-');
        }

        for position in 0..width {
            if group > 0 && position > 0 && (width - position) % group == 0 {
                out.push('_');
            }
            if position < padding {
                out.push('0');
            } else {
                let digit = self.digits[self.digits.len() - 1 - (position - padding)];
                out.push(char::from(b'0' + digit));
            }
        }

        out
    }

    pub fn cmp_value(&self, rhs: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        match (self.negative, rhs.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => compare_magnitude(&self.digits, &rhs.digits),
            (true, true) => compare_magnitude(&rhs.digits, &self.digits),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            Self::from_magnitude(
                self.negative,
                add_magnitudes(&self.digits, &rhs.digits),
            )
        } else {
            match compare_magnitude(&self.digits, &rhs.digits) {
                std::cmp::Ordering::Greater => Self::from_magnitude(
                    self.negative,
                    subtract_magnitudes(&self.digits, &rhs.digits),
                ),
                std::cmp::Ordering::Less => Self::from_magnitude(
                    rhs.negative,
                    subtract_magnitudes(&rhs.digits, &self.digits),
                ),
                std::cmp::Ordering::Equal => Self::zero(),
            }
        }
    }

    pub fn sub(&self, rhs: &Self) -> Self {
        self.add(&rhs.neg())
    }

    pub fn neg(&self) -> Self {
        let mut result = self.clone();
        if !is_zero(&result.digits) {
            result.negative = !result.negative;
        }
        result
    }

    pub fn signum(&self) -> i8 {
        if is_zero(&self.digits) {
            0
        } else if self.negative {
            -1
        } else {
            1
        }
    }

    pub fn mul(&self, rhs: &Self) -> Self {
        if is_zero(&self.digits) || is_zero(&rhs.digits) {
            return Self::zero();
        }

        let mut result = vec![0u8; self.digits.len() + rhs.digits.len()];
        for (i, &left) in self.digits.iter().enumerate() {
            let mut carry = 0u32;
            for (j, &right) in rhs.digits.iter().enumerate() {
                let index = i + j;
                let product = u32::from(left) * u32::from(right)
                    + u32::from(result[index])
                    + carry;
                result[index] = (product % 10) as u8;
                carry = product / 10;
            }

            let mut index = i + rhs.digits.len();
            while carry > 0 {
                if index == result.len() {
                    result.push(0);
                }
                let sum = u32::from(result[index]) + carry;
                result[index] = (sum % 10) as u8;
                carry = sum / 10;
                index += 1;
            }
        }

        Self::from_magnitude(self.negative != rhs.negative, result)
    }

    fn zero() -> Self {
        Self {
            negative: false,
            digits: vec![0],
        }
    }

    fn from_magnitude(negative: bool, mut digits: Vec<u8>) -> Self {
        while digits.len() > 1 && digits.last() == Some(&0) {
            digits.pop();
        }
        if digits.is_empty() {
            digits.push(0);
        }
        let is_zero = is_zero(&digits);
        Self {
            negative: negative && !is_zero,
            digits,
        }
    }
}

fn is_zero(digits: &[u8]) -> bool {
    digits.iter().all(|&digit| digit == 0)
}

fn compare_magnitude(left: &[u8], right: &[u8]) -> std::cmp::Ordering {
    use std::cmp::Ordering;

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

fn add_magnitudes(left: &[u8], right: &[u8]) -> Vec<u8> {
    let length = left.len().max(right.len());
    let mut result = Vec::with_capacity(length + 1);
    let mut carry = 0u16;

    for index in 0..length {
        let a = u16::from(*left.get(index).unwrap_or(&0));
        let b = u16::from(*right.get(index).unwrap_or(&0));
        let sum = a + b + carry;
        result.push((sum % 10) as u8);
        carry = sum / 10;
    }

    if carry > 0 {
        result.push(carry as u8);
    }

    result
}

// Requires left magnitude to be greater than or equal to right.
fn subtract_magnitudes(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(left.len());
    let mut borrow = 0i16;

    for (index, &digit) in left.iter().enumerate() {
        let mut difference = i16::from(digit)
            - i16::from(*right.get(index).unwrap_or(&0))
            - borrow;
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