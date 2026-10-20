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
    names: Vec<(usize, usize)>,
}

fn is_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_name_continue(b: u8) -> bool {
    is_name_start(b) || b.is_ascii_digit()
}

fn parse_expression(template: &str, open: usize, close: usize) -> Result<Expression, Error> {
    let bytes = template.as_bytes();

    if let Some(inner) = (open + 1..close).find(|&i| bytes[i] == b'{') {
        return Err(Error::NestedOpen { pos: inner });
    }

    let mut content_start = open + 1;
    if content_start == close {
        return Err(Error::EmptyExpression { pos: close });
    }

    if matches!(bytes[content_start], b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&') {
        return Err(Error::UnsupportedOperator { pos: content_start });
    }

    let mut names = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    let mut item_start = content_start;

    loop {
        let item_end = (item_start..close)
            .find(|&i| bytes[i] == b',')
            .unwrap_or(close);

        if item_start == item_end {
            return Err(Error::EmptyVariable { pos: item_start });
        }

        let mut invalid_at = None;
        for i in item_start..item_end {
            let valid = if i == item_start {
                is_name_start(bytes[i])
            } else {
                is_name_continue(bytes[i])
            };
            if !valid {
                invalid_at = Some(i);
                break;
            }
        }

        if let Some(pos) = invalid_at {
            return Err(Error::InvalidName { pos });
        }

        let name = &template[item_start..item_end];
        if seen.iter().any(|&previous| previous == name) {
            return Err(Error::DuplicateVariable { pos: item_start });
        }
        seen.push(name);
        names.push((item_start, item_end));

        if item_end == close {
            break;
        }
        item_start = item_end + 1;
    }

    Ok(Expression {
        start: open,
        end: close,
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
                let close = match (open + 1..bytes.len()).find(|&j| bytes[j] == b'}') {
                    Some(close) => close,
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

fn lookup<'a>(name: &str, vars: &'a [(&str, &str)]) -> Option<&'a str> {
    vars.iter()
        .rev()
        .find(|(binding_name, _)| *binding_name == name)
        .map(|(_, value)| *value)
}

fn encode_value(value: &str, out: &mut String) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = value.as_bytes();
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
        } else if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
            i += 1;
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0F) as usize] as char);
            i += 1;
        }
    }
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let expressions = validate(template)?;
    let mut out = String::with_capacity(template.len());
    let mut cursor = 0;

    for expression in expressions {
        out.push_str(&template[cursor..expression.start]);
        let mut first = true;

        for &(name_start, name_end) in &expression.names {
            let name = &template[name_start..name_end];
            if let Some(value) = lookup(name, vars) {
                if !first {
                    out.push(',');
                }
                encode_value(value, &mut out);
                first = false;
            }
        }

        cursor = expression.end + 1;
    }

    out.push_str(&template[cursor..]);
    Ok(out)
}
