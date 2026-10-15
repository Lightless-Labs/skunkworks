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
struct Variable {
    name: String,
    pos: usize,
}

#[derive(Debug)]
struct Expression {
    variables: Vec<Variable>,
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut cursor = 0;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'}' => return Err(Error::UnexpectedClose { pos: cursor }),
            b'{' => {
                let open = cursor;
                let close = match bytes[open + 1..].iter().position(|&byte| byte == b'}') {
                    Some(offset) => open + 1 + offset,
                    None => return Err(Error::Unclosed { pos: open }),
                };

                if let Some(offset) = bytes[open + 1..close]
                    .iter()
                    .position(|&byte| byte == b'{')
                {
                    return Err(Error::NestedOpen {
                        pos: open + 1 + offset,
                    });
                }

                let mut content_start = open + 1;
                if matches!(
                    bytes.get(content_start),
                    Some(b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&')
                ) {
                    return Err(Error::UnsupportedOperator {
                        pos: content_start,
                    });
                }

                if content_start == close {
                    return Err(Error::EmptyExpression { pos: close });
                }

                let mut variables = Vec::new();
                let mut seen = std::collections::HashSet::new();

                loop {
                    let entry_start = content_start;
                    let entry_end = bytes[entry_start..close]
                        .iter()
                        .position(|&byte| byte == b',')
                        .map(|offset| entry_start + offset)
                        .unwrap_or(close);

                    if entry_start == entry_end {
                        return Err(Error::EmptyVariable { pos: entry_start });
                    }

                    let name_bytes = &bytes[entry_start..entry_end];
                    let valid_first = |b: u8| b.is_ascii_alphabetic() || b == b'_';
                    let valid_rest = |b: u8| b.is_ascii_alphanumeric() || b == b'_';

                    if !valid_first(name_bytes[0]) {
                        return Err(Error::InvalidName { pos: entry_start });
                    }
                    if let Some(offset) = name_bytes[1..].iter().position(|&b| !valid_rest(b)) {
                        return Err(Error::InvalidName {
                            pos: entry_start + 1 + offset,
                        });
                    }

                    let name = std::str::from_utf8(name_bytes).expect("validated ASCII name");
                    if !seen.insert(name.to_owned()) {
                        return Err(Error::DuplicateVariable { pos: entry_start });
                    }
                    variables.push(Variable {
                        name: name.to_owned(),
                        pos: entry_start,
                    });

                    if entry_end == close {
                        break;
                    }
                    content_start = entry_end + 1;
                }

                expressions.push((open, close, Expression { variables }));
                cursor = close + 1;
            }
            _ => cursor += 1,
        }
    }

    let mut output = String::new();
    let mut literal_start = 0;
    for (open, close, expression) in expressions {
        output.push_str(&template[literal_start..open]);

        let mut first = true;
        for variable in expression.variables {
            let _ = variable.pos;
            if let Some(value) = vars
                .iter()
                .rev()
                .find(|(name, _)| *name == variable.name)
                .map(|(_, value)| *value)
            {
                if !first {
                    output.push(',');
                }
                first = false;
                output.push_str(&encode(value));
            }
        }

        literal_start = close + 1;
    }
    output.push_str(&template[literal_start..]);
    Ok(output)
}

fn encode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = String::new();
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'%' && i + 2 < bytes.len() && is_hex(bytes[i + 1]) && is_hex(bytes[i + 2]) {
            output.push('%');
            output.push((bytes[i + 1] as char).to_ascii_uppercase());
            output.push((bytes[i + 2] as char).to_ascii_uppercase());
            i += 3;
        } else if is_unreserved(byte) {
            output.push(byte as char);
            i += 1;
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
            i += 1;
        }
    }

    output
}

fn is_hex(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn main() {}