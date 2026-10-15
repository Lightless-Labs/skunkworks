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
    // Little-endian decimal digits. Zero is [0] and is never negative.
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

        if let Some(pos) = magnitude
            .iter()
            .position(|&b| b != b'_' && !b.is_ascii_digit())
        {
            return Err(Error::InvalidDigit { index: start + pos });
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

        let digit_positions: Vec<usize> = magnitude
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b != b'_').then_some(i))
            .collect();

        if digit_positions.len() > 1 && magnitude[digit_positions[0]] == b'0' {
            return Err(Error::LeadingZero {
                index: start + digit_positions[0],
            });
        }

        if digit_positions.len() > max_digits {
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: start + digit_positions[max_digits],
            });
        }

        let mut digits: Vec<u8> = magnitude
            .iter()
            .filter(|&&b| b != b'_')
            .map(|&b| b - b'0')
            .collect();
        digits.reverse();

        Ok(Self::from_parts(negative, digits))
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let actual_len = self.digits.len().max(1);
        let len = actual_len.max(min_digits);
        let padding = len - actual_len;
        let separators = if group > 0 { (len - 1) / group } else { 0 };
        let mut out = String::with_capacity(
            len + separators + usize::from(self.negative),
        );

        if self.negative {
            out.push('-');
        }

        for pos in 0..len {
            if group > 0 && pos > 0 && (len - pos) % group == 0 {
                out.push('_');
            }

            let digit = if pos < padding {
                0
            } else {
                self.digits[len - pos - 1 - padding]
            };
            out.push(char::from(b'0' + digit));
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
        match (self.negative, rhs.negative) {
            (false, false) => Self::from_parts(false, add_magnitude(&self.digits, &rhs.digits)),
            (true, true) => Self::from_parts(true, add_magnitude(&self.digits, &rhs.digits)),
            _ => match cmp_magnitude(&self.digits, &rhs.digits) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => Self::from_parts(
                    self.negative,
                    sub_magnitude(&self.digits, &rhs.digits),
                ),
                Ordering::Less => Self::from_parts(
                    rhs.negative,
                    sub_magnitude(&rhs.digits, &self.digits),
                ),
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
        Self::from_parts(
            self.negative ^ rhs.negative,
            mul_magnitude(&self.digits, &rhs.digits),
        )
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }

        let (quotient, remainder) = div_rem_magnitude(&self.digits, &rhs.digits);
        Ok((
            Self::from_parts(self.negative ^ rhs.negative, quotient),
            Self::from_parts(self.negative, remainder),
        ))
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

    fn from_parts(negative: bool, mut digits: Vec<u8>) -> Self {
        trim_magnitude(&mut digits);
        let is_zero = digits.len() == 1 && digits[0] == 0;
        Self {
            negative: negative && !is_zero,
            digits,
        }
    }
}

fn trim_magnitude(digits: &mut Vec<u8>) {
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
                    other => return other,
                }
            }
            Ordering::Equal
        }
        other => other,
    }
}

fn add_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(a.len().max(b.len()) + 1);
    let mut carry = 0u8;

    for i in 0..a.len().max(b.len()) {
        let sum = a.get(i).copied().unwrap_or(0)
            + b.get(i).copied().unwrap_or(0)
            + carry;
        out.push(sum % 10);
        carry = sum / 10;
    }
    if carry != 0 {
        out.push(carry);
    }
    out
}

// Requires a >= b.
fn sub_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(a.len());
    let mut borrow = 0i8;

    for i in 0..a.len() {
        let mut digit =
            a[i] as i8 - b.get(i).copied().unwrap_or(0) as i8 - borrow;
        if digit < 0 {
            digit += 10;
            borrow = 1;
        } else {
            borrow = 0;
        }
        out.push(digit as u8);
    }

    trim_magnitude(&mut out);
    out
}

fn mul_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; a.len() + b.len()];

    for (i, &da) in a.iter().enumerate() {
        let mut carry = 0u16;
        for (j, &db) in b.iter().enumerate() {
            let k = i + j;
            let value = out[k] as u16 + da as u16 * db as u16 + carry;
            out[k] = (value % 10) as u8;
            carry = value / 10;
        }

        let mut k = i + b.len();
        while carry != 0 {
            if k == out.len() {
                out.push(0);
            }
            let value = out[k] as u16 + carry;
            out[k] = (value % 10) as u8;
            carry = value / 10;
            k += 1;
        }
    }

    trim_magnitude(&mut out);
    out
}

fn div_rem_magnitude(dividend: &[u8], divisor: &[u8]) -> (Vec<u8>, Vec<u8>) {
    if cmp_magnitude(dividend, divisor) == Ordering::Less {
        return (vec![0], dividend.to_vec());
    }

    let mut quotient_be = Vec::with_capacity(dividend.len());
    let mut remainder = vec![0u8];

    for &digit in dividend.iter().rev() {
        if remainder.len() == 1 && remainder[0] == 0 {
            remainder[0] = digit;
        } else {
            remainder.insert(0, digit);
        }
        trim_magnitude(&mut remainder);

        let mut qdigit = 0u8;
        while cmp_magnitude(&remainder, divisor) != Ordering::Less {
            remainder = sub_magnitude(&remainder, divisor);
            qdigit += 1;
        }
        quotient_be.push(qdigit);
    }

    quotient_be.reverse();
    trim_magnitude(&mut quotient_be);
    trim_magnitude(&mut remainder);
    (quotient_be, remainder)
}