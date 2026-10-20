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
    // Little-endian base-10 digits, with zero represented as [0].
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

        // Scan every magnitude byte for invalid characters first.
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' && !byte.is_ascii_digit() {
                return Err(Error::InvalidDigit { index });
            }
        }

        // Then validate separators.
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

        let mut first_digit_index = None;
        let mut digit_count = 0usize;
        let mut first_digit = 0u8;

        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' {
                if first_digit_index.is_none() {
                    first_digit_index = Some(index);
                    first_digit = byte - b'0';
                }
                digit_count += 1;
            }
        }

        let first_index = first_digit_index.expect("nonempty magnitude with valid separators");
        if digit_count > 1 && first_digit == 0 {
            return Err(Error::LeadingZero { index: first_index });
        }

        if digit_count > max_digits {
            let target = max_digits + 1;
            let mut seen = 0usize;
            for (index, &byte) in bytes.iter().enumerate().skip(start) {
                if byte != b'_' {
                    seen += 1;
                    if seen == target {
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
            if byte != b'_' {
                digits.push(byte - b'0');
            }
        }
        digits.reverse();

        Ok(Self { digits })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let natural_len = self.digits.len().max(1);
        let width = natural_len.max(min_digits);
        let padding = width - natural_len;
        let mut out = String::new();

        for position in 0..width {
            if group > 0 && position > 0 && (width - position) % group == 0 {
                out.push('_');
            }

            if position < padding {
                out.push('0');
            } else {
                let digit_from_left = position - padding;
                let digit_index = natural_len - 1 - digit_from_left;
                out.push((b'0' + self.digits[digit_index]) as char);
            }
        }

        out
    }

    pub fn cmp_value(&self, rhs: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        match self.digits.len().cmp(&rhs.digits.len()) {
            Ordering::Equal => {
                for (left, right) in self.digits.iter().rev().zip(rhs.digits.iter().rev()) {
                    match left.cmp(right) {
                        Ordering::Equal => {}
                        non_equal => return non_equal,
                    }
                }
                Ordering::Equal
            }
            non_equal => non_equal,
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        let max_len = self.digits.len().max(rhs.digits.len());
        let mut digits = Vec::with_capacity(max_len + 1);
        let mut carry = 0u8;

        for i in 0..max_len {
            let left = self.digits.get(i).copied().unwrap_or(0);
            let right = rhs.digits.get(i).copied().unwrap_or(0);
            let sum = left + right + carry;
            digits.push(sum % 10);
            carry = sum / 10;
        }

        if carry != 0 {
            digits.push(carry);
        }

        Self { digits }
    }

    pub fn sub(&self, rhs: &Self) -> Result<Self, Error> {
        if self.cmp_value(rhs) == std::cmp::Ordering::Less {
            return Err(Error::Underflow);
        }

        let mut digits = Vec::with_capacity(self.digits.len());
        let mut borrow = 0i8;

        for i in 0..self.digits.len() {
            let left = self.digits[i] as i8 - borrow;
            let right = rhs.digits.get(i).copied().unwrap_or(0) as i8;

            if left < right {
                digits.push((left + 10 - right) as u8);
                borrow = 1;
            } else {
                digits.push((left - right) as u8);
                borrow = 0;
            }
        }

        while digits.len() > 1 && digits.last() == Some(&0) {
            digits.pop();
        }

        Ok(Self { digits })
    }
}