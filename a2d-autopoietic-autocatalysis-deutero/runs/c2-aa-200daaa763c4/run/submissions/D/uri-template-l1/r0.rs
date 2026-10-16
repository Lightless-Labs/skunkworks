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
struct Specifier {
    name: String,
    start: usize,
}

#[derive(Debug)]
enum Part {
    Literal(String),
    Expression(Vec<Specifier>),
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn parse(template: &str) -> Result<Vec<Part>, Error> {
    let bytes = template.as_bytes();
    let mut parts = Vec::new();
    let mut literal_start = 0;
    let mut pos = 0;

    while pos < bytes.len() {
        match bytes[pos] {
            b'}' => return Err(Error::UnexpectedClose { pos }),
            b'{' => {
                if literal_start < pos {
                    parts.push(Part::Literal(template[literal_start..pos].to_owned()));
                }

                let open = pos;
                let close = match bytes[pos + 1..].iter().position(|&b| b == b'}') {
                    Some(offset) => pos + 1 + offset,
                    None => return Err(Error::Unclosed { pos: open }),
                };

                if let Some(offset) = bytes[pos + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen { pos: pos + 1 + offset });
                }

                let mut content_start = open + 1;
                if content_start == close {
                    return Err(Error::EmptyExpression { pos: close });
                }

                if matches!(bytes[content_start], b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&') {
                    return Err(Error::UnsupportedOperator { pos: content_start });
                }

                let mut specifiers = Vec::new();
                let mut seen = Vec::<String>::new();
                let mut start = content_start;

                loop {
                    let end = bytes[start..close]
                        .iter()
                        .position(|&b| b == b',')
                        .map(|offset| start + offset)
                        .unwrap_or(close);

                    if start == end {
                        return Err(Error::EmptyVariable { pos: start });
                    }

                    let name_bytes = &bytes[start..end];
                    if !is_name_start(name_bytes[0]) {
                        return Err(Error::InvalidName { pos: start });
                    }
                    if let Some(offset) = name_bytes.iter().enumerate().skip(1)
                        .find_map(|(i, &b)| if is_name_continue(b) { None } else { Some(i) })
                    {
                        return Err(Error::InvalidName { pos: start + offset });
                    }

                    let name = template[start..end].to_owned();
                    if seen.iter().any(|previous| previous == &name) {
                        return Err(Error::DuplicateVariable { pos: start });
                    }
                    seen.push(name.clone());
                    specifiers.push(Specifier { name, start });

                    if end == close {
                        break;
                    }
                    start = end + 1;
                }

                parts.push(Part::Expression(specifiers));
                pos = close + 1;
                literal_start = pos;
            }
            _ => pos += 1,
        }
    }

    if literal_start < template.len() {
        parts.push(Part::Literal(template[literal_start..].to_owned()));
    }

    Ok(parts)
}

fn encode(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = value.as_bytes();
    let mut result = String::with_capacity(bytes.len());
    let mut pos = 0;

    while pos < bytes.len() {
        let byte = bytes[pos];

        if byte == b'%' && pos + 2 < bytes.len()
            && bytes[pos + 1].is_ascii_hexdigit()
            && bytes[pos + 2].is_ascii_hexdigit()
        {
            result.push('%');
            result.push((bytes[pos + 1] as char).to_ascii_uppercase());
            result.push((bytes[pos + 2] as char).to_ascii_uppercase());
            pos += 3;
        } else if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            result.push(byte as char);
            pos += 1;
        } else {
            result.push('%');
            result.push(HEX[(byte >> 4) as usize] as char);
            result.push(HEX[(byte & 0x0f) as usize] as char);
            pos += 1;
        }
    }

    result
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let parts = parse(template)?;
    let mut output = String::new();

    for part in parts {
        match part {
            Part::Literal(text) => output.push_str(&text),
            Part::Expression(specifiers) => {
                let mut fields = Vec::new();

                for specifier in specifiers {
                    let _ = specifier.start;
                    if let Some((_, value)) = vars.iter().rev().find(|(name, _)| *name == specifier.name) {
                        fields.push(encode(value));
                    }
                }

                if !fields.is_empty() {
                    output.push_str(&fields.join(","));
                }
            }
        }
    }

    Ok(output)
}