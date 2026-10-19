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
    variables: Vec<(String, usize)>,
}

fn validate(template: &str) -> Result<Vec<Expression>, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                let start = i;
                let close = match bytes[start + 1..].iter().position(|&b| b == b'}') {
                    Some(offset) => start + 1 + offset,
                    None => return Err(Error::Unclosed { pos: start }),
                };

                if let Some(offset) = bytes[start + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: start + 1 + offset,
                    });
                }

                let mut content_start = start + 1;
                let operator = if content_start < close
                    && matches!(bytes[content_start], b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&')
                {
                    let op = bytes[content_start];
                    if !matches!(op, b'+' | b'#') {
                        return Err(Error::UnsupportedOperator { pos: content_start });
                    }
                    content_start += 1;
                    Some(op)
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
                    let spec_end = bytes[spec_start..close]
                        .iter()
                        .position(|&b| b == b',')
                        .map(|offset| spec_start + offset)
                        .unwrap_or(close);

                    if spec_start == spec_end {
                        return Err(Error::EmptyVariable { pos: spec_start });
                    }

                    let name_bytes = &bytes[spec_start..spec_end];
                    let valid_first = |b: u8| b.is_ascii_alphabetic() || b == b'_';
                    let valid_rest = |b: u8| b.is_ascii_alphanumeric() || b == b'_';

                    if !valid_first(name_bytes[0]) {
                        return Err(Error::InvalidName { pos: spec_start });
                    }
                    if let Some(offset) = name_bytes[1..].iter().position(|&b| !valid_rest(b)) {
                        return Err(Error::InvalidName {
                            pos: spec_start + 1 + offset,
                        });
                    }

                    let name = String::from_utf8(name_bytes.to_vec()).unwrap();
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
                    end: close,
                    operator,
                    variables,
                });
                i = close + 1;
            }
            _ => i += 1,
        }
    }

    Ok(expressions)
}

fn encode(value: &str, reserved: bool) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = value.as_bytes();
    let mut out = String::new();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];

        if b == b'%' && i + 2 < bytes.len() {
            let h1 = bytes[i + 1];
            let h2 = bytes[i + 2];
            if h1.is_ascii_hexdigit() && h2.is_ascii_hexdigit() {
                out.push('%');
                out.push((h1 as char).to_ascii_uppercase());
                out.push((h2 as char).to_ascii_uppercase());
                i += 3;
                continue;
            }
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
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
        i += 1;
    }

    out
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let expressions = validate(template)?;
    let mut out = String::new();
    let mut literal_start = 0;

    for expr in expressions {
        out.push_str(&template[literal_start..expr.start]);

        let mut fields = Vec::new();
        for (name, _) in &expr.variables {
            if let Some((_, value)) = vars.iter().rev().find(|(key, _)| *key == name) {
                fields.push(encode(value, matches!(expr.operator, Some(b'+' | b'#'))));
            }
        }

        if !fields.is_empty() {
            if expr.operator == Some(b'#') {
                out.push('#');
            }
            for (index, field) in fields.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(field);
            }
        }

        literal_start = expr.end + 1;
    }

    out.push_str(&template[literal_start..]);
    Ok(out)
}