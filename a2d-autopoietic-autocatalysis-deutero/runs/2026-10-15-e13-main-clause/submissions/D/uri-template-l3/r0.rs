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
    operator: Option<u8>,
    variables: Vec<(usize, String)>,
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn parse_expression(template: &str, open: usize, close: usize) -> Result<Expression, Error> {
    let bytes = template.as_bytes();

    if let Some(inner) = (open + 1..close).find(|&i| bytes[i] == b'{') {
        return Err(Error::NestedOpen { pos: inner });
    }

    let mut content_start = open + 1;
    let operator = if content_start < close
        && matches!(bytes[content_start], b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&')
    {
        let op = bytes[content_start];
        content_start += 1;
        Some(op)
    } else if content_start < close
        && matches!(bytes[content_start], b'!' | b'=' | b',' | b'@' | b'|' | b'>' | b'<')
    {
        return Err(Error::UnsupportedOperator { pos: content_start });
    } else {
        None
    };

    if content_start == close {
        return Err(Error::EmptyExpression { pos: close });
    }

    let mut variables = Vec::new();
    let mut seen = Vec::<String>::new();
    let mut spec_start = content_start;

    loop {
        let spec_end = (spec_start..close)
            .find(|&i| bytes[i] == b',')
            .unwrap_or(close);

        if spec_start == spec_end {
            return Err(Error::EmptyVariable { pos: spec_start });
        }

        let name_bytes = &bytes[spec_start..spec_end];
        if !is_name_start(name_bytes[0]) {
            return Err(Error::InvalidName { pos: spec_start });
        }
        if let Some(offset) = name_bytes.iter().position(|&b| !is_name_continue(b)) {
            return Err(Error::InvalidName {
                pos: spec_start + offset,
            });
        }

        let name = template[spec_start..spec_end].to_owned();
        if seen.iter().any(|existing| existing == &name) {
            return Err(Error::DuplicateVariable { pos: spec_start });
        }
        seen.push(name.clone());
        variables.push((spec_start, name));

        if spec_end == close {
            break;
        }
        spec_start = spec_end + 1;
    }

    Ok(Expression {
        start: open,
        end: close,
        operator,
        variables,
    })
}

fn encode(value: &str, reserved: bool) -> String {
    let bytes = value.as_bytes();
    let mut output = String::new();
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];

        if byte == b'%' && i + 2 < bytes.len()
            && bytes[i + 1].is_ascii_hexdigit()
            && bytes[i + 2].is_ascii_hexdigit()
        {
            output.push('%');
            output.push((bytes[i + 1] as char).to_ascii_uppercase());
            output.push((bytes[i + 2] as char).to_ascii_uppercase());
            i += 3;
            continue;
        }

        let unreserved = byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'.' | b'_' | b'~');
        let reserved_byte = reserved
            && matches!(
                byte,
                b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&'
                    | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
            );

        if unreserved || reserved_byte {
            output.push(byte as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
        i += 1;
    }

    output
}

fn expand_expression(expression: &Expression, vars: &[(&str, &str)]) -> String {
    let operator = expression.operator;
    let reserved = matches!(operator, Some(b'+') | Some(b'#'));
    let mut fields = Vec::<String>::new();

    for (_, name) in &expression.variables {
        if let Some((_, value)) = vars.iter().rev().find(|(binding, _)| *binding == name) {
            let encoded = encode(value, reserved);
            let field = match operator {
                Some(b';') if encoded.is_empty() => name.clone(),
                Some(b';' | b'?' | b'&') => format!("{name}={encoded}"),
                _ => encoded,
            };
            fields.push(field);
        }
    }

    if fields.is_empty() {
        return String::new();
    }

    let (prefix, separator) = match operator {
        Some(b'#') => ("#", ","),
        Some(b'.') => (".", "."),
        Some(b'/') => ("/", "/"),
        Some(b';') => (";", ";"),
        Some(b'?') => ("?", "&"),
        Some(b'&') => ("&", "&"),
        _ => ("", ","),
    };

    format!("{prefix}{}", fields.join(separator))
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                let Some(relative_close) = bytes[i + 1..].iter().position(|&b| b == b'}') else {
                    return Err(Error::Unclosed { pos: i });
                };
                let close = i + 1 + relative_close;
                expressions.push(parse_expression(template, i, close)?);
                i = close + 1;
            }
            _ => i += 1,
        }
    }

    let mut output = String::new();
    let mut literal_start = 0;
    for expression in &expressions {
        output.push_str(&template[literal_start..expression.start]);
        output.push_str(&expand_expression(expression, vars));
        literal_start = expression.end + 1;
    }
    output.push_str(&template[literal_start..]);
    Ok(output)
}