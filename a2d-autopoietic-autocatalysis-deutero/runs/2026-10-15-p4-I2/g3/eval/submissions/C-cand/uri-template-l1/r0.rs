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
    operator: Option<(u8, usize)>,
    names: Vec<(&'a str, usize)>,
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn parse_expression<'a>(
    template: &'a str,
    start: usize,
    end: usize,
) -> Result<Expression<'a>, Error> {
    let bytes = template.as_bytes();

    if let Some(relative) = bytes[start + 1..end].iter().position(|&b| b == b'{') {
        return Err(Error::NestedOpen {
            pos: start + 1 + relative,
        });
    }

    let mut content_start = start + 1;
    let operator = match bytes.get(content_start).copied() {
        Some(b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&') => {
            let op = (bytes[content_start], content_start);
            content_start += 1;
            Some(op)
        }
        _ => None,
    };

    if let Some((op, pos)) = operator {
        if op != b'{' {
            return Err(Error::UnsupportedOperator { pos });
        }
    }

    if content_start == end {
        return Err(Error::EmptyExpression { pos: end });
    }

    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut spec_start = content_start;

    loop {
        let spec_end = bytes[spec_start..end]
            .iter()
            .position(|&b| b == b',')
            .map_or(end, |offset| spec_start + offset);

        if spec_start == spec_end {
            return Err(Error::EmptyVariable { pos: spec_start });
        }

        let name_bytes = &bytes[spec_start..spec_end];
        if !is_name_start(name_bytes[0]) {
            return Err(Error::InvalidName { pos: spec_start });
        }
        if let Some(offset) = name_bytes[1..]
            .iter()
            .position(|&b| !is_name_continue(b))
        {
            return Err(Error::InvalidName {
                pos: spec_start + 1 + offset,
            });
        }

        let name = &template[spec_start..spec_end];
        if !seen.insert(name) {
            return Err(Error::DuplicateVariable { pos: spec_start });
        }
        names.push((name, spec_start));

        if spec_end == end {
            break;
        }
        spec_start = spec_end + 1;
    }

    Ok(Expression {
        start,
        end,
        operator,
        names,
    })
}

fn validate(template: &str) -> Result<Vec<Expression<'_>>, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'}' => return Err(Error::UnexpectedClose { pos: index }),
            b'{' => {
                let Some(relative_end) = bytes[index + 1..].iter().position(|&b| b == b'}') else {
                    return Err(Error::Unclosed { pos: index });
                };
                let end = index + 1 + relative_end;
                expressions.push(parse_expression(template, index, end)?);
                index = end + 1;
            }
            _ => index += 1,
        }
    }

    Ok(expressions)
}

fn encode_value(value: &str, output: &mut String) {
    let bytes = value.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];

        if byte == b'%' && index + 2 < bytes.len() {
            let a = bytes[index + 1];
            let b = bytes[index + 2];
            if a.is_ascii_hexdigit() && b.is_ascii_hexdigit() {
                output.push('%');
                output.push((a as char).to_ascii_uppercase());
                output.push((b as char).to_ascii_uppercase());
                index += 3;
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
        index += 1;
    }
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let expressions = validate(template)?;
    let mut output = String::with_capacity(template.len());
    let mut cursor = 0;

    for expression in expressions {
        output.push_str(&template[cursor..expression.start]);

        let mut contributed = false;
        for (name, _) in expression.names {
            if let Some((_, value)) = vars.iter().rev().find(|(key, _)| key == &name) {
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

fn main() {}