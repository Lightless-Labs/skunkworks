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

fn validate(template: &str) -> Result<Vec<Expression>, Error> {
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

                if let Some(relative_open) = bytes[start + 1..end].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: start + 1 + relative_open,
                    });
                }

                let mut content_start = start + 1;
                if content_start == end {
                    return Err(Error::EmptyExpression { pos: end });
                }

                if matches!(bytes[content_start], b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&') {
                    return Err(Error::UnsupportedOperator { pos: content_start });
                }

                let mut names = Vec::new();
                let mut seen: Vec<&str> = Vec::new();
                loop {
                    let spec_start = content_start;
                    let spec_end = bytes[spec_start..end]
                        .iter()
                        .position(|&b| b == b',')
                        .map_or(end, |relative| spec_start + relative);

                    if spec_start == spec_end {
                        return Err(Error::EmptyVariable { pos: spec_start });
                    }

                    let name_bytes = &bytes[spec_start..spec_end];
                    let first_valid = name_bytes[0].is_ascii_alphabetic() || name_bytes[0] == b'_';
                    if !first_valid {
                        return Err(Error::InvalidName { pos: spec_start });
                    }
                    for (offset, &byte) in name_bytes.iter().enumerate().skip(1) {
                        if !(byte.is_ascii_alphanumeric() || byte == b'_') {
                            return Err(Error::InvalidName { pos: spec_start + offset });
                        }
                    }

                    let name = &template[spec_start..spec_end];
                    if seen.contains(&name) {
                        return Err(Error::DuplicateVariable { pos: spec_start });
                    }
                    seen.push(name);
                    names.push((spec_start, name.to_owned()));

                    if spec_end == end {
                        break;
                    }
                    content_start = spec_end + 1;
                }

                expressions.push(Expression { start, end, names });
                i = end + 1;
            }
            _ => i += 1,
        }
    }

    Ok(expressions)
}

fn encode_value(value: &str, output: &mut String) {
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'%' && i + 2 < bytes.len() {
            let hi = bytes[i + 1];
            let lo = bytes[i + 2];
            if hi.is_ascii_hexdigit() && lo.is_ascii_hexdigit() {
                output.push('%');
                output.push((hi as char).to_ascii_uppercase());
                output.push((lo as char).to_ascii_uppercase());
                i += 3;
                continue;
            }
        }

        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            output.push(byte as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
        i += 1;
    }
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let expressions = validate(template)?;
    let mut output = String::new();
    let mut cursor = 0;

    for expression in expressions {
        output.push_str(&template[cursor..expression.start]);

        let mut contributed = false;
        for (_, name) in &expression.names {
            if let Some((_, value)) = vars.iter().rev().find(|(key, _)| *key == name) {
                if contributed {
                    output.push(',');
                }
                encode_value(value, &mut output);
                contributed = true;
            }
        }

        cursor = expression.end + 1;
    }

    output.push_str(&template[cursor..]);
    Ok(output)
}