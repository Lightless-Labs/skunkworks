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
    InvalidRadix { radix: u32 },
    ExponentTooLarge { exponent: u32, limit: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big {
    negative: bool,
    digits: Vec<u8>,
}

impl Big {
    fn zero() -> Self {
        Self {
            negative: false,
            digits: vec![0],
        }
    }

    fn one() -> Self {
        Self {
            negative: false,
            digits: vec![1],
        }
    }

    fn normalize(&mut self) {
        while self.digits.len() > 1 && self.digits.last() == Some(&0) {
            self.digits.pop();
        }
        if self.digits.len() == 1 && self.digits[0] == 0 {
            self.negative = false;
        }
    }

    fn digit_value(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'z' => Some(byte - b'a' + 10),
            b'A'..=b'Z' => Some(byte - b'A' + 10),
            _ => None,
        }
    }

    fn valid_radix(radix: u32) -> Result<(), Error> {
        if (2..=36).contains(&radix) {
            Ok(())
        } else {
            Err(Error::InvalidRadix { radix })
        }
    }

    pub fn parse(text: &str, radix: u32, max_digits: usize) -> Result<Self, Error> {
        Self::valid_radix(radix)?;
        if text.is_empty() {
            return Err(Error::Empty);
        }

        let bytes = text.as_bytes();
        let (negative, start) = match bytes[0] {
            b'+' => (false, 1),
            b'-' => (true, 1),
            _ => (false, 0),
        };
        if start == bytes.len() {
            return Err(Error::SignOnly);
        }

        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            if byte != b'_'
                && Self::digit_value(byte).is_none_or(|d| u32::from(d) >= radix)
            {
                return Err(Error::InvalidDigit { index });
            }
        }

        for index in start..bytes.len() {
            if bytes[index] == b'_' {
                let preceding_digit = index > start
                    && bytes[index - 1] != b'_'
                    && Self::digit_value(bytes[index - 1])
                        .is_some_and(|d| u32::from(d) < radix);
                let following_digit = index + 1 < bytes.len()
                    && bytes[index + 1] != b'_'
                    && Self::digit_value(bytes[index + 1])
                        .is_some_and(|d| u32::from(d) < radix);
                if !(preceding_digit && following_digit) {
                    return Err(Error::Separator { index });
                }
            }
        }

        let digit_count = bytes[start..].iter().filter(|&&b| b != b'_').count();
        let first_digit_index = (start..bytes.len()).find(|&i| bytes[i] != b'_').unwrap();
        let first_digit = Self::digit_value(bytes[first_digit_index]).unwrap();
        if digit_count > 1 && first_digit == 0 {
            return Err(Error::LeadingZero {
                index: first_digit_index,
            });
        }

        if digit_count > max_digits {
            let mut count = 0;
            for (index, &byte) in bytes.iter().enumerate().skip(start) {
                if byte != b'_' {
                    count += 1;
                    if count > max_digits {
                        return Err(Error::TooManyDigits {
                            limit: max_digits,
                            index,
                        });
                    }
                }
            }
        }

        let mut result = Self::zero();
        for &byte in &bytes[start..] {
            if byte == b'_' {
                continue;
            }
            result.mul_small(radix as u8);
            result.add_small(Self::digit_value(byte).unwrap());
        }
        result.negative = negative && !result.is_zero();
        Ok(result)
    }

    pub fn format(
        &self,
        radix: u32,
        min_digits: usize,
        group: usize,
    ) -> Result<String, Error> {
        Self::valid_radix(radix)?;
        let radix = radix as u8;
        let mut magnitude = self.digits.clone();
        let mut chars = Vec::new();
        while !(magnitude.len() == 1 && magnitude[0] == 0) {
            let rem = Self::div_small_vec(&mut magnitude, radix);
            chars.push(Self::digit_char(rem));
        }
        if chars.is_empty() {
            chars.push('0');
        }
        chars.reverse();
        while chars.len() < min_digits {
            chars.insert(0, '0');
        }

        let mut output = String::new();
        if self.negative && !self.is_zero() {
            output.push('-');
        }
        for (i, ch) in chars.iter().enumerate() {
            if group > 0 && i > 0 && (chars.len() - i) % group == 0 {
                output.push('_');
            }
            output.push(*ch);
        }
        Ok(output)
    }

    fn digit_char(digit: u8) -> char {
        if digit < 10 {
            (b'0' + digit) as char
        } else {
            (b'a' + digit - 10) as char
        }
    }

    fn is_zero(&self) -> bool {
        self.digits.len() == 1 && self.digits[0] == 0
    }

    fn abs_cmp(&self, rhs: &Self) -> Ordering {
        match self.digits.len().cmp(&rhs.digits.len()) {
            Ordering::Equal => {
                for (a, b) in self.digits.iter().rev().zip(rhs.digits.iter().rev()) {
                    match a.cmp(b) {
                        Ordering::Equal => {}
                        other => return other,
                    }
                }
                Ordering::Equal
            }
            other => other,
        }
    }

    fn add_abs(a: &[u8], b: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(a.len().max(b.len()) + 1);
        let mut carry = 0u16;
        for i in 0..a.len().max(b.len()) {
            let sum = a.get(i).copied().unwrap_or(0) as u16
                + b.get(i).copied().unwrap_or(0) as u16
                + carry;
            out.push((sum % 10) as u8);
            carry = sum / 10;
        }
        if carry > 0 {
            out.push(carry as u8);
        }
        out
    }

    fn sub_abs(a: &[u8], b: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(a.len());
        let mut borrow = 0i16;
        for i in 0..a.len() {
            let mut value = a[i] as i16 - b.get(i).copied().unwrap_or(0) as i16 - borrow;
            if value < 0 {
                value += 10;
                borrow = 1;
            } else {
                borrow = 0;
            }
            out.push(value as u8);
        }
        let mut result = Self {
            negative: false,
            digits: out,
        };
        result.normalize();
        result.digits
    }

    fn mul_small(&mut self, factor: u8) {
        if factor == 0 || self.is_zero() {
            self.digits = vec![0];
            self.negative = false;
            return;
        }
        let mut carry = 0u16;
        for digit in &mut self.digits {
            let value = *digit as u16 * factor as u16 + carry;
            *digit = (value % 10) as u8;
            carry = value / 10;
        }
        while carry > 0 {
            self.digits.push((carry % 10) as u8);
            carry /= 10;
        }
    }

    fn add_small(&mut self, value: u8) {
        let mut carry = value as u16;
        let mut i = 0;
        while carry > 0 {
            if i == self.digits.len() {
                self.digits.push(0);
            }
            let sum = self.digits[i] as u16 + carry;
            self.digits[i] = (sum % 10) as u8;
            carry = sum / 10;
            i += 1;
        }
    }

    fn div_small_vec(value: &mut Vec<u8>, divisor: u8) -> u8 {
        let mut remainder = 0u16;
        for digit in value.iter_mut().rev() {
            let current = remainder * 10 + *digit as u16;
            *digit = (current / divisor as u16) as u8;
            remainder = current % divisor as u16;
        }
        while value.len() > 1 && value.last() == Some(&0) {
            value.pop();
        }
        remainder as u8
    }

    fn abs(&self) -> Self {
        let mut result = self.clone();
        result.negative = false;
        result
    }

    fn from_abs_digits(digits: Vec<u8>, negative: bool) -> Self {
        let mut result = Self { negative, digits };
        result.normalize();
        result
    }

    fn mul_abs(a: &[u8], b: &[u8]) -> Vec<u8> {
        let mut out = vec![0u8; a.len() + b.len()];
        for (i, &x) in a.iter().enumerate() {
            let mut carry = 0u16;
            for (j, &y) in b.iter().enumerate() {
                let index = i + j;
                let value = out[index] as u16 + x as u16 * y as u16 + carry;
                out[index] = (value % 10) as u8;
                carry = value / 10;
            }
            let mut index = i + b.len();
            while carry > 0 {
                if index == out.len() {
                    out.push(0);
                }
                let value = out[index] as u16 + carry;
                out[index] = (value % 10) as u8;
                carry = value / 10;
                index += 1;
            }
        }
        while out.len() > 1 && out.last() == Some(&0) {
            out.pop();
        }
        out
    }

    fn mul_abs_small(digits: &[u8], factor: u8) -> Vec<u8> {
        let mut out = Vec::with_capacity(digits.len() + 1);
        let mut carry = 0u16;
        for &digit in digits {
            let value = digit as u16 * factor as u16 + carry;
            out.push((value % 10) as u8);
            carry = value / 10;
        }
        while carry > 0 {
            out.push((carry % 10) as u8);
            carry /= 10;
        }
        out
    }

    fn div_rem_abs(dividend: &Self, divisor: &Self) -> (Self, Self) {
        let mut quotient_be = Vec::with_capacity(dividend.digits.len());
        let mut remainder = Self::zero();
        for &digit in dividend.digits.iter().rev() {
            remainder.mul_small(10);
            remainder.add_small(digit);
            let mut qdigit = 0u8;
            for candidate in (1..=9).rev() {
                let product = Self::from_abs_digits(
                    Self::mul_abs_small(&divisor.digits, candidate),
                    false,
                );
                if product.abs_cmp(&remainder) != Ordering::Greater {
                    qdigit = candidate;
                    remainder = Self::from_abs_digits(
                        Self::sub_abs(&remainder.digits, &product.digits),
                        false,
                    );
                    break;
                }
            }
            quotient_be.push(qdigit);
        }
        quotient_be.reverse();
        let mut quotient = Self::from_abs_digits(quotient_be, false);
        quotient.normalize();
        (quotient, remainder)
    }

    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.negative, rhs.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => self.abs_cmp(rhs),
            (true, true) => self.abs_cmp(rhs).reverse(),
        }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            Self::from_abs_digits(
                Self::add_abs(&self.digits, &rhs.digits),
                self.negative,
            )
        } else {
            match self.abs_cmp(rhs) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => Self::from_abs_digits(
                    Self::sub_abs(&self.digits, &rhs.digits),
                    self.negative,
                ),
                Ordering::Less => Self::from_abs_digits(
                    Self::sub_abs(&rhs.digits, &self.digits),
                    rhs.negative,
                ),
            }
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
        Self::from_abs_digits(
            Self::mul_abs(&self.digits, &rhs.digits),
            self.negative ^ rhs.negative,
        )
    }

    pub fn div_rem(&self, rhs: &Self) -> Result<(Self, Self), Error> {
        if rhs.is_zero() {
            return Err(Error::DivisionByZero);
        }
        let (mut quotient, mut remainder) = Self::div_rem_abs(&self.abs(), &rhs.abs());
        quotient.negative = (self.negative ^ rhs.negative) && !quotient.is_zero();
        remainder.negative = self.negative && !remainder.is_zero();
        Ok((quotient, remainder))
    }

    pub fn pow(&self, exponent: u32) -> Result<Self, Error> {
        const LIMIT: u32 = 1024;
        if exponent > LIMIT {
            return Err(Error::ExponentTooLarge { exponent, limit: LIMIT });
        }
        let mut power = exponent;
        let mut base = self.clone();
        let mut result = Self::one();
        while power > 0 {
            if power & 1 == 1 {
                result = result.mul(&base);
            }
            power >>= 1;
            if power > 0 {
                base = base.mul(&base);
            }
        }
        Ok(result)
    }

    pub fn gcd(&self, rhs: &Self) -> Self {
        let mut a = self.abs();
        let mut b = rhs.abs();
        while !b.is_zero() {
            let (_, remainder) = a.div_rem(&b).expect("nonzero divisor");
            a = b;
            b = remainder.abs();
        }
        a.negative = false;
        a
    }
}