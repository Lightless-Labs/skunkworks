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

#[derive(Debug)]
struct Expression {
    variables: Vec<String>,
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let expressions = validate(template)?;
    let mut output = String::new();
    let mut cursor = 0;

    for (start, end, expression) in expressions {
        output.push_str(&template[cursor..start]);

        let mut first = true;
        for name in &expression.variables {
            if let Some(value) = vars
                .iter()
                .rev()
                .find_map(|(binding_name, value)| (*binding_name == name).then_some(*value))
            {
                if !first {
                    output.push(',');
                }
                output.push_str(&encode(value));
                first = false;
            }
        }

        cursor = end + 1;
    }

    output.push_str(&template[cursor..]);
    Ok(output)
}

fn validate(template: &str) -> Result<Vec<(usize, usize, Expression)>, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                let start = i;
                let close = match bytes[i + 1..].iter().position(|&b| b == b'}') {
                    Some(offset) => i + 1 + offset,
                    None => return Err(Error::Unclosed { pos: start }),
                };

                if let Some(offset) = bytes[i + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: i + 1 + offset,
                    });
                }

                let mut content_start = i + 1;
                if content_start == close {
                    return Err(Error::EmptyExpression { pos: close });
                }

                if matches!(
                    bytes[content_start],
                    b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&'
                ) {
                    return Err(Error::UnsupportedOperator { pos: content_start });
                }

                let mut names = Vec::new();
                let mut seen = Vec::<String>::new();

                loop {
                    let entry_start = content_start;
                    let entry_end = bytes[entry_start..close]
                        .iter()
                        .position(|&b| b == b',')
                        .map_or(close, |offset| entry_start + offset);

                    if entry_start == entry_end {
                        return Err(Error::EmptyVariable { pos: entry_start });
                    }

                    let name_bytes = &bytes[entry_start..entry_end];
                    if !is_name_start(name_bytes[0]) {
                        return Err(Error::InvalidName { pos: entry_start });
                    }
                    if let Some(offset) = name_bytes[1..]
                        .iter()
                        .position(|&byte| !is_name_continue(byte))
                    {
                        return Err(Error::InvalidName {
                            pos: entry_start + 1 + offset,
                        });
                    }

                    let name = template[entry_start..entry_end].to_owned();
                    if seen.iter().any(|used| used == &name) {
                        return Err(Error::DuplicateVariable { pos: entry_start });
                    }
                    seen.push(name.clone());
                    names.push(name);

                    if entry_end == close {
                        break;
                    }
                    content_start = entry_end + 1;
                }

                expressions.push((
                    start,
                    close,
                    Expression { variables: names },
                ));
                i = close + 1;
            }
            _ => i += 1,
        }
    }

    Ok(expressions)
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn encode(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = value.as_bytes();
    let mut output = String::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];

        if byte == b'%' && i + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                output.push('%');
                output.push(HEX[high] as char);
                output.push(HEX[low] as char);
                i += 3;
                continue;
            }
        }

        if is_unreserved(byte) {
            output.push(byte as char);
        } else {
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
        i += 1;
    }

    output
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn hex_value(byte: u8) -> Option<usize> {
    match byte {
        b'0'..=b'9' => Some((byte - b'0') as usize),
        b'a'..=b'f' => Some((byte - b'a' + 10) as usize),
        b'A'..=b'F' => Some((byte - b'A' + 10) as usize),
        _ => None,
    }
}