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
    content_start: usize,
    operator: Option<u8>,
    names: Vec<(usize, usize)>,
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn parse_expression(
    template: &str,
    open: usize,
    close: usize,
) -> Result<Expression, Error> {
    let bytes = template.as_bytes();
    let content_start = open + 1;
    let content_end = close;

    if let Some(inner) = (content_start..content_end).find(|&i| bytes[i] == b'{') {
        return Err(Error::NestedOpen { pos: inner });
    }

    let operator = match bytes.get(content_start).copied() {
        Some(b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&') => {
            Some(bytes[content_start])
        }
        _ => None,
    };

    if operator.is_some() {
        return Err(Error::UnsupportedOperator { pos: content_start });
    }

    if content_start == content_end {
        return Err(Error::EmptyExpression { pos: close });
    }

    let mut names = Vec::new();
    let mut spec_start = content_start;

    loop {
        let spec_end = (spec_start..content_end)
            .find(|&i| bytes[i] == b',')
            .unwrap_or(content_end);

        if spec_start == spec_end {
            return Err(Error::EmptyVariable { pos: spec_start });
        }

        if !is_name_start(bytes[spec_start]) {
            return Err(Error::InvalidName { pos: spec_start });
        }

        let mut i = spec_start + 1;
        while i < spec_end && is_name_continue(bytes[i]) {
            i += 1;
        }
        if i < spec_end {
            return Err(Error::InvalidName { pos: i });
        }

        if names.iter().any(|&(s, e)| &template[s..e] == &template[spec_start..spec_end]) {
            return Err(Error::DuplicateVariable { pos: spec_start });
        }
        names.push((spec_start, spec_end));

        if spec_end == content_end {
            break;
        }
        spec_start = spec_end + 1;
    }

    Ok(Expression {
        start: open,
        end: close + 1,
        content_start,
        operator,
        names,
    })
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
                let close = match (i + 1..bytes.len()).find(|&j| bytes[j] == b'}') {
                    Some(pos) => pos,
                    None => return Err(Error::Unclosed { pos: open }),
                };
                expressions.push(parse_expression(template, open, close)?);
                i = close + 1;
            }
            _ => i += 1,
        }
    }

    Ok(expressions)
}

fn encode_value(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = value.as_bytes();
    let mut output = String::new();
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'%' && i + 2 < bytes.len() {
            let a = bytes[i + 1];
            let b = bytes[i + 2];
            if a.is_ascii_hexdigit() && b.is_ascii_hexdigit() {
                output.push('%');
                output.push((a as char).to_ascii_uppercase());
                output.push((b as char).to_ascii_uppercase());
                i += 3;
                continue;
            }
        }

        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
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

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let expressions = validate(template)?;
    let mut output = String::new();
    let mut cursor = 0;

    for expression in expressions {
        output.push_str(&template[cursor..expression.start]);

        let _content_start = expression.content_start;
        let _operator = expression.operator;
        let mut fields = Vec::new();

        for &(start, end) in &expression.names {
            let name = &template[start..end];
            if let Some((_, value)) = vars.iter().rev().find(|(binding, _)| *binding == name) {
                fields.push(encode_value(value));
            }
        }

        output.push_str(&fields.join(","));
        cursor = expression.end;
    }

    output.push_str(&template[cursor..]);
    Ok(output)
}

fn main() {}