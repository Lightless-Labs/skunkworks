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

#[derive(Clone)]
struct Expression {
    start: usize,
    end: usize,
    operator: Option<u8>,
    variables: Vec<Variable>,
}

#[derive(Clone)]
struct Variable {
    name: String,
    start: usize,
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_byte(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn operator(byte: u8) -> bool {
    matches!(byte, b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&')
}

fn validate(template: &str) -> Result<Vec<Expression>, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                let open = i;
                let Some(relative_close) = bytes[i + 1..].iter().position(|&b| b == b'}') else {
                    return Err(Error::Unclosed { pos: open });
                };
                let close = i + 1 + relative_close;

                if let Some(relative_inner) = bytes[open + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: open + 1 + relative_inner,
                    });
                }

                let mut content_start = open + 1;
                let mut op = None;
                if content_start < close && operator(bytes[content_start]) {
                    let byte = bytes[content_start];
                    if !matches!(byte, b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&') {
                        return Err(Error::UnsupportedOperator { pos: content_start });
                    }
                    op = Some(byte);
                    content_start += 1;
                }

                if content_start == close {
                    return Err(Error::EmptyExpression { pos: close });
                }

                let mut variables = Vec::new();
                let mut seen = std::collections::HashSet::new();
                let mut part_start = content_start;

                loop {
                    let part_end = bytes[part_start..close]
                        .iter()
                        .position(|&b| b == b',')
                        .map(|n| part_start + n)
                        .unwrap_or(close);

                    if part_start == part_end {
                        return Err(Error::EmptyVariable { pos: part_start });
                    }

                    let name_bytes = &bytes[part_start..part_end];
                    if !is_name_start(name_bytes[0]) {
                        return Err(Error::InvalidName { pos: part_start });
                    }
                    if let Some(offset) = name_bytes.iter().position(|&b| !is_name_byte(b)) {
                        return Err(Error::InvalidName { pos: part_start + offset });
                    }

                    let name = std::str::from_utf8(name_bytes).expect("ASCII name");
                    if !seen.insert(name.to_owned()) {
                        return Err(Error::DuplicateVariable { pos: part_start });
                    }
                    variables.push(Variable {
                        name: name.to_owned(),
                        start: part_start,
                    });

                    if part_end == close {
                        break;
                    }
                    part_start = part_end + 1;
                }

                expressions.push(Expression {
                    start: open,
                    end: close,
                    operator: op,
                    variables,
                });
                i = close + 1;
            }
            _ => i += 1,
        }
    }

    Ok(expressions)
}

fn lookup<'a>(name: &str, vars: &'a [(&str, &str)]) -> Option<&'a str> {
    vars.iter()
        .rev()
        .find(|(binding, _)| *binding == name)
        .map(|(_, value)| *value)
}

fn encode(value: &str, reserved: bool) -> String {
    let bytes = value.as_bytes();
    let mut out = String::new();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];

        if b == b'%' && i + 2 < bytes.len()
            && bytes[i + 1].is_ascii_hexdigit()
            && bytes[i + 2].is_ascii_hexdigit()
        {
            out.push('%');
            out.push((bytes[i + 1] as char).to_ascii_uppercase());
            out.push((bytes[i + 2] as char).to_ascii_uppercase());
            i += 3;
            continue;
        }

        let unreserved = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~');
        let reserved_byte = reserved
            && matches!(
                b,
                b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&'
                    | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
            );

        if unreserved || reserved_byte {
            out.push(b as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
        i += 1;
    }

    out
}

fn expand_expression(expression: &Expression, vars: &[(&str, &str)]) -> String {
    let op = expression.operator;
    let mut fields = Vec::new();

    for variable in &expression.variables {
        if let Some(value) = lookup(&variable.name, vars) {
            let encoded = encode(value, matches!(op, Some(b'+') | Some(b'#')));
            let field = match op {
                Some(b';') if encoded.is_empty() => variable.name.clone(),
                Some(b';' | b'?' | b'&') => format!("{}={}", variable.name, encoded),
                _ => encoded,
            };
            fields.push(field);
        }
    }

    if fields.is_empty() {
        return String::new();
    }

    let (prefix, separator) = match op {
        Some(b'#') => ("#", ","),
        Some(b'.') => (".", "."),
        Some(b'/') => ("/", "/"),
        Some(b';') => (";", ";"),
        Some(b'?') => ("?", "&"),
        Some(b'&') => ("&", "&"),
        _ => ("", ","),
    };

    let mut out = String::from(prefix);
    out.push_str(&fields.join(separator));
    out
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let expressions = validate(template)?;
    let mut out = String::new();
    let mut cursor = 0;

    for expression in &expressions {
        out.push_str(&template[cursor..expression.start]);
        out.push_str(&expand_expression(expression, vars));
        cursor = expression.end + 1;
    }

    out.push_str(&template[cursor..]);
    Ok(out)
}

fn main() {}