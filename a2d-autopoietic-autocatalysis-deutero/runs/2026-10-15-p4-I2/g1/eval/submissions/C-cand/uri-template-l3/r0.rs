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
struct Expression<'a> {
    start: usize,
    end: usize,
    operator: Option<u8>,
    variables: Vec<(&'a str, usize)>,
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                let start = i;
                let Some(relative_end) = bytes[i + 1..].iter().position(|&b| b == b'}') else {
                    return Err(Error::Unclosed { pos: start });
                };
                let end = i + 1 + relative_end;

                if let Some(relative_nested) = bytes[start + 1..end].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: start + 1 + relative_nested,
                    });
                }

                let mut content_start = start + 1;
                let operator = match bytes.get(content_start).copied() {
                    Some(op @ (b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&')) => {
                        content_start += 1;
                        Some(op)
                    }
                    Some(op @ (b'!' | b'=' | b'@' | b'|' | b',')) => {
                        return Err(Error::UnsupportedOperator { pos: content_start });
                    }
                    _ => None,
                };

                if content_start == end {
                    return Err(Error::EmptyExpression { pos: end });
                }

                let mut variables = Vec::new();
                let mut spec_start = content_start;
                loop {
                    let spec_end = bytes[spec_start..end]
                        .iter()
                        .position(|&b| b == b',')
                        .map_or(end, |relative| spec_start + relative);

                    if spec_start == spec_end {
                        return Err(Error::EmptyVariable { pos: spec_start });
                    }

                    let name_bytes = &bytes[spec_start..spec_end];
                    let valid_first = |b: u8| b.is_ascii_alphabetic() || b == b'_';
                    let valid_rest = |b: u8| valid_first(b) || b.is_ascii_digit();

                    if !valid_first(name_bytes[0]) {
                        return Err(Error::InvalidName { pos: spec_start });
                    }
                    if let Some(offset) = name_bytes.iter().position(|&b| !valid_rest(b)) {
                        return Err(Error::InvalidName {
                            pos: spec_start + offset,
                        });
                    }

                    let name = &template[spec_start..spec_end];
                    if variables.iter().any(|(prior, _)| *prior == name) {
                        return Err(Error::DuplicateVariable { pos: spec_start });
                    }
                    variables.push((name, spec_start));

                    if spec_end == end {
                        break;
                    }
                    spec_start = spec_end + 1;
                    if spec_start == end {
                        return Err(Error::EmptyVariable { pos: end });
                    }
                }

                expressions.push(Expression {
                    start,
                    end,
                    operator,
                    variables,
                });
                i = end + 1;
            }
            _ => i += 1,
        }
    }

    let mut output = String::with_capacity(template.len());
    let mut literal_start = 0;

    for expression in expressions {
        output.push_str(&template[literal_start..expression.start]);

        let mut fields: Vec<String> = Vec::new();
        for (name, _) in expression.variables {
            if let Some(value) = vars
                .iter()
                .rev()
                .find_map(|(binding, value)| (*binding == name).then_some(*value))
            {
                let encoded = encode(value, matches!(expression.operator, Some(b'+' | b'#')));
                let field = match expression.operator {
                    Some(b';') if encoded.is_empty() => name.to_owned(),
                    Some(b';' | b'?' | b'&') => format!("{name}={encoded}"),
                    _ => encoded,
                };
                fields.push(field);
            }
        }

        if !fields.is_empty() {
            let (prefix, separator) = match expression.operator {
                Some(b'#') => ("#", ","),
                Some(b'.') => (".", "."),
                Some(b'/') => ("/", "/"),
                Some(b';') => (";", ";"),
                Some(b'?') => ("?", "&"),
                Some(b'&') => ("&", "&"),
                _ => ("", ","),
            };
            output.push_str(prefix);
            output.push_str(&fields.join(separator));
        }

        literal_start = expression.end + 1;
    }
    output.push_str(&template[literal_start..]);

    Ok(output)
}

fn encode(value: &str, allow_reserved: bool) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = value.as_bytes();
    let mut output = String::with_capacity(bytes.len());

    let is_unreserved = |b: u8| {
        b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~')
    };
    let is_reserved = |b: u8| {
        matches!(
            b,
            b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&'
                | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
        )
    };
    let is_hex = |b: u8| b.is_ascii_hexdigit();

    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'%' && i + 2 < bytes.len() && is_hex(bytes[i + 1]) && is_hex(bytes[i + 2]) {
            output.push('%');
            output.push((bytes[i + 1] as char).to_ascii_uppercase());
            output.push((bytes[i + 2] as char).to_ascii_uppercase());
            i += 3;
        } else if is_unreserved(byte) || (allow_reserved && is_reserved(byte)) {
            output.push(byte as char);
            i += 1;
        } else {
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
            i += 1;
        }
    }

    output
}

fn main() {}