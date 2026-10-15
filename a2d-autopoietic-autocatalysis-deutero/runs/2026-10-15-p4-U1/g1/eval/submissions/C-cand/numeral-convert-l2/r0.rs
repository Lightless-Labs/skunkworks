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

fn valid_base(base: u8) -> bool {
    (2..=36).contains(&base)
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
    let payload_start = if let Some(boundary) = bytes.iter().position(|&b| b == b'#') {
        let expected = radix.to_string();
        if &input[..boundary] != expected {
            return Err(BaseError::InvalidTag { operand, index: 0 });
        }
        boundary + 1
    } else {
        0
    };

    if payload_start == bytes.len() {
        return Err(BaseError::Empty { operand });
    }

    let mut digits = Vec::new();

    // Scan all non-separator bytes first, so invalid digits take precedence.
    for index in payload_start..bytes.len() {
        let byte = bytes[index];
        if byte == b'_' {
            continue;
        }
        match digit_value(byte) {
            Some(value) if value < radix => digits.push(value),
            _ => return Err(BaseError::InvalidDigit { operand, index }),
        }
    }

    // Validate separators against immediate payload bytes.
    for index in payload_start..bytes.len() {
        if bytes[index] == b'_'
            && (index == payload_start
                || index + 1 == bytes.len()
                || bytes[index - 1] == b'_'
                || bytes[index + 1] == b'_')
        {
            return Err(BaseError::Separator { operand, index });
        }
    }

    if digits.len() >= 2 && bytes[payload_start] == b'0' {
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

fn parse_magnitude(input: &str, radix: u8, operand: usize) -> Result<Vec<u8>, BaseError> {
    let digits = parse_operand(input, radix, operand)?;
    let mut value = vec![0u8];
    for digit in digits {
        multiply_add(&mut value, radix, digit);
    }
    while value.len() > 1 && value.last() == Some(&0) {
        value.pop();
    }
    Ok(value)
}

fn add_magnitudes(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(left.len().max(right.len()) + 1);
    let mut carry = 0u16;
    for index in 0..left.len().max(right.len()) {
        let sum = left.get(index).copied().unwrap_or(0) as u16
            + right.get(index).copied().unwrap_or(0) as u16
            + carry;
        result.push(sum as u8);
        carry = sum >> 8;
    }
    if carry != 0 {
        result.push(carry as u8);
    }
    result
}

fn divide_magnitude(value: &mut Vec<u8>, divisor: u8) -> u8 {
    let mut remainder = 0u16;
    for byte in value.iter_mut().rev() {
        let current = (remainder << 8) | (*byte as u16);
        *byte = (current / divisor as u16) as u8;
        remainder = current % divisor as u16;
    }
    while value.len() > 1 && value.last() == Some(&0) {
        value.pop();
    }
    remainder as u8
}

fn render(mut value: Vec<u8>, radix: u8, style: BaseStyle) -> String {
    let mut digits = Vec::new();
    if value.iter().all(|&byte| byte == 0) {
        digits.push(0u8);
    } else {
        while value.iter().any(|&byte| byte != 0) {
            digits.push(divide_magnitude(&mut value, radix));
        }
        digits.reverse();
    }

    let padding = style.min_width.saturating_sub(digits.len());
    let mut chars = Vec::with_capacity(padding + digits.len());
    chars.extend(std::iter::repeat_n('0', padding));
    for digit in digits {
        let byte = if digit < 10 {
            b'0' + digit
        } else if style.upper {
            b'A' + (digit - 10)
        } else {
            b'a' + (digit - 10)
        };
        chars.push(byte as char);
    }

    if style.group == 0 || chars.len() <= style.group {
        return chars.into_iter().collect();
    }

    let mut output = String::with_capacity(chars.len() + (chars.len() - 1) / style.group);
    let first_group = chars.len() % style.group;
    for (index, ch) in chars.into_iter().enumerate() {
        if index != 0
            && (index == first_group || (index > first_group && (index - first_group) % style.group == 0))
        {
            output.push('_');
        }
        output.push(ch);
    }
    output
}

pub fn convert_base(
    input: &str,
    from: u8,
    to: u8,
    style: BaseStyle,
) -> Result<String, BaseError> {
    if !valid_base(from) {
        return Err(BaseError::BadBase {
            which: 0,
            value: from,
        });
    }
    if !valid_base(to) {
        return Err(BaseError::BadBase {
            which: 1,
            value: to,
        });
    }
    validate_style(&style)?;
    let value = parse_magnitude(input, from, 0)?;
    Ok(render(value, to, style))
}

pub fn add_base(
    left: &str,
    right: &str,
    base: u8,
    style: BaseStyle,
) -> Result<String, BaseError> {
    if !valid_base(base) {
        return Err(BaseError::BadBase {
            which: 0,
            value: base,
        });
    }
    validate_style(&style)?;
    let left_value = parse_magnitude(left, base, 0)?;
    let right_value = parse_magnitude(right, base, 1)?;
    Ok(render(add_magnitudes(&left_value, &right_value), base, style))
}

fn main() {}