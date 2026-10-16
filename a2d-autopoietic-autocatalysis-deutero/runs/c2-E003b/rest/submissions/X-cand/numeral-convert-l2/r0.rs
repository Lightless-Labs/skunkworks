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

fn validate_base(value: u8, which: usize) -> Result<(), BaseError> {
    if !(2..=36).contains(&value) {
        Err(BaseError::BadBase { which, value })
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

fn parse_operand(input: &str, radix: u8, operand: usize) -> Result<Vec<u8>, BaseError> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Err(BaseError::Empty { operand });
    }

    let payload_start = if let Some(hash) = bytes.iter().position(|&b| b == b'#') {
        let expected = radix.to_string();
        if &bytes[..hash] != expected.as_bytes() {
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
        if byte == b'_' {
            continue;
        }
        if !matches!(digit_value(byte), Some(value) if value < radix) {
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
                || payload[offset - 1] == b'_'
                || payload[offset + 1] == b'_')
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

    if digits.is_empty() {
        let offset = payload.iter().position(|&byte| byte == b'_').unwrap_or(0);
        return Err(BaseError::Separator {
            operand,
            index: payload_start + offset,
        });
    }

    if digits.len() >= 2 && digits[0] == 0 {
        return Err(BaseError::LeadingZero {
            operand,
            index: payload_start,
        });
    }

    Ok(digits)
}

// Little-endian base-256 magnitude.
fn multiply_add(value: &mut Vec<u8>, multiplier: u8, addend: u8) {
    let mut carry = addend as u16;
    for byte in value.iter_mut() {
        let product = (*byte as u16) * (multiplier as u16) + carry;
        *byte = product as u8;
        carry = product >> 8;
    }
    while carry != 0 {
        value.push(carry as u8);
        carry >>= 8;
    }
}

fn add_magnitudes(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(left.len().max(right.len()) + 1);
    let mut carry = 0u16;
    for i in 0..left.len().max(right.len()) {
        let sum = left.get(i).copied().unwrap_or(0) as u16
            + right.get(i).copied().unwrap_or(0) as u16
            + carry;
        result.push(sum as u8);
        carry = sum >> 8;
    }
    if carry != 0 {
        result.push(carry as u8);
    }
    result
}

fn render(
    mut value: Vec<u8>,
    radix: u8,
    style: BaseStyle,
) -> String {
    let mut digits = Vec::new();

    if value.is_empty() {
        digits.push(0);
    } else {
        while value.iter().any(|&byte| byte != 0) {
            let mut remainder = 0u16;
            for byte in value.iter_mut().rev() {
                let current = (remainder << 8) | (*byte as u16);
                *byte = (current / radix as u16) as u8;
                remainder = current % radix as u16;
            }
            digits.push(remainder as u8);
            while value.last() == Some(&0) {
                value.pop();
            }
        }
    }

    while digits.len() < style.min_width {
        digits.push(0);
    }
    digits.reverse();

    let mut output = String::with_capacity(digits.len() + digits.len() / style.group.max(1));
    for (index, &digit) in digits.iter().enumerate() {
        if style.group != 0 && index != 0 && (digits.len() - index) % style.group == 0 {
            output.push('_');
        }
        let character = match digit {
            0..=9 => (b'0' + digit) as char,
            _ if style.upper => (b'A' + digit - 10) as char,
            _ => (b'a' + digit - 10) as char,
        };
        output.push(character);
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
    let mut value = Vec::new();
    for digit in digits {
        multiply_add(&mut value, from, digit);
    }
    Ok(render(value, to, style))
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

    let mut left_value = Vec::new();
    for digit in left_digits {
        multiply_add(&mut left_value, base, digit);
    }

    let mut right_value = Vec::new();
    for digit in right_digits {
        multiply_add(&mut right_value, base, digit);
    }

    Ok(render(add_magnitudes(&left_value, &right_value), base, style))
}