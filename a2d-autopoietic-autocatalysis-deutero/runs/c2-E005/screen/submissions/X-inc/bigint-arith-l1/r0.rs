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
    // Little-endian base-10 digits, with at least one digit and no high zeroes.
    digits: Vec<u8>,
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

        if let Some(index) = (start..bytes.len()).find(|&i| {
            let b = bytes[i];
            b != b'_' && !b.is_ascii_digit()
        }) {
            return Err(Error::InvalidDigit { index });
        }

        if let Some(index) = (start..bytes.len()).find(|&i| {
            if bytes[i] != b'_' {
                return false;
            }
            i == start
                || i + 1 == bytes.len()
                || !bytes[i - 1].is_ascii_digit()
                || !bytes[i + 1].is_ascii_digit()
        }) {
            return Err(Error::Separator { index });
        }

        let first_digit = (start..bytes.len()).find(|&i| bytes[i] != b'_').unwrap();
        let digit_count = bytes[start..].iter().filter(|&&b| b != b'_').count();

        if digit_count > 1 && bytes[first_digit] == b'0' {
            return Err(Error::LeadingZero { index: first_digit });
        }

        if digit_count > max_digits {
            let excess_number = max_digits + 1;
            let mut seen = 0usize;
            let index = (start..bytes.len())
                .find(|&i| {
                    if bytes[i] == b'_' {
                        false
                    } else {
                        seen += 1;
                        seen == excess_number
                    }
                })
                .unwrap_or(first_digit);
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index,
            });
        }

        let digits = bytes[start..]
            .iter()
            .filter(|&&b| b != b'_')
            .rev()
            .map(|&b| b - b'0')
            .collect();
        Ok(Self::from_digits(digits))
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let magnitude_len = self.digits.len().max(min_digits).max(1);
        let mut out = String::with_capacity(
            magnitude_len + if group > 0 { (magnitude_len - 1) / group } else { 0 },
        );

        for pos in (0..magnitude_len).rev() {
            if group > 0 && pos != magnitude_len - 1 && (pos + 1) % group == 0 {
                out.push('_');
            }
            let digit = if pos < self.digits.len() {
                self.digits[pos]
            } else {
                0
            };
            out.push(char::from(b'0' + digit));
        }
        out
    }

    pub fn cmp_value(&self, rhs: &Self) -> std::cmp::Ordering {
        self.digits
            .len()
            .cmp(&rhs.digits.len())
            .then_with(|| self.digits.iter().rev().cmp(rhs.digits.iter().rev()))
    }

    pub fn add(&self, rhs: &Self) -> Self {
        let len = self.digits.len().max(rhs.digits.len());
        let mut result = Vec::with_capacity(len + 1);
        let mut carry = 0u8;

        for i in 0..len {
            let a = self.digits.get(i).copied().unwrap_or(0);
            let b = rhs.digits.get(i).copied().unwrap_or(0);
            let sum = a + b + carry;
            result.push(sum % 10);
            carry = sum / 10;
        }
        if carry != 0 {
            result.push(carry);
        }

        Self::from_digits(result)
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, Error> {
        if self.cmp_value(rhs) == std::cmp::Ordering::Less {
            return Err(Error::Underflow);
        }

        let mut result = Vec::with_capacity(self.digits.len());
        let mut borrow = 0i8;

        for i in 0..self.digits.len() {
            let a = self.digits[i] as i8 - borrow;
            let b = rhs.digits.get(i).copied().unwrap_or(0) as i8;
            if a < b {
                result.push((a + 10 - b) as u8);
                borrow = 1;
            } else {
                result.push((a - b) as u8);
                borrow = 0;
            }
        }

        Ok(Self::from_digits(result))
    }

    fn from_digits(mut digits: Vec<u8>) -> Self {
        while digits.len() > 1 && digits.last() == Some(&0) {
            digits.pop();
        }
        if digits.is_empty() {
            digits.push(0);
        }
        Self { digits }
    }
}