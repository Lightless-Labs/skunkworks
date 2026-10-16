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
    // Little-endian base-10 digits; zero is represented by [0].
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

        let magnitude = &bytes[start..];

        if let Some(offset) = magnitude
            .iter()
            .enumerate()
            .find_map(|(i, &b)| (b != b'_' && !b.is_ascii_digit()).then_some(i))
        {
            return Err(Error::InvalidDigit {
                index: start + offset,
            });
        }

        for i in 0..magnitude.len() {
            if magnitude[i] == b'_'
                && (i == 0
                    || i + 1 == magnitude.len()
                    || !magnitude[i - 1].is_ascii_digit()
                    || !magnitude[i + 1].is_ascii_digit())
            {
                return Err(Error::Separator { index: start + i });
            }
        }

        let first_digit = magnitude.iter().position(|&b| b != b'_').unwrap();
        let digit_count = magnitude.iter().filter(|&&b| b != b'_').count();

        if digit_count > 1 && magnitude[first_digit] == b'0' {
            return Err(Error::LeadingZero {
                index: start + first_digit,
            });
        }

        if digit_count > max_digits {
            let excess = max_digits;
            let offset = magnitude
                .iter()
                .enumerate()
                .filter(|(_, b)| **b != b'_')
                .nth(excess)
                .map(|(i, _)| i)
                .unwrap();
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: start + offset,
            });
        }

        let mut digits = Vec::with_capacity(digit_count);
        for &b in magnitude.iter().rev() {
            if b != b'_' {
                digits.push(b - b'0');
            }
        }

        let is_zero = digits.iter().all(|&d| d == 0);
        Ok(Self {
            negative: negative && !is_zero,
            digits,
        })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let digit_len = self.digits.len().max(min_digits).max(1);
        let mut out = String::with_capacity(digit_len + digit_len / 3 + 1);

        if self.negative {
            out.push('-');
        }

        for i in (0..digit_len).rev() {
            if group > 0 && i < digit_len - 1 && (i + 1) % group == 0 {
                out.push('_');
            }

            let digit = if i < self.digits.len() {
                self.digits[i]
            } else {
                0
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
            let digits = add_magnitudes(&self.digits, &rhs.digits);
            Self {
                negative: self.negative && !is_zero(&digits),
                digits,
            }
        } else {
            match cmp_magnitude(&self.digits, &rhs.digits) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => {
                    let digits = sub_magnitudes(&self.digits, &rhs.digits);
                    Self {
                        negative: self.negative && !is_zero(&digits),
                        digits,
                    }
                }
                Ordering::Less => {
                    let digits = sub_magnitudes(&rhs.digits, &self.digits);
                    Self {
                        negative: rhs.negative && !is_zero(&digits),
                        digits,
                    }
                }
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

        let mut digits = vec![0u8; self.digits.len() + rhs.digits.len()];
        for (i, &a) in self.digits.iter().enumerate() {
            let mut carry = 0u16;
            for (j, &b) in rhs.digits.iter().enumerate() {
                let index = i + j;
                let total = digits[index] as u16 + a as u16 * b as u16 + carry;
                digits[index] = (total % 10) as u8;
                carry = total / 10;
            }

            let mut index = i + rhs.digits.len();
            while carry > 0 {
                let total = digits[index] as u16 + carry;
                digits[index] = (total % 10) as u8;
                carry = total / 10;
                index += 1;
                if index == digits.len() && carry > 0 {
                    digits.push(0);
                }
            }
        }

        trim_zeros(&mut digits);
        Self {
            negative: self.negative != rhs.negative,
            digits,
        }
    }

    fn zero() -> Self {
        Self {
            negative: false,
            digits: vec![0],
        }
    }
}

fn is_zero(digits: &[u8]) -> bool {
    digits.iter().all(|&d| d == 0)
}

fn trim_zeros(digits: &mut Vec<u8>) {
    while digits.len() > 1 && digits.last() == Some(&0) {
        digits.pop();
    }
}

fn cmp_magnitude(a: &[u8], b: &[u8]) -> Ordering {
    match a.len().cmp(&b.len()) {
        Ordering::Equal => {
            for (&x, &y) in a.iter().rev().zip(b.iter().rev()) {
                match x.cmp(&y) {
                    Ordering::Equal => {}
                    other => return other,
                }
            }
            Ordering::Equal
        }
        other => other,
    }
}

fn add_magnitudes(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(a.len().max(b.len()) + 1);
    let mut carry = 0u8;

    for i in 0..a.len().max(b.len()) {
        let total = a.get(i).copied().unwrap_or(0) + b.get(i).copied().unwrap_or(0) + carry;
        result.push(total % 10);
        carry = total / 10;
    }

    if carry > 0 {
        result.push(carry);
    }
    result
}

// Requires a >= b.
fn sub_magnitudes(a: &[u8], b: &[u8]) -> Vec<u8> {
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

    trim_zeros(&mut result);
    result
}