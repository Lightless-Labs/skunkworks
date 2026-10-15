#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseStyle {
    pub upper: bool,
    pub group: usize,
    pub min_width: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BaseError {
    BadBase { which: usize, value: u8 },
    BadStyle,
    Empty { operand: usize },
    InvalidTag { operand: usize, index: usize },
    InvalidDigit { operand: usize, index: usize },
    Separator { operand: usize, index: usize },
    LeadingZero { operand: usize, index: usize },
}

#[derive(Clone)]
struct BigNat {
    // Little-endian base-1,000,000,000 limbs.
    limbs: Vec<u32>,
}

impl BigNat {
    fn zero() -> Self {
        Self { limbs: vec![0] }
    }

    fn normalize(&mut self) {
        while self.limbs.len() > 1 && self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }
    }

    fn mul_add_small(&mut self, multiplier: u32, addend: u32) {
        const BASE: u64 = 1_000_000_000;
        let mut carry = addend as u64;

        for limb in &mut self.limbs {
            let value = (*limb as u64) * (multiplier as u64) + carry;
            *limb = (value % BASE) as u32;
            carry = value / BASE;
        }

        while carry != 0 {
            self.limbs.push((carry % BASE) as u32);
            carry /= BASE;
        }
    }

    fn add_assign(&mut self, other: &Self) {
        const BASE: u64 = 1_000_000_000;
        let max_len = self.limbs.len().max(other.limbs.len());
        self.limbs.resize(max_len, 0);
        let mut carry = 0u64;

        for i in 0..max_len {
            let a = self.limbs[i] as u64;
            let b = other.limbs.get(i).copied().unwrap_or(0) as u64;
            let sum = a + b + carry;
            self.limbs[i] = (sum % BASE) as u32;
            carry = sum / BASE;
        }

        if carry != 0 {
            self.limbs.push(carry as u32);
        }
    }

    fn to_radix_digits(&self, radix: u32) -> Vec<u8> {
        let mut value = self.clone();
        value.normalize();

        if value.limbs.len() == 1 && value.limbs[0] == 0 {
            return vec![0];
        }

        let mut reversed = Vec::new();
        while !(value.limbs.len() == 1 && value.limbs[0] == 0) {
            let mut remainder = 0u64;
            for limb in value.limbs.iter_mut().rev() {
                let current = remainder * 1_000_000_000 + (*limb as u64);
                *limb = (current / radix as u64) as u32;
                remainder = current % radix as u64;
            }
            value.normalize();
            reversed.push(remainder as u8);
        }

        reversed.reverse();
        reversed
    }
}

fn digit_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'Z' => Some(byte - b'A' + 10),
        b'a'..=b'z' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn parse_operand(input: &str, radix: u8, operand: usize) -> Result<BigNat, BaseError> {
    if input.is_empty() {
        return Err(BaseError::Empty { operand });
    }

    let bytes = input.as_bytes();
    let mut payload_start = 0usize;

    if let Some(hash_index) = bytes.iter().position(|&b| b == b'#') {
        let expected = radix.to_string();
        if &bytes[..hash_index] != expected.as_bytes() {
            return Err(BaseError::InvalidTag { operand, index: 0 });
        }
        payload_start = hash_index + 1;
    }

    if payload_start == bytes.len() {
        return Err(BaseError::Empty { operand });
    }

    let payload = &bytes[payload_start..];
    let mut digits = Vec::new();

    // Validate every non-separator byte before reporting separator errors.
    for (offset, &byte) in payload.iter().enumerate() {
        if byte == b'_' {
            continue;
        }
        match digit_value(byte) {
            Some(value) if value < radix => digits.push((offset, value)),
            _ => {
                return Err(BaseError::InvalidDigit {
                    operand,
                    index: payload_start + offset,
                });
            }
        }
    }

    for (offset, &byte) in payload.iter().enumerate() {
        if byte == b'_' {
            let left_is_digit = offset > 0 && payload[offset - 1] != b'_';
            let right_is_digit = offset + 1 < payload.len() && payload[offset + 1] != b'_';
            if !left_is_digit || !right_is_digit {
                return Err(BaseError::Separator {
                    operand,
                    index: payload_start + offset,
                });
            }
        }
    }

    if digits.len() >= 2 && digits[0].1 == 0 {
        return Err(BaseError::LeadingZero {
            operand,
            index: payload_start,
        });
    }

    let mut value = BigNat::zero();
    for (_, digit) in digits {
        value.mul_add_small(radix as u32, digit as u32);
    }
    Ok(value)
}

fn format_value(value: &BigNat, radix: u8, style: BaseStyle) -> String {
    let mut digits = value.to_radix_digits(radix as u32);
    if digits.len() < style.min_width {
        let mut padded = vec![0; style.min_width - digits.len()];
        padded.extend(digits);
        digits = padded;
    }

    let mut output = String::with_capacity(digits.len() + digits.len() / 2);
    for (i, digit) in digits.iter().copied().enumerate() {
        if style.group != 0 && i > 0 && (digits.len() - i) % style.group == 0 {
            output.push('_');
        }

        let ch = match digit {
            0..=9 => (b'0' + digit) as char,
            _ if style.upper => (b'A' + digit - 10) as char,
            _ => (b'a' + digit - 10) as char,
        };
        output.push(ch);
    }
    output
}

fn validate_style(style: &BaseStyle) -> Result<(), BaseError> {
    if style.group > 8 || style.min_width > 256 {
        Err(BaseError::BadStyle)
    } else {
        Ok(())
    }
}

pub fn convert_base(
    input: &str,
    from: u8,
    to: u8,
    style: BaseStyle,
) -> Result<String, BaseError> {
    if !(2..=36).contains(&from) {
        return Err(BaseError::BadBase {
            which: 0,
            value: from,
        });
    }
    if !(2..=36).contains(&to) {
        return Err(BaseError::BadBase {
            which: 1,
            value: to,
        });
    }
    validate_style(&style)?;

    let value = parse_operand(input, from, 0)?;
    Ok(format_value(&value, to, style))
}

pub fn add_base(
    left: &str,
    right: &str,
    base: u8,
    style: BaseStyle,
) -> Result<String, BaseError> {
    if !(2..=36).contains(&base) {
        return Err(BaseError::BadBase {
            which: 0,
            value: base,
        });
    }
    validate_style(&style)?;

    let mut left_value = parse_operand(left, base, 0)?;
    let right_value = parse_operand(right, base, 1)?;
    left_value.add_assign(&right_value);
    Ok(format_value(&left_value, base, style))
}

fn main() {}