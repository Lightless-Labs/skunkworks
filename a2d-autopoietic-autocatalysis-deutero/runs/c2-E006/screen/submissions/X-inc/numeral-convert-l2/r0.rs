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

fn validate_style(style: &BaseStyle) -> Result<(), BaseError> {
    if style.group <= 8 && style.min_width <= 256 {
        Ok(())
    } else {
        Err(BaseError::BadStyle)
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
    let payload_start = if let Some(hash) = bytes.iter().position(|&b| b == b'#') {
        let expected = radix.to_string();
        if &input[..hash] != expected {
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

    // Invalid digits take precedence over all separator and leading-zero errors.
    for (i, &byte) in payload.iter().enumerate() {
        if byte == b'_' {
            continue;
        }
        if !matches!(digit_value(byte), Some(value) if value < radix) {
            return Err(BaseError::InvalidDigit {
                operand,
                index: payload_start + i,
            });
        }
    }

    let mut digits = Vec::new();
    for (i, &byte) in payload.iter().enumerate() {
        if byte == b'_' {
            let has_digit_before = i > 0 && payload[i - 1] != b'_';
            let has_digit_after = i + 1 < payload.len() && payload[i + 1] != b'_';
            if !has_digit_before || !has_digit_after {
                return Err(BaseError::Separator {
                    operand,
                    index: payload_start + i,
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

fn multiply_add(number: &mut Vec<u8>, radix: u8, digit: u8) {
    let mut carry = digit as u16;
    for value in number.iter_mut() {
        let product = (*value as u16) * (radix as u16) + carry;
        *value = (product & 0xff) as u8;
        carry = product >> 8;
    }
    while carry != 0 {
        number.push((carry & 0xff) as u8);
        carry >>= 8;
    }
}

fn add_numbers(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(left.len().max(right.len()) + 1);
    let mut carry = 0u16;
    for i in 0..left.len().max(right.len()) {
        let sum = left.get(i).copied().unwrap_or(0) as u16
            + right.get(i).copied().unwrap_or(0) as u16
            + carry;
        result.push((sum & 0xff) as u8);
        carry = sum >> 8;
    }
    if carry != 0 {
        result.push(carry as u8);
    }
    result
}

fn render(mut number: Vec<u8>, radix: u8, style: BaseStyle) -> String {
    while number.last() == Some(&0) {
        number.pop();
    }

    let mut digits = Vec::new();
    if number.is_empty() {
        digits.push(0);
    } else {
        while !number.is_empty() {
            let mut remainder = 0u16;
            for byte in number.iter_mut().rev() {
                let current = (remainder << 8) | (*byte as u16);
                *byte = (current / radix as u16) as u8;
                remainder = current % radix as u16;
            }
            digits.push(remainder as u8);
            while number.last() == Some(&0) {
                number.pop();
            }
        }
        digits.reverse();
    }

    let padding = style.min_width.saturating_sub(digits.len());
    let total_digits = padding + digits.len();
    let mut output = String::with_capacity(total_digits + total_digits / style.group.max(1));
    let alphabet = if style.upper {
        b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    } else {
        b"0123456789abcdefghijklmnopqrstuvwxyz"
    };

    for i in 0..total_digits {
        if style.group != 0 && i > 0 && (total_digits - i) % style.group == 0 {
            output.push('_');
        }
        let value = if i < padding { 0 } else { digits[i - padding] };
        output.push(alphabet[value as usize] as char);
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

    Ok(render(add_numbers(&left_value, &right_value), base, style))
}