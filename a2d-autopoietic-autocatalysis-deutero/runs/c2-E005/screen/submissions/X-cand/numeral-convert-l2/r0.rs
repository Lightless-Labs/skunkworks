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
    if input.is_empty() {
        return Err(BaseError::Empty { operand });
    }

    let bytes = input.as_bytes();
    let payload_start = if let Some(hash_index) = bytes.iter().position(|&b| b == b'#') {
        let expected = radix.to_string();
        if &input[..hash_index] != expected {
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

    for (offset, &byte) in payload.iter().enumerate() {
        if byte == b'_' {
            continue;
        }
        match digit_value(byte) {
            Some(value) if value < radix => {}
            _ => {
                return Err(BaseError::InvalidDigit {
                    operand,
                    index: payload_start + offset,
                });
            }
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
        .filter(|&&byte| byte != b'_')
        .map(|&byte| digit_value(byte).unwrap())
        .collect();

    if digits.len() >= 2 && digits[0] == 0 {
        return Err(BaseError::LeadingZero {
            operand,
            index: payload_start,
        });
    }

    Ok(digits)
}

fn multiply_add(number: &mut Vec<u8>, multiplier: u8, addend: u8) {
    let mut carry = addend as u16;
    for digit in number.iter_mut() {
        let value = (*digit as u16) * (multiplier as u16) + carry;
        *digit = (value % 256) as u8;
        carry = value / 256;
    }
    while carry != 0 {
        number.push((carry % 256) as u8);
        carry /= 256;
    }
}

fn add_numbers(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(left.len().max(right.len()) + 1);
    let mut carry = 0u16;
    for index in 0..left.len().max(right.len()) {
        let sum = left.get(index).copied().unwrap_or(0) as u16
            + right.get(index).copied().unwrap_or(0) as u16
            + carry;
        result.push((sum % 256) as u8);
        carry = sum / 256;
    }
    if carry != 0 {
        result.push(carry as u8);
    }
    result
}

fn convert_from_radix(digits: &[u8], radix: u8) -> Vec<u8> {
    let mut value = vec![0u8];
    for &digit in digits {
        multiply_add(&mut value, radix, digit);
    }
    while value.len() > 1 && value.last() == Some(&0) {
        value.pop();
    }
    value
}

fn render(mut value: Vec<u8>, radix: u8, style: BaseStyle) -> String {
    let mut digits = Vec::new();
    while !(value.len() == 1 && value[0] == 0) {
        let mut remainder = 0u16;
        for byte in value.iter_mut().rev() {
            let current = remainder * 256 + *byte as u16;
            *byte = (current / radix as u16) as u8;
            remainder = current % radix as u16;
        }
        digits.push(remainder as u8);
        while value.len() > 1 && value.last() == Some(&0) {
            value.pop();
        }
    }
    if digits.is_empty() {
        digits.push(0);
    }

    while digits.len() < style.min_width {
        digits.push(0);
    }
    digits.reverse();

    let mut output = String::new();
    for (index, digit) in digits.iter().copied().enumerate() {
        if style.group != 0
            && index != 0
            && (digits.len() - index) % style.group == 0
        {
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
    let value = convert_from_radix(&digits, from);
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
    let left_value = convert_from_radix(&left_digits, base);
    let right_value = convert_from_radix(&right_digits, base);
    Ok(render(add_numbers(&left_value, &right_value), base, style))
}