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
    if (2..=36).contains(&base) {
        Ok(())
    } else {
        Err(BaseError::BadBase { which, value: base })
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

fn validate_operand(input: &str, base: u8, operand: usize) -> Result<Vec<u8>, BaseError> {
    if input.is_empty() {
        return Err(BaseError::Empty { operand });
    }

    let bytes = input.as_bytes();
    let payload_start = if let Some(hash) = bytes.iter().position(|&byte| byte == b'#') {
        if &bytes[..hash] != base.to_string().as_bytes() {
            return Err(BaseError::InvalidTag { operand, index: 0 });
        }
        hash + 1
    } else {
        0
    };

    if payload_start == bytes.len() {
        return Err(BaseError::Empty { operand });
    }

    let payload = &bytes[payload_start..];

    for (offset, &byte) in payload.iter().enumerate() {
        if byte != b'_' && !matches!(digit_value(byte), Some(value) if value < base) {
            return Err(BaseError::InvalidDigit {
                operand,
                index: payload_start + offset,
            });
        }
    }

    for (offset, &byte) in payload.iter().enumerate() {
        if byte == b'_'
            && (offset == 0
                || offset + 1 == payload.len()
                || digit_value(payload[offset - 1]).is_none()
                || digit_value(payload[offset + 1]).is_none())
        {
            return Err(BaseError::Separator {
                operand,
                index: payload_start + offset,
            });
        }
    }

    let digits: Vec<u8> = payload
        .iter()
        .filter_map(|&byte| if byte == b'_' { None } else { digit_value(byte) })
        .collect();

    if digits.len() >= 2 && digits[0] == 0 {
        return Err(BaseError::LeadingZero {
            operand,
            index: payload_start,
        });
    }

    Ok(digits)
}

fn parse_big(digits: &[u8], base: u8) -> Vec<u32> {
    const LIMB_BASE: u64 = 1_000_000_000;
    let mut limbs = vec![0u32];

    for &digit in digits {
        let mut carry = digit as u64;
        for limb in &mut limbs {
            let value = (*limb as u64) * base as u64 + carry;
            *limb = (value % LIMB_BASE) as u32;
            carry = value / LIMB_BASE;
        }
        if carry != 0 {
            limbs.push(carry as u32);
        }
    }

    limbs
}

fn divide_big(limbs: &mut Vec<u32>, divisor: u8) -> u8 {
    let mut remainder = 0u64;

    for limb in limbs.iter_mut().rev() {
        let value = remainder * 1_000_000_000 + *limb as u64;
        *limb = (value / divisor as u64) as u32;
        remainder = value % divisor as u64;
    }

    while limbs.len() > 1 && limbs.last() == Some(&0) {
        limbs.pop();
    }

    remainder as u8
}

fn render(limbs: Vec<u32>, base: u8, style: BaseStyle) -> String {
    let mut value = limbs;
    let mut digits = Vec::new();

    if value.iter().all(|&limb| limb == 0) {
        digits.push(0);
    } else {
        while value.iter().any(|&limb| limb != 0) {
            digits.push(divide_big(&mut value, base));
        }
        digits.reverse();
    }

    let padding = style.min_width.saturating_sub(digits.len());
    let total_digits = digits.len() + padding;
    let mut output = String::with_capacity(total_digits + total_digits / 2);

    for index in 0..total_digits {
        if style.group != 0 && index > 0 && (total_digits - index) % style.group == 0 {
            output.push('_');
        }

        let digit = if index < padding {
            0
        } else {
            digits[index - padding]
        };

        let byte = match digit {
            0..=9 => b'0' + digit,
            _ if style.upper => b'A' + (digit - 10),
            _ => b'a' + (digit - 10),
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

    if style.group > 8 || style.min_width > 256 {
        return Err(BaseError::BadStyle);
    }

    let digits = validate_operand(input, from, 0)?;
    Ok(render(parse_big(&digits, from), to, style))
}

pub fn add_base(
    left: &str,
    right: &str,
    base: u8,
    style: BaseStyle,
) -> Result<String, BaseError> {
    validate_base(base, 0)?;

    if style.group > 8 || style.min_width > 256 {
        return Err(BaseError::BadStyle);
    }

    let left_digits = validate_operand(left, base, 0)?;
    let right_digits = validate_operand(right, base, 1)?;
    let mut sum = parse_big(&left_digits, base);
    let other = parse_big(&right_digits, base);

    let mut carry = 0u64;
    let length = sum.len().max(other.len());
    sum.resize(length, 0);

    for (index, limb) in sum.iter_mut().enumerate() {
        let rhs = other.get(index).copied().unwrap_or(0) as u64;
        let value = *limb as u64 + rhs + carry;
        *limb = (value % 1_000_000_000) as u32;
        carry = value / 1_000_000_000;
    }

    if carry != 0 {
        sum.push(carry as u32);
    }

    Ok(render(sum, base, style))
}