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
    start: usize,
    end: usize,
    names: Vec<(usize, String)>,
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut literal_start = 0;
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                let open = i;
                let close = match bytes[i + 1..].iter().position(|&b| b == b'}') {
                    Some(offset) => i + 1 + offset,
                    None => return Err(Error::Unclosed { pos: open }),
                };

                if let Some(offset) = bytes[i + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen { pos: i + 1 + offset });
                }

                let mut content_start = open + 1;
                if content_start == close {
                    return Err(Error::EmptyExpression { pos: close });
                }

                if matches!(bytes[content_start], b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&') {
                    return Err(Error::UnsupportedOperator { pos: content_start });
                }

                let mut names = Vec::new();
                let mut spec_start = content_start;
                loop {
                    let spec_end = bytes[spec_start..close]
                        .iter()
                        .position(|&b| b == b',')
                        .map(|offset| spec_start + offset)
                        .unwrap_or(close);

                    if spec_start == spec_end {
                        return Err(Error::EmptyVariable { pos: spec_start });
                    }

                    let spec = &bytes[spec_start..spec_end];
                    if !is_name_start(spec[0]) {
                        return Err(Error::InvalidName { pos: spec_start });
                    }
                    if let Some(offset) = spec[1..].iter().position(|&b| !is_name_continue(b)) {
                        return Err(Error::InvalidName { pos: spec_start + 1 + offset });
                    }

                    let name = template[spec_start..spec_end].to_owned();
                    if names.iter().any(|(_, existing)| existing == &name) {
                        return Err(Error::DuplicateVariable { pos: spec_start });
                    }
                    names.push((spec_start, name));

                    if spec_end == close {
                        break;
                    }
                    spec_start = spec_end + 1;
                }

                expressions.push(Expression {
                    start: open,
                    end: close + 1,
                    names,
                });
                i = close + 1;
                literal_start = i;
            }
            _ => i += 1,
        }
    }

    let mut output = String::new();
    let mut cursor = 0;
    for expression in expressions {
        output.push_str(&template[cursor..expression.start]);

        let mut fields = Vec::new();
        for (_, name) in expression.names {
            if let Some((_, value)) = vars.iter().rev().find(|(binding, _)| *binding == name) {
                fields.push(encode_value(value));
            }
        }
        output.push_str(&fields.join(","));
        cursor = expression.end;
    }
    output.push_str(&template[cursor..]);
    let _ = literal_start;
    Ok(output)
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn encode_value(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = value.as_bytes();
    let mut out = String::new();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() && bytes[i + 1].is_ascii_hexdigit() && bytes[i + 2].is_ascii_hexdigit() {
            out.push('%');
            out.push((bytes[i + 1] as char).to_ascii_uppercase());
            out.push((bytes[i + 2] as char).to_ascii_uppercase());
            i += 3;
            continue;
        }

        let byte = bytes[i];
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0F) as usize] as char);
        }
        i += 1;
    }

    out
}

fn main() {}