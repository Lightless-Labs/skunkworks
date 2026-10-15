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

fn parse_operand(input: &str, radix: u8, operand: usize) -> Result<Vec<u8>, BaseError> {
    if input.is_empty() {
        return Err(BaseError::Empty { operand });
    }

    let bytes = input.as_bytes();
    let payload_start = if let Some(hash) = bytes.iter().position(|&b| b == b'#') {
        let expected = radix.to_string();
        if &bytes[..hash] != expected.as_bytes() {
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

    if digits.is_empty() {
        return Err(BaseError::Separator {
            operand,
            index: payload_start,
        });
    }

    Ok(digits)
}

fn multiply_add(value: &mut Vec<u8>, radix: u8, digit: u8) {
    let mut carry = digit as u32;
    for limb in value.iter_mut() {
        let next = (*limb as u32) * radix as u32 + carry;
        *limb = (next & 0xff) as u8;
        carry = next >> 8;
    }
    while carry != 0 {
        value.push((carry & 0xff) as u8);
        carry >>= 8;
    }
}

fn add_values(left: &[u8], right: &[u8]) -> Vec<u8> {
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

fn divide_value(value: &mut Vec<u8>, divisor: u8) -> u8 {
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
        digits.push(b'0');
    } else {
        while !work.is_empty() {
            let digit = divide_value(&mut work, radix);
            let byte = if digit < 10 {
                b'0' + digit
            } else if style.upper {
                b'A' + digit - 10
            } else {
                b'a' + digit - 10
            };
            digits.push(byte);
        }
        digits.reverse();
    }

    if digits.len() < style.min_width {
        let mut padded = vec![b'0'; style.min_width - digits.len()];
        padded.extend(digits);
        digits = padded;
    }

    if style.group == 0 {
        return String::from_utf8(digits).unwrap();
    }

    let mut output = Vec::with_capacity(digits.len() + digits.len() / style.group);
    for (i, byte) in digits.iter().copied().enumerate() {
        if i != 0 && (digits.len() - i) % style.group == 0 {
            output.push(b'_');
        }
        output.push(byte);
    }
    String::from_utf8(output).unwrap()
}

fn parse_to_value(digits: &[u8], radix: u8) -> Vec<u8> {
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
    let value = parse_to_value(&digits, from);
    Ok(render(&value, to, style))
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
    let left_value = parse_to_value(&left_digits, base);
    let right_value = parse_to_value(&right_digits, base);
    let sum = add_values(&left_value, &right_value);
    Ok(render(&sum, base, style))
}

fn main() {}