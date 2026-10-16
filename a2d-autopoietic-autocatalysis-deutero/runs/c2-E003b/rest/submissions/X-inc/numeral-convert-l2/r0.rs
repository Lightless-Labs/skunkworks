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

    let mut payload_start = 0;
    if let Some(tag_end) = bytes.iter().position(|&byte| byte == b'#') {
        let expected = radix.to_string();
        if &bytes[..tag_end] != expected.as_bytes() {
            return Err(BaseError::InvalidTag { operand, index: 0 });
        }
        payload_start = tag_end + 1;
        if payload_start == bytes.len() {
            return Err(BaseError::Empty { operand });
        }
    }

    let payload = &bytes[payload_start..];

    // Invalid digits take precedence over separator errors.
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

    let mut digits = Vec::new();
    for (offset, &byte) in payload.iter().enumerate() {
        if byte == b'_' {
            if offset == 0
                || offset + 1 == payload.len()
                || payload[offset - 1] == b'_'
                || payload[offset + 1] == b'_'
            {
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

fn add_small(number: &mut Vec<u8>, value: u8, radix: u8) {
    let mut carry = value as u16;
    for digit in number.iter_mut().rev() {
        let sum = *digit as u16 + carry;
        *digit = (sum % radix as u16) as u8;
        carry = sum / radix as u16;
        if carry == 0 {
            return;
        }
    }
    while carry != 0 {
        number.insert(0, (carry % radix as u16) as u8);
        carry /= radix as u16;
    }
}

fn multiply_small(number: &mut Vec<u8>, multiplier: u8, radix: u8) {
    let mut carry = 0u16;
    for digit in number.iter_mut().rev() {
        let product = *digit as u16 * multiplier as u16 + carry;
        *digit = (product % radix as u16) as u8;
        carry = product / radix as u16;
    }
    while carry != 0 {
        number.insert(0, (carry % radix as u16) as u8);
        carry /= radix as u16;
    }
}

fn convert_digits(digits: &[u8], from: u8, to: u8) -> Vec<u8> {
    let mut result = vec![0];
    for &digit in digits {
        multiply_small(&mut result, from, to);
        add_small(&mut result, digit, to);
    }
    while result.len() > 1 && result[0] == 0 {
        result.remove(0);
    }
    result
}

fn add_numbers(left: &[u8], right: &[u8], radix: u8) -> Vec<u8> {
    let mut result = Vec::with_capacity(left.len().max(right.len()) + 1);
    let mut carry = 0u16;
    let mut li = left.len();
    let mut ri = right.len();

    while li > 0 || ri > 0 || carry > 0 {
        let a = if li > 0 {
            li -= 1;
            left[li] as u16
        } else {
            0
        };
        let b = if ri > 0 {
            ri -= 1;
            right[ri] as u16
        } else {
            0
        };
        let sum = a + b + carry;
        result.push((sum % radix as u16) as u8);
        carry = sum / radix as u16;
    }

    result.reverse();
    result
}

fn format_result(digits: &[u8], style: BaseStyle) -> String {
    let pad = style.min_width.saturating_sub(digits.len());
    let total_len = pad + digits.len();
    let mut output = String::with_capacity(total_len + total_len / style.group.max(1));
    let alphabet = if style.upper {
        b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    } else {
        b"0123456789abcdefghijklmnopqrstuvwxyz"
    };

    for position in 0..total_len {
        if style.group != 0 && position != 0 && (total_len - position) % style.group == 0 {
            output.push('_');
        }

        let digit = if position < pad {
            0
        } else {
            digits[position - pad]
        };
        output.push(alphabet[digit as usize] as char);
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
    let converted = convert_digits(&digits, from, to);
    Ok(format_result(&converted, style))
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
    let sum = add_numbers(&left_digits, &right_digits, base);
    Ok(format_result(&sum, style))
}