#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    // Base-10 digits, most significant first; zero is represented as [0].
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
    Underflow,
}

impl Big {
    pub fn parse(text: &str, max_digits: usize) -> Result<Self, Error> {
        let bytes = text.as_bytes();

        if bytes.is_empty() {
            return Err(Error::Empty);
        }

        let start = usize::from(bytes[0] == b'+');
        if start == bytes.len() {
            return Err(Error::SignOnly);
        }

        // Invalid bytes take precedence over malformed separators.
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' && !byte.is_ascii_digit() {
                return Err(Error::InvalidDigit { index });
            }
        }

        let magnitude = &bytes[start..];

        // Separators must have a digit immediately on each side.
        for (offset, &byte) in magnitude.iter().enumerate() {
            if byte == b'_'
                && (offset == 0
                    || offset + 1 == magnitude.len()
                    || !magnitude[offset - 1].is_ascii_digit()
                    || !magnitude[offset + 1].is_ascii_digit())
            {
                return Err(Error::Separator {
                    index: start + offset,
                });
            }
        }

        let digit_count = magnitude.iter().filter(|&&byte| byte != b'_').count();
        let first_digit_index = magnitude
            .iter()
            .position(|&byte| byte != b'_')
            .expect("nonempty magnitude has a digit after validation");

        if digit_count > 1 && magnitude[first_digit_index] == b'0' {
            return Err(Error::LeadingZero {
                index: start + first_digit_index,
            });
        }

        if digit_count > max_digits {
            let excess_digit = max_digits;
            let mut seen = 0usize;
            let mut index = start;
            for (offset, &byte) in magnitude.iter().enumerate() {
                if byte != b'_' {
                    if seen == excess_digit {
                        index = start + offset;
                        break;
                    }
                    seen += 1;
                }
            }
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index,
            });
        }

        let digits = magnitude
            .iter()
            .filter(|&&byte| byte != b'_')
            .map(|&byte| byte - b'0')
            .collect();

        Ok(Self { digits })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let width = self.digits.len().max(min_digits).max(1);
        let padding = width - self.digits.len();
        let mut output = String::with_capacity(width + width / group.max(1));

        for position in 0..width {
            if group > 0 && position > 0 && (width - position) % group == 0 {
                output.push('_');
            }
            if position < padding {
                output.push('0');
            } else {
                output.push(char::from(b'0' + self.digits[position - padding]));
            }
        }

        output
    }

    pub fn cmp_value(&self, rhs: &Self) -> std::cmp::Ordering {
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
            let a = left.next().copied();
            let b = right.next().copied();
            if a.is_none() && b.is_none() && carry == 0 {
                break;
            }

            let sum = a.unwrap_or(0) + b.unwrap_or(0) + carry;
            result.push(sum % 10);
            carry = sum / 10;
        }

        result.reverse();
        Self { digits: result }
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, Error> {
        if self.cmp_value(rhs) == std::cmp::Ordering::Less {
            return Err(Error::Underflow);
        }

        let mut result = Vec::with_capacity(self.digits.len());
        let mut borrow = 0i16;
        let mut right = rhs.digits.iter().rev();

        for &digit in self.digits.iter().rev() {
            let mut difference = i16::from(digit) - borrow;
            let other = i16::from(right.next().copied().unwrap_or(0));
            if difference < other {
                difference += 10;
                borrow = 1;
            } else {
                borrow = 0;
            }
            result.push((difference - other) as u8);
        }

        while result.len() > 1 && result.last() == Some(&0) {
            result.pop();
        }

        result.reverse();
        Ok(Self { digits: result })
    }
}

fn main() {}