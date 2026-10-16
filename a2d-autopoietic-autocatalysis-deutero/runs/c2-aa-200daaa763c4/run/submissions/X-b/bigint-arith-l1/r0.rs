use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Empty,
    SignOnly,
    InvalidDigit { index: usize },
    Separator { index: usize },
    LeadingZero { index: usize },
    TooManyDigits { limit: usize, index: usize },
    Underflow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    digits: Vec<u8>,
}

impl Big {
    pub fn parse(text: &str, max_digits: usize) -> Result<Self, Error> {
        let bytes = text.as_bytes();

        if bytes.is_empty() {
            return Err(Error::Empty);
        }

        let start = if bytes[0] == b'+' { 1 } else { 0 };
        if start == bytes.len() {
            return Err(Error::SignOnly);
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

        let first_digit = (start..bytes.len())
            .find(|&i| bytes[i] != b'_')
            .unwrap();
        let digit_count = bytes[start..].iter().filter(|&&b| b != b'_').count();

        if digit_count > 1 && bytes[first_digit] == b'0' {
            return Err(Error::LeadingZero { index: first_digit });
        }

        if digit_count > max_digits {
            let index = bytes[start..]
                .iter()
                .enumerate()
                .filter(|&(_, &b)| b != b'_')
                .nth(max_digits)
                .map(|(offset, _)| start + offset)
                .unwrap();

            return Err(Error::TooManyDigits {
                limit: max_digits,
                index,
            });
        }

        let digits = bytes[start..]
            .iter()
            .filter(|&&b| b != b'_')
            .map(|&b| b - b'0')
            .collect();

        Ok(Self::from_digits(digits))
    }

    fn from_digits(mut digits: Vec<u8>) -> Self {
        let first_nonzero = digits.iter().position(|&d| d != 0);
        match first_nonzero {
            Some(index) => {
                if index > 0 {
                    digits.drain(..index);
                }
            }
            None => digits = vec![0],
        }
        Self { digits }
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let width = self.digits.len().max(min_digits).max(1);
        let padding = width - self.digits.len();
        let mut out = String::with_capacity(width + if group > 0 { width / group } else { 0 });

        for position in 0..width {
            if group > 0 && position > 0 && (width - position) % group == 0 {
                out.push('_');
            }
            if position < padding {
                out.push('0');
            } else {
                out.push((b'0' + self.digits[position - padding]) as char);
            }
        }

        out
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        self.digits
            .len()
            .cmp(&rhs.digits.len())
            .then_with(|| self.digits.cmp(&rhs.digits))
    }

    pub fn add(&self, rhs: &Self) -> Self {
        let mut result = Vec::with_capacity(self.digits.len().max(rhs.digits.len()) + 1);
        let mut carry = 0u8;
        let mut left = self.digits.iter().rev();
        let mut right = rhs.digits.iter().rev();

        loop {
            match (left.next(), right.next()) {
                (None, None) => break,
                (a, b) => {
                    let sum = a.copied().unwrap_or(0) + b.copied().unwrap_or(0) + carry;
                    result.push(sum % 10);
                    carry = sum / 10;
                }
            }
        }

        if carry != 0 {
            result.push(carry);
        }
        result.reverse();
        Self { digits: result }
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, Error> {
        if self.cmp_value(rhs) == Ordering::Less {
            return Err(Error::Underflow);
        }

        let mut result = Vec::with_capacity(self.digits.len());
        let mut borrow = 0i16;
        let mut right = rhs.digits.iter().rev();

        for &a in self.digits.iter().rev() {
            let mut digit = a as i16 - borrow;
            let b = right.next().copied().unwrap_or(0) as i16;
            if digit < b {
                digit += 10;
                borrow = 1;
            } else {
                borrow = 0;
            }
            result.push((digit - b) as u8);
        }

        result.reverse();
        Ok(Self::from_digits(result))
    }
}