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

    // Validate all non-separator bytes before checking separator placement.
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
            let left_is_digit = offset > 0 && payload[offset - 1] != b'_';
            let right_is_digit = offset + 1 < payload.len() && payload[offset + 1] != b'_';
            if !left_is_digit || !right_is_digit {
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
        let n = (*limb as u16) * (radix as u16) + carry;
        *limb = (n & 0xff) as u8;
        carry = n >> 8;
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
        let a = left.get(i).copied().unwrap_or(0) as u16;
        let b = right.get(i).copied().unwrap_or(0) as u16;
        let n = a + b + carry;
        result.push((n & 0xff) as u8);
        carry = n >> 8;
    }
    if carry != 0 {
        result.push(carry as u8);
    }
    result
}

fn render(mut value: Vec<u8>, radix: u8, style: BaseStyle) -> String {
    while value.len() > 1 && value.last() == Some(&0) {
        value.pop();
    }

    let mut digits = Vec::new();
    if value.iter().all(|&b| b == 0) {
        digits.push(0);
    } else {
        while value.iter().any(|&b| b != 0) {
            let mut remainder = 0u16;
            for limb in value.iter_mut().rev() {
                let current = (remainder << 8) | (*limb as u16);
                *limb = (current / radix as u16) as u8;
                remainder = current % radix as u16;
            }
            digits.push(remainder as u8);
            while value.len() > 1 && value.last() == Some(&0) {
                value.pop();
            }
        }
    }

    while digits.len() < style.min_width {
        digits.push(0);
    }
    digits.reverse();

    let mut out = String::with_capacity(digits.len() + digits.len() / style.group.max(1));
    for (i, digit) in digits.iter().enumerate() {
        if style.group != 0 && i != 0 && (digits.len() - i) % style.group == 0 {
            out.push('_');
        }
        let ch = match *digit {
            0..=9 => (b'0' + *digit) as char,
            n if style.upper => (b'A' + n - 10) as char,
            n => (b'a' + n - 10) as char,
        };
        out.push(ch);
    }
    out
}

fn parse_to_integer(digits: &[u8], radix: u8) -> Vec<u8> {
    let mut value = vec![0];
    for &digit in digits {
        multiply_add(&mut value, radix, digit);
    }
    value
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
    validate_base(from, 0)?;
    validate_base(to, 1)?;
    validate_style(&style)?;
    let digits = parse_operand(input, from, 0)?;
    let value = parse_to_integer(&digits, from);
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
    let left_value = parse_to_integer(&left_digits, base);
    let right_value = parse_to_integer(&right_digits, base);
    Ok(render(add_values(&left_value, &right_value), base, style))
}