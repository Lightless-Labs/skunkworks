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
    if (2..=36).contains(&value) {
        Ok(())
    } else {
        Err(BaseError::BadBase { which, value })
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
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Err(BaseError::Empty { operand });
    }

    let payload_start = if let Some(tag_end) = bytes.iter().position(|&b| b == b'#') {
        if bytes[..tag_end] != *base.to_string().as_bytes() {
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
    let mut digits = Vec::with_capacity(payload.len());

    for (offset, &byte) in payload.iter().enumerate() {
        if byte == b'_' {
            continue;
        }
        match digit_value(byte) {
            Some(value) if value < base => digits.push(value),
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

    if digits.len() >= 2 && digits[0] == 0 {
        return Err(BaseError::LeadingZero {
            operand,
            index: payload_start,
        });
    }

    Ok(digits)
}

// Digits are stored little-endian in the supplied radix.
fn add_digits(left: &[u8], right: &[u8], base: u8) -> Vec<u8> {
    let mut result = Vec::with_capacity(left.len().max(right.len()) + 1);
    let mut carry = 0u16;
    for index in 0..left.len().max(right.len()) {
        let sum = left.get(index).copied().unwrap_or(0) as u16
            + right.get(index).copied().unwrap_or(0) as u16
            + carry;
        result.push((sum % base as u16) as u8);
        carry = sum / base as u16;
    }
    if carry != 0 {
        result.push(carry as u8);
    }
    if result.is_empty() {
        result.push(0);
    }
    result
}

fn multiply_add(digits: &mut Vec<u8>, multiplier: u8, addend: u8, base: u8) {
    let mut carry = addend as u16;
    for digit in digits.iter_mut() {
        let value = *digit as u16 * multiplier as u16 + carry;
        *digit = (value % base as u16) as u8;
        carry = value / base as u16;
    }
    while carry != 0 {
        digits.push((carry % base as u16) as u8);
        carry /= base as u16;
    }
}

fn convert_digits(input: &[u8], from: u8, to: u8) -> Vec<u8> {
    // Build the value directly in the destination radix.
    let mut value = vec![0];
    for &digit in input {
        multiply_add(&mut value, from, digit, to);
    }
    value
}

fn render(mut digits: Vec<u8>, style: BaseStyle) -> String {
    while digits.len() > 1 && digits.last() == Some(&0) {
        digits.pop();
    }
    while digits.len() < style.min_width {
        digits.push(0);
    }

    let digit_count = digits.len();
    let separator_count = if style.group == 0 {
        0
    } else {
        (digit_count - 1) / style.group
    };
    let mut output = String::with_capacity(digit_count + separator_count);

    for index in (0..digit_count).rev() {
        if index != digit_count - 1
            && style.group != 0
            && (digit_count - 1 - index) % style.group == 0
        {
            output.push('_');
        }
        let digit = digits[index];
        let byte = if digit < 10 {
            b'0' + digit
        } else if style.upper {
            b'A' + (digit - 10)
        } else {
            b'a' + (digit - 10)
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

    let digits = parse_operand(input, from, 0)?;
    Ok(render(convert_digits(&digits, from, to), style))
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

    let left_digits = parse_operand(left, base, 0)?;
    let right_digits = parse_operand(right, base, 1)?;

    // Convert each value to base 36, add there, then convert the sum back.
    let left_value = convert_digits(&left_digits, base, 36);
    let right_value = convert_digits(&right_digits, base, 36);
    let sum = add_digits(&left_value, &right_value, 36);
    let result = convert_digits(&sum, 36, base);

    Ok(render(result, style))
}