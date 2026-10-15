#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    UnexpectedClose { pos: usize },
    Unclosed { pos: usize },
    NestedOpen { pos: usize },
    EmptyExpression { pos: usize },
    UnsupportedOperator { pos: usize },
    EmptyVariable { pos: usize },
    InvalidName { pos: usize },
    DuplicateVariable { pos: usize },
}

#[derive(Clone, Copy)]
enum Operator {
    Simple,
    Reserved,
    Fragment,
}

struct Expression {
    start: usize,
    end: usize,
    operator: Operator,
    names: Vec<(usize, usize)>,
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut cursor = 0;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'}' => return Err(Error::UnexpectedClose { pos: cursor }),
            b'{' => {
                let start = cursor;
                let close = match bytes[start + 1..].iter().position(|&b| b == b'}') {
                    Some(offset) => start + 1 + offset,
                    None => return Err(Error::Unclosed { pos: start }),
                };

                if let Some(offset) = bytes[start + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: start + 1 + offset,
                    });
                }

                let mut content_start = start + 1;
                let operator = match bytes.get(content_start).copied() {
                    Some(b'+') => {
                        content_start += 1;
                        Operator::Reserved
                    }
                    Some(b'#') => {
                        content_start += 1;
                        Operator::Fragment
                    }
                    Some(b'.' | b'/' | b';' | b'?' | b'&') => {
                        return Err(Error::UnsupportedOperator { pos: content_start });
                    }
                    _ => Operator::Simple,
                };

                if content_start == close {
                    return Err(Error::EmptyExpression { pos: close });
                }

                let mut names = Vec::new();
                let mut spec_start = content_start;
                while spec_start <= close {
                    let spec_end = bytes[spec_start..close]
                        .iter()
                        .position(|&b| b == b',')
                        .map(|offset| spec_start + offset)
                        .unwrap_or(close);

                    if spec_start == spec_end {
                        return Err(Error::EmptyVariable { pos: spec_start });
                    }

                    let name_bytes = &bytes[spec_start..spec_end];
                    if !is_name_start(name_bytes[0]) {
                        return Err(Error::InvalidName { pos: spec_start });
                    }
                    if let Some(offset) = name_bytes[1..]
                        .iter()
                        .position(|&b| !is_name_continue(b))
                    {
                        return Err(Error::InvalidName {
                            pos: spec_start + 1 + offset,
                        });
                    }

                    if names.iter().any(|&(a, b)| {
                        bytes[a..b] == bytes[spec_start..spec_end]
                    }) {
                        return Err(Error::DuplicateVariable { pos: spec_start });
                    }

                    names.push((spec_start, spec_end));
                    if spec_end == close {
                        break;
                    }
                    spec_start = spec_end + 1;
                }

                expressions.push(Expression {
                    start,
                    end: close + 1,
                    operator,
                    names,
                });
                cursor = close + 1;
            }
            _ => cursor += 1,
        }
    }

    let mut output = String::new();
    let mut literal_start = 0;

    for expression in expressions {
        output.push_str(&template[literal_start..expression.start]);

        let mut fields = Vec::new();
        for (name_start, name_end) in expression.names {
            let name = &template[name_start..name_end];
            if let Some((_, value)) = vars.iter().rev().find(|(key, _)| *key == name) {
                fields.push(*value);
            }
        }

        if !fields.is_empty() {
            if matches!(expression.operator, Operator::Fragment) {
                output.push('#');
            }

            for (index, value) in fields.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                encode_value(value, matches!(expression.operator, Operator::Reserved | Operator::Fragment), &mut output);
            }
        }

        literal_start = expression.end;
    }

    output.push_str(&template[literal_start..]);
    Ok(output)
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn is_reserved(byte: u8) -> bool {
    matches!(
        byte,
        b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&'
            | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn hex_digit(value: u8) -> u8 {
    if value < 10 {
        b'0' + value
    } else {
        b'A' + value - 10
    }
}

fn encode_value(value: &str, allow_reserved: bool, output: &mut String) {
    let bytes = value.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];

        if byte == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                output.push('%');
                output.push(hex_digit(high) as char);
                output.push(hex_digit(low) as char);
                index += 3;
                continue;
            }
        }

        if is_unreserved(byte) || (allow_reserved && is_reserved(byte)) {
            output.push(byte as char);
        } else {
            output.push('%');
            output.push(hex_digit(byte >> 4) as char);
            output.push(hex_digit(byte & 0x0f) as char);
        }
        index += 1;
    }
}

fn main() {}