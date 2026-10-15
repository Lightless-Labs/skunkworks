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

#[derive(Debug, Clone)]
struct Expression {
    start: usize,
    end: usize,
    variables: Vec<(String, usize)>,
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
                let close = match bytes[cursor + 1..].iter().position(|&b| b == b'}') {
                    Some(offset) => cursor + 1 + offset,
                    None => return Err(Error::Unclosed { pos: start }),
                };

                if let Some(offset) = bytes[cursor + 1..close]
                    .iter()
                    .position(|&b| b == b'{')
                {
                    return Err(Error::NestedOpen {
                        pos: cursor + 1 + offset,
                    });
                }

                let content_start = cursor + 1;
                let mut spec_start = content_start;

                if content_start < close
                    && matches!(bytes[content_start], b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&')
                {
                    return Err(Error::UnsupportedOperator { pos: content_start });
                }

                if content_start == close {
                    return Err(Error::EmptyExpression { pos: close });
                }

                let mut variables = Vec::new();
                let mut seen = Vec::<String>::new();

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
                    let first = name_bytes[0];
                    if !first.is_ascii_alphabetic() && first != b'_' {
                        return Err(Error::InvalidName { pos: spec_start });
                    }

                    for (i, &byte) in name_bytes.iter().enumerate().skip(1) {
                        if !byte.is_ascii_alphanumeric() && byte != b'_' {
                            return Err(Error::InvalidName { pos: spec_start + i });
                        }
                    }

                    let name = template[spec_start..spec_end].to_string();
                    if seen.iter().any(|existing| existing == &name) {
                        return Err(Error::DuplicateVariable { pos: spec_start });
                    }
                    seen.push(name.clone());
                    variables.push((name, spec_start));

                    if spec_end == close {
                        break;
                    }
                    spec_start = spec_end + 1;
                }

                expressions.push(Expression {
                    start,
                    end: close + 1,
                    variables,
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
        for (name, _) in expression.variables {
            if let Some((_, value)) = vars.iter().rev().find(|(binding, _)| *binding == name) {
                fields.push(encode_value(value));
            }
        }
        output.push_str(&fields.join(","));
        literal_start = expression.end;
    }

    output.push_str(&template[literal_start..]);
    Ok(output)
}

fn encode_value(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = String::new();
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];

        if byte == b'%' && i + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                output.push('%');
                output.push(hex_digit(high));
                output.push(hex_digit(low));
                i += 3;
                continue;
            }
        }

        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            output.push(byte as char);
        } else {
            output.push('%');
            output.push(hex_digit(byte >> 4));
            output.push(hex_digit(byte & 0x0f));
        }
        i += 1;
    }

    output
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'A' + value - 10) as char,
    }
}

fn main() {}