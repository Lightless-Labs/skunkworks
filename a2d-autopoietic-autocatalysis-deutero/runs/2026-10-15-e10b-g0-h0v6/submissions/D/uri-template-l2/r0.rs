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
    operator: Option<char>,
    names: Vec<(&'a str, usize)>,
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
                let Some(relative_close) = bytes[open + 1..].iter().position(|&b| b == b'}') else {
                    return Err(Error::Unclosed { pos: open });
                };
                let close = open + 1 + relative_close;

                if let Some(relative_open) = bytes[open + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: open + 1 + relative_open,
                    });
                }

                let content_start = open + 1;
                let mut name_start = content_start;
                let mut operator = None;

                if content_start < close
                    && matches!(
                        bytes[content_start],
                        b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&'
                    )
                {
                    let op = bytes[content_start] as char;
                    if op != '+' && op != '#' {
                        return Err(Error::UnsupportedOperator { pos: content_start });
                    }
                    operator = Some(op);
                    name_start += 1;
                }

                if name_start == close {
                    return Err(Error::EmptyExpression { pos: close });
                }

                let mut names = Vec::new();
                let mut seen: Vec<&str> = Vec::new();
                let mut part_start = name_start;

                loop {
                    let part_end = bytes[part_start..close]
                        .iter()
                        .position(|&b| b == b',')
                        .map(|relative| part_start + relative)
                        .unwrap_or(close);

                    if part_start == part_end {
                        return Err(Error::EmptyVariable { pos: part_start });
                    }

                    let part = &template[part_start..part_end];
                    let part_bytes = part.as_bytes();
                    if !is_name_start(part_bytes[0]) {
                        return Err(Error::InvalidName { pos: part_start });
                    }

                    for (offset, &byte) in part_bytes.iter().enumerate().skip(1) {
                        if !is_name_continue(byte) {
                            return Err(Error::InvalidName {
                                pos: part_start + offset,
                            });
                        }
                    }

                    if seen.contains(&part) {
                        return Err(Error::DuplicateVariable { pos: part_start });
                    }

                    seen.push(part);
                    names.push((part, part_start));

                    if part_end == close {
                        break;
                    }
                    part_start = part_end + 1;
                }

                expressions.push(Expression {
                    start: open,
                    end: close + 1,
                    operator,
                    names,
                });
                cursor = close + 1;
            }
            _ => cursor += 1,
        }
    }

    let mut output = String::with_capacity(template.len());
    let mut literal_start = 0;

    for expression in expressions {
        output.push_str(&template[literal_start..expression.start]);

        let mut fields = Vec::new();
        for (name, _) in expression.names {
            if let Some((_, value)) = vars.iter().rev().find(|(binding, _)| *binding == name) {
                fields.push(encode(value, expression.operator.is_some()));
            }
        }

        if !fields.is_empty() {
            if expression.operator == Some('#') {
                output.push('#');
            }
            for (index, field) in fields.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                output.push_str(field);
            }
        }

        literal_start = expression.end;
    }

    output.push_str(&template[literal_start..]);
    Ok(output)
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn encode(value: &str, allow_reserved: bool) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = value.as_bytes();
    let mut output = String::with_capacity(bytes.len());

    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];

        if byte == b'%'
            && i + 2 < bytes.len()
            && bytes[i + 1].is_ascii_hexdigit()
            && bytes[i + 2].is_ascii_hexdigit()
        {
            output.push('%');
            output.push((bytes[i + 1] as char).to_ascii_uppercase());
            output.push((bytes[i + 2] as char).to_ascii_uppercase());
            i += 3;
            continue;
        }

        if is_unreserved(byte) || (allow_reserved && is_reserved(byte)) {
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

fn is_reserved(byte: u8) -> bool {
    matches!(
        byte,
        b':'
            | b'/'
            | b'?'
            | b'#'
            | b'['
            | b']'
            | b'@'
            | b'!'
            | b'$'
            | b'&'
            | b'\''
            | b'('
            | b')'
            | b'*'
            | b'+'
            | b','
            | b';'
            | b'='
    )
}