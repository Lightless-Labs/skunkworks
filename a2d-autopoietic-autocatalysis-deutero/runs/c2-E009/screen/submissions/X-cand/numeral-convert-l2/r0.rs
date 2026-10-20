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

fn digit_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'Z' => Some(byte - b'A' + 10),
        b'a'..=b'z' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn validate_operand(input: &str, radix: u8, operand: usize) -> Result<Vec<u8>, BaseError> {
    if input.is_empty() {
        return Err(BaseError::Empty { operand });
    }

    let bytes = input.as_bytes();
    let payload_start = if let Some(hash_index) = bytes.iter().position(|&byte| byte == b'#') {
        let expected = radix.to_string();
        if &bytes[..hash_index] != expected.as_bytes() {
            return Err(BaseError::InvalidTag {
                operand,
                index: 0,
            });
        }
        hash_index + 1
    } else {
        0
    };

    if payload_start == bytes.len() {
        return Err(BaseError::Empty { operand });
    }

    let payload = &bytes[payload_start..];

    // Check all digit bytes before reporting any separator error.
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

    let first_digit = payload.iter().find(|&&byte| byte != b'_').copied();
    let digit_count = payload.iter().filter(|&&byte| byte != b'_').count();
    if digit_count >= 2 && first_digit == Some(b'0') {
        return Err(BaseError::LeadingZero {
            operand,
            index: payload_start,
        });
    }

    Ok(payload
        .iter()
        .filter_map(|&byte| {
            if byte == b'_' {
                None
            } else {
                digit_value(byte)
            }
        })
        .collect())
}

fn multiply_add(number: &mut Vec<u8>, multiplier: u8, addend: u8) {
    let mut carry = addend as u16;
    for digit in number.iter_mut() {
        let value = (*digit as u16) * (multiplier as u16) + carry;
        *digit = (value % 256) as u8;
        carry = value / 256;
    }
    while carry > 0 {
        number.push((carry % 256) as u8);
        carry /= 256;
    }
}

fn convert_digits_to_base(digits: &[u8], from: u8, to: u8) -> Vec<u8> {
    let mut value = Vec::<u8>::new(); // little-endian bytes, base 256
    for &digit in digits {
        multiply_add(&mut value, from, digit);
    }

    if value.is_empty() {
        return vec![0];
    }

    let mut output = Vec::new();
    while value.iter().any(|&byte| byte != 0) {
        let mut remainder = 0u32;
        for byte in value.iter_mut().rev() {
            let current = (remainder << 8) | (*byte as u32);
            *byte = (current / to as u32) as u8;
            remainder = current % to as u32;
        }
        output.push(remainder as u8);
        while value.last() == Some(&0) {
            value.pop();
        }
    }
    output.reverse();
    output
}

fn add_values(left: &[u8], right: &[u8], radix: u8) -> Vec<u8> {
    let mut result = Vec::new();
    let mut carry = 0u16;
    let count = left.len().max(right.len());
    for index in 0..count {
        let l = left
            .len()
            .checked_sub(index + 1)
            .map(|i| left[i])
            .unwrap_or(0) as u16;
        let r = right
            .len()
            .checked_sub(index + 1)
            .map(|i| right[i])
            .unwrap_or(0) as u16;
        let sum = l + r + carry;
        result.push((sum % radix as u16) as u8);
        carry = sum / radix as u16;
    }
    if carry > 0 {
        result.push(carry as u8);
    }
    if result.is_empty() {
        result.push(0);
    }
    result.reverse();
    result
}

fn format_digits(mut digits: Vec<u8>, style: BaseStyle) -> String {
    if digits.is_empty() {
        digits.push(0);
    }
    if digits.len() < style.min_width {
        let padding = style.min_width - digits.len();
        let mut padded = Vec::new();
        padded.extend(std::iter::repeat(0).take(padding));
        padded.extend(digits);
        digits = padded;
    }

    let mut output = String::new();
    for (index, &digit) in digits.iter().enumerate() {
        if style.group != 0 && index > 0 && (digits.len() - index) % style.group == 0 {
            output.push('_');
        }
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
    let input_digits = validate_operand(input, from, 0)?;
    let output_digits = convert_digits_to_base(&input_digits, from, to);
    Ok(format_digits(output_digits, style))
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
    let sum = add_values(&left_digits, &right_digits, base);
    Ok(format_digits(sum, style))
}