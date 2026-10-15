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

    for (offset, &byte) in payload.iter().enumerate() {
        if byte == b'_' {
            continue;
        }
        if digit_value(byte).is_none_or(|value| value >= radix) {
            return Err(BaseError::InvalidDigit {
                operand,
                index: payload_start + offset,
            });
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

    let digits: Vec<u8> = payload
        .iter()
        .filter(|&&byte| byte != b'_')
        .map(|&byte| digit_value(byte).unwrap())
        .collect();

    if digits.len() >= 2 && digits[0] == 0 {
        return Err(BaseError::LeadingZero {
            operand,
            index: payload_start,
        });
    }

    Ok(digits)
}

fn mul_add_small(number: &mut Vec<u8>, radix: u8, digit: u8) {
    let mut carry = digit as u16;
    for limb in number.iter_mut() {
        let value = (*limb as u16) * (radix as u16) + carry;
        *limb = (value & 0xff) as u8;
        carry = value >> 8;
    }
    while carry != 0 {
        number.push((carry & 0xff) as u8);
        carry >>= 8;
    }
}

fn add_numbers(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(left.len().max(right.len()) + 1);
    let mut carry = 0u16;
    for index in 0..left.len().max(right.len()) {
        let value = left.get(index).copied().unwrap_or(0) as u16
            + right.get(index).copied().unwrap_or(0) as u16
            + carry;
        result.push((value & 0xff) as u8);
        carry = value >> 8;
    }
    if carry != 0 {
        result.push(carry as u8);
    }
    result
}

fn divide_small(number: &mut Vec<u8>, divisor: u8) -> u8 {
    let mut remainder = 0u16;
    for limb in number.iter_mut().rev() {
        let value = (remainder << 8) | *limb as u16;
        *limb = (value / divisor as u16) as u8;
        remainder = value % divisor as u16;
    }
    while number.last() == Some(&0) {
        number.pop();
    }
    remainder as u8
}

fn render(number: &[u8], radix: u8, style: BaseStyle) -> String {
    let mut digits = if number.is_empty() {
        vec![0]
    } else {
        let mut work = number.to_vec();
        let mut reversed = Vec::new();
        while !work.is_empty() {
            reversed.push(divide_small(&mut work, radix));
        }
        reversed.reverse();
        reversed
    };

    if digits.len() < style.min_width {
        let mut padded = vec![0; style.min_width - digits.len()];
        padded.extend(digits);
        digits = padded;
    }

    let mut output = String::with_capacity(digits.len() + digits.len() / style.group.max(1));
    for (index, digit) in digits.iter().copied().enumerate() {
        if style.group != 0 && index != 0 && (digits.len() - index) % style.group == 0 {
            output.push('_');
        }
        let byte = match digit {
            0..=9 => b'0' + digit,
            _ if style.upper => b'A' + (digit - 10),
            _ => b'a' + (digit - 10),
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

    let digits = parse_operand(input, from, 0)?;
    let mut number = Vec::new();
    for digit in digits {
        mul_add_small(&mut number, from, digit);
    }
    Ok(render(&number, to, style))
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

    let mut left_number = Vec::new();
    for digit in left_digits {
        mul_add_small(&mut left_number, base, digit);
    }
    let mut right_number = Vec::new();
    for digit in right_digits {
        mul_add_small(&mut right_number, base, digit);
    }

    Ok(render(&add_numbers(&left_number, &right_number), base, style))
}

fn main() {}