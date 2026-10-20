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

fn parse_operand(input: &str, radix: u8, operand: usize) -> Result<Vec<u8>, BaseError> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Err(BaseError::Empty { operand });
    }

    let (payload_start, payload) = if let Some(hash) = bytes.iter().position(|&b| b == b'#') {
        let expected = radix.to_string();
        if &bytes[..hash] != expected.as_bytes() {
            return Err(BaseError::InvalidTag { operand, index: 0 });
        }
        (hash + 1, &bytes[hash + 1..])
    } else {
        (0, bytes)
    };

    if payload.is_empty() {
        return Err(BaseError::Empty { operand });
    }

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
            let has_digit_left = offset > 0 && payload[offset - 1] != b'_';
            let has_digit_right = offset + 1 < payload.len() && payload[offset + 1] != b'_';
            if !has_digit_left || !has_digit_right {
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
    let mut carry = u16::from(digit);
    for limb in value.iter_mut() {
        let n = u16::from(*limb) * u16::from(radix) + carry;
        *limb = (n % 256) as u8;
        carry = n / 256;
    }
    while carry != 0 {
        value.push((carry % 256) as u8);
        carry /= 256;
    }
}

fn add_values(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(left.len().max(right.len()) + 1);
    let mut carry = 0u16;
    for i in 0..left.len().max(right.len()) {
        let sum = u16::from(*left.get(i).unwrap_or(&0))
            + u16::from(*right.get(i).unwrap_or(&0))
            + carry;
        result.push((sum % 256) as u8);
        carry = sum / 256;
    }
    if carry != 0 {
        result.push(carry as u8);
    }
    result
}

fn render(mut value: Vec<u8>, radix: u8, style: BaseStyle) -> String {
    let mut digits = Vec::new();
    if value.iter().all(|&byte| byte == 0) {
        digits.push(0);
    } else {
        while value.iter().any(|&byte| byte != 0) {
            let mut remainder = 0u16;
            for byte in value.iter_mut().rev() {
                let n = remainder * 256 + u16::from(*byte);
                *byte = (n / u16::from(radix)) as u8;
                remainder = n % u16::from(radix);
            }
            digits.push(remainder as u8);
        }
        digits.reverse();
    }

    let padding = style.min_width.saturating_sub(digits.len());
    let mut output_digits = Vec::with_capacity(padding + digits.len());
    output_digits.resize(padding, 0);
    output_digits.extend(digits);

    let mut output = String::with_capacity(output_digits.len() + output_digits.len() / 2);
    for (i, digit) in output_digits.iter().enumerate() {
        if style.group != 0
            && i > 0
            && (output_digits.len() - i) % style.group == 0
        {
            output.push('_');
        }
        let byte = match *digit {
            0..=9 => b'0' + *digit,
            value if style.upper => b'A' + (value - 10),
            value => b'a' + (value - 10),
        };
        output.push(char::from(byte));
    }
    output
}

fn parse_to_binary(digits: &[u8], radix: u8) -> Vec<u8> {
    let mut value = Vec::new();
    for &digit in digits {
        multiply_add(&mut value, radix, digit);
    }
    value
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
    Ok(render(parse_to_binary(&digits, from), to, style))
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
    let left_value = parse_to_binary(&left_digits, base);
    let right_value = parse_to_binary(&right_digits, base);
    Ok(render(add_values(&left_value, &right_value), base, style))
}