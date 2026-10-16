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
    if input.is_empty() {
        return Err(BaseError::Empty { operand });
    }

    let bytes = input.as_bytes();
    let (start, payload) = if let Some(hash) = bytes.iter().position(|&b| b == b'#') {
        let expected = radix.to_string();
        if &bytes[..hash] != expected.as_bytes() {
            return Err(BaseError::InvalidTag {
                operand,
                index: 0,
            });
        }
        (hash + 1, &bytes[hash + 1..])
    } else {
        (0, bytes)
    };

    if payload.is_empty() {
        return Err(BaseError::Empty { operand });
    }

    let mut digits = Vec::new();

    // Digit validation takes precedence over separator validation.
    for (offset, &byte) in payload.iter().enumerate() {
        if byte == b'_' {
            continue;
        }
        match digit_value(byte) {
            Some(value) if value < radix => digits.push(value),
            _ => {
                return Err(BaseError::InvalidDigit {
                    operand,
                    index: start + offset,
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
                index: start + offset,
            });
        }
    }

    if digits.len() >= 2 && digits[0] == 0 {
        return Err(BaseError::LeadingZero {
            operand,
            index: start,
        });
    }

    Ok(digits)
}

fn mul_add_small(value: &mut Vec<u8>, radix: u8, digit: u8) {
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

fn parse_value(digits: &[u8], radix: u8) -> Vec<u8> {
    let mut value = Vec::new();
    for &digit in digits {
        mul_add_small(&mut value, radix, digit);
    }
    value
}

fn div_small(value: &mut Vec<u8>, divisor: u8) -> u8 {
    let mut remainder = 0u16;
    for limb in value.iter_mut().rev() {
        let n = (remainder << 8) | (*limb as u16);
        *limb = (n / divisor as u16) as u8;
        remainder = n % divisor as u16;
    }
    while value.last() == Some(&0) {
        value.pop();
    }
    remainder as u8
}

fn render(mut value: Vec<u8>, radix: u8, style: BaseStyle) -> String {
    let mut digits = Vec::new();

    if value.is_empty() {
        digits.push(0);
    } else {
        while !value.is_empty() {
            digits.push(div_small(&mut value, radix));
        }
        digits.reverse();
    }

    let padding = style.min_width.saturating_sub(digits.len());
    if padding != 0 {
        let mut padded = vec![0; padding];
        padded.extend(digits);
        digits = padded;
    }

    let mut output = String::new();
    for (i, digit) in digits.iter().copied().enumerate() {
        if style.group != 0 && i != 0 && (digits.len() - i) % style.group == 0 {
            output.push('_');
        }
        let ch = match digit {
            0..=9 => (b'0' + digit) as char,
            _ if style.upper => (b'A' + digit - 10) as char,
            _ => (b'a' + digit - 10) as char,
        };
        output.push(ch);
    }
    output
}

pub fn convert_base(
    input: &str,
    from: u8,
    to: u8,
    style: BaseStyle,
) -> Result<String, BaseError> {
    validate_radix(from, 0)?;
    validate_radix(to, 1)?;
    if style.group > 8 || style.min_width > 256 {
        return Err(BaseError::BadStyle);
    }

    let digits = parse_operand(input, from, 0)?;
    let value = parse_value(&digits, from);
    Ok(render(value, to, style))
}

pub fn add_base(
    left: &str,
    right: &str,
    base: u8,
    style: BaseStyle,
) -> Result<String, BaseError> {
    validate_radix(base, 0)?;
    if style.group > 8 || style.min_width > 256 {
        return Err(BaseError::BadStyle);
    }

    let left_digits = parse_operand(left, base, 0)?;
    let right_digits = parse_operand(right, base, 1)?;
    let mut a = parse_value(&left_digits, base);
    let b = parse_value(&right_digits, base);

    if a.len() < b.len() {
        a.resize(b.len(), 0);
    }
    let mut carry = 0u16;
    for (i, slot) in a.iter_mut().enumerate() {
        let sum = *slot as u16 + b.get(i).copied().unwrap_or(0) as u16 + carry;
        *slot = (sum & 0xff) as u8;
        carry = sum >> 8;
    }
    if carry != 0 {
        a.push(carry as u8);
    }

    Ok(render(a, base, style))
}