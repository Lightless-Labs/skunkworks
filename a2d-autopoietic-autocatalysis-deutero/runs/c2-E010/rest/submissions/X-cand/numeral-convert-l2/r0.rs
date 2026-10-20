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

fn validate_base(base: u8, which: usize) -> Result<(), BaseError> {
    if !(2..=36).contains(&base) {
        Err(BaseError::BadBase { which, value: base })
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

fn parse_operand(input: &str, base: u8, operand: usize) -> Result<Vec<u8>, BaseError> {
    if input.is_empty() {
        return Err(BaseError::Empty { operand });
    }

    let bytes = input.as_bytes();
    let mut payload_start = 0;
    if let Some(hash) = bytes.iter().position(|&b| b == b'#') {
        let prefix = &input[..hash];
        if prefix != base.to_string() {
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

    if digits.is_empty() {
        return Err(BaseError::Separator {
            operand,
            index: payload_start,
        });
    }

    if digits.len() >= 2 && digits[0] == 0 {
        return Err(BaseError::LeadingZero {
            operand,
            index: payload_start,
        });
    }

    Ok(digits)
}

fn mul_add_small(number: &mut Vec<u32>, multiplier: u32, addend: u32) {
    let mut carry = addend as u64;
    for limb in number.iter_mut() {
        let value = (*limb as u64) * multiplier as u64 + carry;
        *limb = value as u32;
        carry = value >> 32;
    }
    while carry != 0 {
        number.push(carry as u32);
        carry >>= 32;
    }
}

fn from_digits(digits: &[u8], base: u8) -> Vec<u32> {
    let mut number = Vec::new();
    for &digit in digits {
        mul_add_small(&mut number, base as u32, digit as u32);
    }
    number
}

fn div_rem_small(number: &mut Vec<u32>, divisor: u32) -> u32 {
    let mut remainder = 0u64;
    for limb in number.iter_mut().rev() {
        let value = (remainder << 32) | *limb as u64;
        *limb = (value / divisor as u64) as u32;
        remainder = value % divisor as u64;
    }
    while number.last() == Some(&0) {
        number.pop();
    }
    remainder as u32
}

fn render(mut number: Vec<u32>, base: u8, style: &BaseStyle) -> String {
    let alphabet = if style.upper {
        b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    } else {
        b"0123456789abcdefghijklmnopqrstuvwxyz"
    };

    let mut reversed = Vec::new();
    if number.is_empty() {
        reversed.push(b'0');
    } else {
        while !number.is_empty() {
            reversed.push(alphabet[div_rem_small(&mut number, base as u32) as usize]);
        }
    }
    reversed.reverse();

    if reversed.len() < style.min_width {
        let mut padded = vec![b'0'; style.min_width - reversed.len()];
        padded.extend(reversed);
        reversed = padded;
    }

    let mut output = Vec::with_capacity(reversed.len() + reversed.len() / style.group.max(1));
    for (index, &byte) in reversed.iter().enumerate() {
        if style.group != 0 && index != 0 && (reversed.len() - index) % style.group == 0 {
            output.push(b'_');
        }
        output.push(byte);
    }
    String::from_utf8(output).unwrap()
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
    Ok(render(from_digits(&digits, from), to, &style))
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
    let mut sum = from_digits(&left_digits, base);
    let rhs = from_digits(&right_digits, base);

    let mut carry = 0u64;
    let length = sum.len().max(rhs.len());
    sum.resize(length, 0);
    for index in 0..length {
        let value = sum[index] as u64
            + rhs.get(index).copied().unwrap_or(0) as u64
            + carry;
        sum[index] = value as u32;
        carry = value >> 32;
    }
    if carry != 0 {
        sum.push(carry as u32);
    }

    Ok(render(sum, base, &style))
}
