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

fn digit_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'Z' => Some(byte - b'A' + 10),
        b'a'..=b'z' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn validate_and_parse(input: &str, radix: u8, operand: usize) -> Result<Vec<u8>, BaseError> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Err(BaseError::Empty { operand });
    }

    let mut payload_start = 0;
    if let Some(hash) = bytes.iter().position(|&b| b == b'#') {
        if bytes[..hash] != *radix.to_string().as_bytes() {
            return Err(BaseError::InvalidTag { operand, index: 0 });
        }
        payload_start = hash + 1;
        if payload_start == bytes.len() {
            return Err(BaseError::Empty { operand });
        }
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

fn mul_add_small(number: &mut Vec<u8>, multiplier: u8, addend: u8) {
    let mut carry = addend as u16;
    for digit in number.iter_mut() {
        let value = (*digit as u16) * (multiplier as u16) + carry;
        *digit = (value % 10) as u8;
        carry = value / 10;
    }
    while carry != 0 {
        number.push((carry % 10) as u8);
        carry /= 10;
    }
}

fn convert_digits(digits: &[u8], from: u8, to: u8) -> Vec<u8> {
    let mut decimal = vec![0u8];
    for &digit in digits {
        mul_add_small(&mut decimal, from, digit);
    }

    let mut result = Vec::new();
    while decimal.len() > 1 || decimal[0] != 0 {
        let mut remainder = 0u16;
        for digit in decimal.iter_mut().rev() {
            let value = remainder * 10 + *digit as u16;
            *digit = (value / to as u16) as u8;
            remainder = value % to as u16;
        }
        result.push(remainder as u8);
        while decimal.len() > 1 && decimal.last() == Some(&0) {
            decimal.pop();
        }
    }

    if result.is_empty() {
        result.push(0);
    }
    result.reverse();
    result
}

fn add_digits(left: &[u8], right: &[u8], base: u8) -> Vec<u8> {
    let mut result = Vec::new();
    let mut carry = 0u16;
    let mut left_index = left.len();
    let mut right_index = right.len();

    while left_index > 0 || right_index > 0 || carry > 0 {
        let left_digit = if left_index > 0 {
            left_index -= 1;
            left[left_index] as u16
        } else {
            0
        };
        let right_digit = if right_index > 0 {
            right_index -= 1;
            right[right_index] as u16
        } else {
            0
        };

        let sum = left_digit + right_digit + carry;
        result.push((sum % base as u16) as u8);
        carry = sum / base as u16;
    }

    if result.is_empty() {
        result.push(0);
    }
    result.reverse();
    result
}

fn render(mut digits: Vec<u8>, style: BaseStyle) -> String {
    if digits.len() < style.min_width {
        let mut padded = vec![0; style.min_width - digits.len()];
        padded.extend(digits);
        digits = padded;
    }

    let mut output = String::new();
    for (index, &digit) in digits.iter().enumerate() {
        if style.group != 0 && index != 0 && (digits.len() - index) % style.group == 0 {
            output.push('_');
        }
        let ch = if digit < 10 {
            b'0' + digit
        } else if style.upper {
            b'A' + digit - 10
        } else {
            b'a' + digit - 10
        };
        output.push(ch as char);
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
    if style.group > 8 || style.min_width > 256 {
        return Err(BaseError::BadStyle);
    }

    let digits = validate_and_parse(input, from, 0)?;
    Ok(render(convert_digits(&digits, from, to), style))
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
    if style.group > 8 || style.min_width > 256 {
        return Err(BaseError::BadStyle);
    }

    let left_digits = validate_and_parse(left, base, 0)?;
    let right_digits = validate_and_parse(right, base, 1)?;
    Ok(render(add_digits(&left_digits, &right_digits, base), style))
}

fn main() {}