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

fn validate_base(base: u8, which: usize) -> Result<(), BaseError> {
    if !(2..=36).contains(&base) {
        Err(BaseError::BadBase { which, value: base })
    } else {
        Ok(())
    }
}

fn validate_style(style: &BaseStyle) -> Result<(), BaseError> {
    if style.group > 8 || style.min_width > 256 {
        Err(BaseError::BadStyle)
    } else {
        Ok(())
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

fn parse_operand(input: &str, base: u8, operand: usize) -> Result<Vec<u8>, BaseError> {
    if input.is_empty() {
        return Err(BaseError::Empty { operand });
    }

    let bytes = input.as_bytes();
    let payload_start = if let Some(hash_index) = bytes.iter().position(|&byte| byte == b'#') {
        let expected = base.to_string();
        if &bytes[..hash_index] != expected.as_bytes() {
            return Err(BaseError::InvalidTag { operand, index: 0 });
        }
        hash_index + 1
    } else {
        0
    };

    if payload_start == bytes.len() {
        return Err(BaseError::Empty { operand });
    }

    let payload = &bytes[payload_start..];

    // Invalid digits have precedence over separator and leading-zero errors.
    for (offset, &byte) in payload.iter().enumerate() {
        if byte == b'_' {
            continue;
        }
        match digit_value(byte) {
            Some(value) if value < base => {}
            _ => {
                return Err(BaseError::InvalidDigit {
                    operand,
                    index: payload_start + offset,
                });
            }
        }
    }

    let mut digits = Vec::new();
    for (offset, &byte) in payload.iter().enumerate() {
        if byte == b'_' {
            let valid_left = offset > 0 && payload[offset - 1] != b'_';
            let valid_right = offset + 1 < payload.len() && payload[offset + 1] != b'_';
            if !valid_left || !valid_right {
                return Err(BaseError::Separator {
                    operand,
                    index: payload_start + offset,
                });
            }
        } else if let Some(value) = digit_value(byte) {
            digits.push(value);
        }
    }

    if digits.len() >= 2 && digits[0] == 0 {
        return Err(BaseError::LeadingZero {
            operand,
            index: payload_start,
        });
    }

    Ok(digits)
}

fn multiply_add(number: &mut Vec<u8>, base: u8, value: u8) {
    let mut carry = value as u32;
    for digit in number.iter_mut() {
        let current = (*digit as u32) * (base as u32) + carry;
        *digit = (current % 1_000_000_000) as u8;
        carry = current / 1_000_000_000;
    }

    // This helper uses decimal limbs, so the caller uses the dedicated
    // arbitrary-precision routines below instead.
    let _ = carry;
}

#[derive(Clone)]
struct BigNat {
    limbs: Vec<u32>,
}

impl BigNat {
    const LIMB_BASE: u64 = 1_000_000_000;

    fn zero() -> Self {
        Self { limbs: vec![0] }
    }

    fn from_digits(digits: &[u8], base: u8) -> Self {
        let mut value = Self::zero();
        for &digit in digits {
            value.mul_small(base as u32);
            value.add_small(digit as u32);
        }
        value
    }

    fn mul_small(&mut self, factor: u32) {
        let mut carry = 0u64;
        for limb in &mut self.limbs {
            let product = (*limb as u64) * factor as u64 + carry;
            *limb = (product % Self::LIMB_BASE) as u32;
            carry = product / Self::LIMB_BASE;
        }
        while carry != 0 {
            self.limbs.push((carry % Self::LIMB_BASE) as u32);
            carry /= Self::LIMB_BASE;
        }
    }

    fn add_small(&mut self, addend: u32) {
        let mut carry = addend as u64;
        let mut index = 0;
        while carry != 0 {
            if index == self.limbs.len() {
                self.limbs.push(0);
            }
            let sum = self.limbs[index] as u64 + carry;
            self.limbs[index] = (sum % Self::LIMB_BASE) as u32;
            carry = sum / Self::LIMB_BASE;
            index += 1;
        }
    }

    fn add_assign(&mut self, other: &Self) {
        let length = self.limbs.len().max(other.limbs.len());
        self.limbs.resize(length, 0);
        let mut carry = 0u64;
        for index in 0..length {
            let right = other.limbs.get(index).copied().unwrap_or(0) as u64;
            let sum = self.limbs[index] as u64 + right + carry;
            self.limbs[index] = (sum % Self::LIMB_BASE) as u32;
            carry = sum / Self::LIMB_BASE;
        }
        if carry != 0 {
            self.limbs.push(carry as u32);
        }
    }

    fn div_small(&mut self, divisor: u32) -> u32 {
        let mut remainder = 0u64;
        for limb in self.limbs.iter_mut().rev() {
            let current = remainder * Self::LIMB_BASE + *limb as u64;
            *limb = (current / divisor as u64) as u32;
            remainder = current % divisor as u64;
        }
        while self.limbs.len() > 1 && self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }
        remainder as u32
    }

    fn to_radix_digits(&self, radix: u8) -> Vec<u8> {
        let mut value = self.clone();
        if value.limbs.len() == 1 && value.limbs[0] == 0 {
            return vec![0];
        }
        let mut digits = Vec::new();
        while !(value.limbs.len() == 1 && value.limbs[0] == 0) {
            digits.push(value.div_small(radix as u32) as u8);
        }
        digits.reverse();
        digits
    }
}

fn render(value: &BigNat, radix: u8, style: BaseStyle) -> String {
    let digits = value.to_radix_digits(radix);
    let length = digits.len().max(style.min_width);
    let padding = length - digits.len();
    let separator_count = if style.group == 0 {
        0
    } else {
        (length - 1) / style.group
    };

    let mut output = String::with_capacity(length + separator_count);
    for index in 0..length {
        if style.group != 0 && index > 0 && (length - index) % style.group == 0 {
            output.push('_');
        }

        let value = if index < padding {
            0
        } else {
            digits[index - padding]
        };
        let byte = match value {
            0..=9 => b'0' + value,
            _ if style.upper => b'A' + (value - 10),
            _ => b'a' + (value - 10),
        };
        output.push(byte as char);
    }
    output
}

pub fn convert_base(
    input: &str,
    from: u8,
    to: u8,
    style: BaseStyle,
) -> Result<String, BaseError> {
    validate_base(from, 0)?;
    validate_base(to, 1)?;
    validate_style(&style)?;
    let digits = parse_operand(input, from, 0)?;
    let value = BigNat::from_digits(&digits, from);
    Ok(render(&value, to, style))
}

pub fn add_base(
    left: &str,
    right: &str,
    base: u8,
    style: BaseStyle,
) -> Result<String, BaseError> {
    validate_base(base, 0)?;
    validate_style(&style)?;
    let left_digits = parse_operand(left, base, 0)?;
    let right_digits = parse_operand(right, base, 1)?;
    let mut sum = BigNat::from_digits(&left_digits, base);
    sum.add_assign(&BigNat::from_digits(&right_digits, base));
    Ok(render(&sum, base, style))
}