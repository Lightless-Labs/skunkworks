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

fn validate_radix(value: u8, which: usize) -> Result<(), BaseError> {
    if !(2..=36).contains(&value) {
        Err(BaseError::BadBase { which, value })
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
    let payload_start = if let Some(tag_end) = bytes.iter().position(|&b| b == b'#') {
        let expected = radix.to_string();
        if &input[..tag_end] != expected {
            return Err(BaseError::InvalidTag { operand, index: 0 });
        }
        tag_end + 1
    } else {
        0
    };

    if payload_start == bytes.len() {
        return Err(BaseError::Empty { operand });
    }

    let payload = &bytes[payload_start..];
    let mut digits = Vec::new();

    for (offset, &byte) in payload.iter().enumerate() {
        if byte == b'_' {
            continue;
        }
        match digit_value(byte) {
            Some(value) if value < radix => digits.push(value),
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
            let has_digit_before = offset > 0 && payload[offset - 1] != b'_';
            let has_digit_after = offset + 1 < payload.len() && payload[offset + 1] != b'_';
            if !has_digit_before || !has_digit_after {
                return Err(BaseError::Separator {
                    operand,
                    index: payload_start + offset,
                });
            }
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

fn multiply_add(value: &mut Vec<u8>, radix: u8, digit: u8) {
    let mut carry = digit as u16;
    for limb in value.iter_mut() {
        let total = (*limb as u16) * radix as u16 + carry;
        *limb = (total & 0xff) as u8;
        carry = total >> 8;
    }
    while carry > 0 {
        value.push((carry & 0xff) as u8);
        carry >>= 8;
    }
}

fn parse_to_binary(digits: &[u8], radix: u8) -> Vec<u8> {
    let mut value = Vec::new();
    for &digit in digits {
        multiply_add(&mut value, radix, digit);
    }
    value
}

fn add_binary(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(left.len().max(right.len()) + 1);
    let mut carry = 0u16;
    for i in 0..left.len().max(right.len()) {
        let a = left.get(i).copied().unwrap_or(0) as u16;
        let b = right.get(i).copied().unwrap_or(0) as u16;
        let sum = a + b + carry;
        result.push((sum & 0xff) as u8);
        carry = sum >> 8;
    }
    if carry != 0 {
        result.push(carry as u8);
    }
    result
}

fn divide_small(value: &mut Vec<u8>, divisor: u8) -> u8 {
    let mut remainder = 0u16;
    for limb in value.iter_mut().rev() {
        let current = (remainder << 8) | *limb as u16;
        *limb = (current / divisor as u16) as u8;
        remainder = current % divisor as u16;
    }
    while value.last() == Some(&0) {
        value.pop();
    }
    remainder as u8
}

fn render(value: &[u8], radix: u8, style: BaseStyle) -> String {
    let mut work = value.to_vec();
    let mut digits = Vec::new();

    if work.is_empty() {
        digits.push('0');
    } else {
        while !work.is_empty() {
            let remainder = divide_small(&mut work, radix);
            let character = match remainder {
                0..=9 => (b'0' + remainder) as char,
                _ if style.upper => (b'A' + remainder - 10) as char,
                _ => (b'a' + remainder - 10) as char,
            };
            digits.push(character);
        }
        digits.reverse();
    }

    if digits.len() < style.min_width {
        let mut padded = vec!['0'; style.min_width - digits.len()];
        padded.extend(digits);
        digits = padded;
    }

    if style.group == 0 {
        return digits.into_iter().collect();
    }

    let mut output = String::with_capacity(digits.len() + digits.len() / style.group);
    for (index, character) in digits.iter().enumerate() {
        if index > 0 && (digits.len() - index) % style.group == 0 {
            output.push('_');
        }
        output.push(*character);
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
    validate_radix(from, 0)?;
    validate_radix(to, 1)?;
    validate_style(&style)?;

    let digits = parse_operand(input, from, 0)?;
    let value = parse_to_binary(&digits, from);
    Ok(render(&value, to, style))
}

pub fn add_base(
    left: &str,
    right: &str,
    base: u8,
    style: BaseStyle,
) -> Result<String, BaseError> {
    validate_radix(base, 0)?;
    validate_style(&style)?;

    let left_digits = parse_operand(left, base, 0)?;
    let right_digits = parse_operand(right, base, 1)?;
    let left_value = parse_to_binary(&left_digits, base);
    let right_value = parse_to_binary(&right_digits, base);
    let sum = add_binary(&left_value, &right_value);
    Ok(render(&sum, base, style))
}

fn main() {}