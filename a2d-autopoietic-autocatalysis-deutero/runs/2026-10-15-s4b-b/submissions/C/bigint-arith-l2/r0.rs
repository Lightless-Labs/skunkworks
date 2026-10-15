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
    digits: Vec<u8>,
}

impl Big {
    pub fn parse(text: &str, max_digits: usize) -> Result<Self, Error> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(Error::Empty);
        }

        let has_sign = bytes[0] == b'+' || bytes[0] == b'-';
        let negative = bytes[0] == b'-';
        let start = usize::from(has_sign);

        if has_sign && bytes.len() == 1 {
            return Err(Error::SignOnly);
        }

        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_' && !byte.is_ascii_digit() {
                return Err(Error::InvalidDigit { index });
            }
        }

        for index in start..bytes.len() {
            if bytes[index] == b'_' {
                let previous_is_digit =
                    index > start && bytes[index - 1].is_ascii_digit();
                let next_is_digit =
                    index + 1 < bytes.len() && bytes[index + 1].is_ascii_digit();
                if !(previous_is_digit && next_is_digit) {
                    return Err(Error::Separator { index });
                }
            }
        }

        let digit_count = bytes[start..]
            .iter()
            .filter(|&&byte| byte != b'_')
            .count();
        let first_digit_index = (start..bytes.len())
            .find(|&index| bytes[index].is_ascii_digit())
            .expect("validated magnitude contains a digit");

        if digit_count > 1 && bytes[first_digit_index] == b'0' {
            return Err(Error::LeadingZero {
                index: first_digit_index,
            });
        }

        if digit_count > max_digits {
            let mut seen = 0usize;
            let mut excess_index = first_digit_index;
            for index in start..bytes.len() {
                if bytes[index].is_ascii_digit() {
                    seen += 1;
                    if seen == max_digits.saturating_add(1) {
                        excess_index = index;
                        break;
                    }
                }
            }
            return Err(Error::TooManyDigits {
                limit: max_digits,
                index: excess_index,
            });
        }

        let mut digits: Vec<u8> = bytes[start..]
            .iter()
            .filter(|&&byte| byte != b'_')
            .map(|&byte| byte - b'0')
            .collect();
        normalize(&mut digits);

        Ok(Self {
            negative: negative && !is_zero(&digits),
            digits,
        })
    }

    pub fn format(&self, min_digits: usize, group: usize) -> String {
        let actual_len = self.digits.len().max(1);
        let total_len = actual_len.max(min_digits);
        let padding = total_len - actual_len;
        let mut magnitude = String::with_capacity(
            total_len.saturating_add(if group > 0 { total_len / group } else { 0 }),
        );

        for position in 0..total_len {
            if group > 0 && position > 0 && (total_len - position) % group == 0 {
                magnitude.push('_');
            }
            if position < padding {
                magnitude.push('0');
            } else {
                magnitude.push((b'0' + self.digits[position - padding]) as char);
            }
        }

        if self.negative {
            let mut result = String::with_capacity(magnitude.len() + 1);
            result.push('-');
            result.push_str(&magnitude);
            result
        } else {
            magnitude
        }
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
            let digits = add_magnitude(&self.digits, &rhs.digits);
            Self {
                negative: self.negative && !is_zero(&digits),
                digits,
            }
        } else {
            match cmp_magnitude(&self.digits, &rhs.digits) {
                Ordering::Greater => {
                    let digits = sub_magnitude(&self.digits, &rhs.digits);
                    Self {
                        negative: self.negative && !is_zero(&digits),
                        digits,
                    }
                }
                Ordering::Less => {
                    let digits = sub_magnitude(&rhs.digits, &self.digits);
                    Self {
                        negative: rhs.negative && !is_zero(&digits),
                        digits,
                    }
                }
                Ordering::Equal => Self::zero(),
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

        let mut accum = vec![0u32; self.digits.len() + rhs.digits.len()];
        for (i, &left) in self.digits.iter().rev().enumerate() {
            for (j, &right) in rhs.digits.iter().rev().enumerate() {
                accum[i + j] += u32::from(left) * u32::from(right);
            }
        }

        for index in 0..accum.len() - 1 {
            let carry = accum[index] / 10;
            accum[index] %= 10;
            accum[index + 1] += carry;
        }

        let mut digits: Vec<u8> = accum.into_iter().rev().map(|value| value as u8).collect();
        normalize(&mut digits);
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

fn normalize(digits: &mut Vec<u8>) {
    match digits.iter().position(|&digit| digit != 0) {
        Some(index) if index > 0 => {
            digits.drain(..index);
        }
        Some(_) => {}
        None => {
            digits.clear();
            digits.push(0);
        }
    }
}

fn is_zero(digits: &[u8]) -> bool {
    digits.len() == 1 && digits[0] == 0
}

fn cmp_magnitude(left: &[u8], right: &[u8]) -> Ordering {
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

fn add_magnitude(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(left.len().max(right.len()) + 1);
    let mut carry = 0u8;
    let mut left_index = left.len();
    let mut right_index = right.len();

    while left_index > 0 || right_index > 0 || carry > 0 {
        let left_digit = if left_index > 0 {
            left_index -= 1;
            left[left_index]
        } else {
            0
        };
        let right_digit = if right_index > 0 {
            right_index -= 1;
            right[right_index]
        } else {
            0
        };
        let sum = left_digit + right_digit + carry;
        result.push(sum % 10);
        carry = sum / 10;
    }

    result.reverse();
    result
}

fn sub_magnitude(larger: &[u8], smaller: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(larger.len());
    let mut borrow = 0i16;
    let mut small_index = smaller.len();

    for index in (0..larger.len()).rev() {
        let small_digit = if small_index > 0 {
            small_index -= 1;
            smaller[small_index] as i16
        } else {
            0
        };
        let mut difference = larger[index] as i16 - small_digit - borrow;
        if difference < 0 {
            difference += 10;
            borrow = 1;
        } else {
            borrow = 0;
        }
        result.push(difference as u8);
    }

    result.reverse();
    normalize(&mut result);
    result
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(text: &str) -> Big {
        Big::parse(text, 10_000).unwrap()
    }

    #[test]
    fn parse_error_precedence_and_offsets() {
        assert_eq!(Big::parse("", 10), Err(Error::Empty));
        assert_eq!(Big::parse("-", 10), Err(Error::SignOnly));
        assert_eq!(Big::parse("_1x", 10), Err(Error::InvalidDigit { index: 2 }));
        assert_eq!(Big::parse("_1", 10), Err(Error::Separator { index: 0 }));
        assert_eq!(Big::parse("01", 10), Err(Error::LeadingZero { index: 0 }));
        assert_eq!(
            Big::parse("1_2_3", 2),
            Err(Error::TooManyDigits { limit: 2, index: 4 })
        );
        assert_eq!(Big::parse("é", 10), Err(Error::InvalidDigit { index: 0 }));
        assert_eq!(Big::parse("0", 0), Err(Error::TooManyDigits { limit: 0, index: 0 }));
    }

    #[test]
    fn formatting_padding_and_grouping() {
        assert_eq!(value("-12").format(5, 2), "-00_012");
        assert_eq!(value("0").format(0, 1), "0");
        assert_eq!(value("12345").format(0, 3), "12_345");
    }

    #[test]
    fn signed_arithmetic_and_zero_canonicalization() {
        let a = value("-999999999999999999999");
        let b = value("999999999999999999999");
        assert_eq!(a.add(&b), value("0"));
        assert_eq!(a.sub(&a), value("0"));
        assert_eq!(value("-12").mul(&value("-7")), value("84"));
        assert_eq!(value("0").mul(&value("-7")), value("0"));
        assert_eq!(value("-12").neg(), value("12"));
        assert_eq!(value("-12").signum(), -1);
    }
}