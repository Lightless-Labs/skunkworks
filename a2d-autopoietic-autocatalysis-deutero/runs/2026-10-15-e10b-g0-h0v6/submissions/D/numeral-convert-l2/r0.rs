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

fn validate_operand(input: &str, radix: u8, operand: usize) -> Result<Vec<u8>, BaseError> {
    if input.is_empty() {
        return Err(BaseError::Empty { operand });
    }

    let bytes = input.as_bytes();
    let payload_start = if let Some(hash_index) = bytes.iter().position(|&b| b == b'#') {
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

fn add_small(number: &mut Vec<u8>, value: u8, radix: u8) {
    let mut carry = value as u16;
    let radix = radix as u16;
    for digit in number.iter_mut().rev() {
        let sum = *digit as u16 + carry;
        *digit = (sum % radix) as u8;
        carry = sum / radix;
        if carry == 0 {
            return;
        }
    }
    while carry != 0 {
        number.insert(0, (carry % radix) as u8);
        carry /= radix;
    }
}

fn multiply_small(number: &mut Vec<u8>, factor: u8, radix: u8) {
    let radix = radix as u16;
    let mut carry = 0u16;
    for digit in number.iter_mut().rev() {
        let product = *digit as u16 * factor as u16 + carry;
        *digit = (product % radix) as u8;
        carry = product / radix;
    }
    while carry != 0 {
        number.insert(0, (carry % radix) as u8);
        carry /= radix;
    }
}

fn parse_value(digits: &[u8], radix: u8) -> Vec<u8> {
    let mut value = vec![0];
    for &digit in digits {
        multiply_small(&mut value, radix, 10);
        add_small(&mut value, digit, 10);
    }
    value
}

fn add_values(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(left.len().max(right.len()) + 1);
    let mut i = left.len();
    let mut j = right.len();
    let mut carry = 0u8;

    while i > 0 || j > 0 || carry > 0 {
        let a = if i > 0 {
            i -= 1;
            left[i]
        } else {
            0
        };
        let b = if j > 0 {
            j -= 1;
            right[j]
        } else {
            0
        };
        let sum = a + b + carry;
        result.push(sum % 10);
        carry = sum / 10;
    }

    if result.is_empty() {
        result.push(0);
    }
    result.reverse();
    result
}

fn divide_small(number: &mut Vec<u8>, divisor: u8) -> u8 {
    let mut remainder = 0u16;
    let divisor = divisor as u16;
    for digit in number.iter_mut() {
        let current = remainder * 10 + *digit as u16;
        *digit = (current / divisor) as u8;
        remainder = current % divisor;
    }
    while number.len() > 1 && number[0] == 0 {
        number.remove(0);
    }
    remainder as u8
}

fn render(value: &[u8], radix: u8, style: BaseStyle) -> String {
    let mut decimal = value.to_vec();
    let mut digits = Vec::new();

    if decimal.iter().all(|&d| d == 0) {
        digits.push(0);
    } else {
        while decimal.iter().any(|&d| d != 0) {
            digits.push(divide_small(&mut decimal, radix));
        }
        digits.reverse();
    }

    let zero = if style.upper { b'0' } else { b'0' };
    let mut chars: Vec<u8> = digits
        .into_iter()
        .map(|digit| {
            if digit < 10 {
                zero + digit
            } else if style.upper {
                b'A' + digit - 10
            } else {
                b'a' + digit - 10
            }
        })
        .collect();

    if chars.len() < style.min_width {
        let mut padded = vec![b'0'; style.min_width - chars.len()];
        padded.extend(chars);
        chars = padded;
    }

    if style.group == 0 || chars.len() <= style.group {
        return String::from_utf8(chars).expect("ASCII output");
    }

    let first_group = chars.len() % style.group;
    let first_group = if first_group == 0 {
        style.group
    } else {
        first_group
    };
    let mut output = Vec::with_capacity(chars.len() + chars.len() / style.group);
    for (index, byte) in chars.into_iter().enumerate() {
        if index > 0 && (index == first_group || (index > first_group && (index - first_group) % style.group == 0)) {
            output.push(b'_');
        }
        output.push(byte);
    }
    String::from_utf8(output).expect("ASCII output")
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
    let digits = validate_operand(input, from, 0)?;
    let decimal = parse_value(&digits, from);
    Ok(render(&decimal, to, style))
}

pub fn add_base(
    left: &str,
    right: &str,
    base: u8,
    style: BaseStyle,
) -> Result<String, BaseError> {
    validate_base(base, 0)?;
    validate_style(&style)?;
    let left_digits = validate_operand(left, base, 0)?;
    let right_digits = validate_operand(right, base, 1)?;
    let left_value = parse_value(&left_digits, base);
    let right_value = parse_value(&right_digits, base);
    let sum = add_values(&left_value, &right_value);
    Ok(render(&sum, base, style))
}