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
    variables: Vec<(String, usize)>,
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn validate_expression(template: &str, start: usize, end: usize) -> Result<Expression, Error> {
    let bytes = template.as_bytes();
    let mut content_start = start + 1;

    if matches!(bytes[content_start], b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&') {
        return Err(Error::UnsupportedOperator { pos: content_start });
    }

    if content_start == end {
        return Err(Error::EmptyExpression { pos: end });
    }

    let mut variables = Vec::new();
    let mut seen = Vec::<&str>::new();
    let mut spec_start = content_start;

    loop {
        let spec_end = (spec_start..end)
            .find(|&i| bytes[i] == b',')
            .unwrap_or(end);

        if spec_start == spec_end {
            return Err(Error::EmptyVariable { pos: spec_start });
        }

        let name_bytes = &bytes[spec_start..spec_end];
        if !is_name_start(name_bytes[0]) {
            return Err(Error::InvalidName { pos: spec_start });
        }
        if let Some(offset) = name_bytes.iter().enumerate().skip(1)
            .find_map(|(i, &byte)| (!is_name_continue(byte)).then_some(i))
        {
            return Err(Error::InvalidName { pos: spec_start + offset });
        }

        let name = &template[spec_start..spec_end];
        if seen.contains(&name) {
            return Err(Error::DuplicateVariable { pos: spec_start });
        }
        seen.push(name);
        variables.push((name.to_owned(), spec_start));

        if spec_end == end {
            break;
        }
        spec_start = spec_end + 1;
    }

    Ok(Expression { start, end, variables })
}

fn parse_template(template: &str) -> Result<Vec<Expression>, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                let start = i;
                let Some(relative_close) = bytes[i + 1..].iter().position(|&b| b == b'}') else {
                    return Err(Error::Unclosed { pos: start });
                };
                let end = i + 1 + relative_close;

                if let Some(relative_open) = bytes[start + 1..end].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: start + 1 + relative_open,
                    });
                }

                expressions.push(validate_expression(template, start, end)?);
                i = end + 1;
            }
            _ => i += 1,
        }
    }

    Ok(expressions)
}

fn encode_value(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = value.as_bytes();
    let mut output = String::with_capacity(bytes.len());

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
        } else if byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'.' | b'_' | b'~')
        {
            output.push(byte as char);
            i += 1;
        } else {
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0F) as usize] as char);
            i += 1;
        }
    }

    output
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let expressions = parse_template(template)?;
    let mut output = String::with_capacity(template.len());
    let mut literal_start = 0;

    for expression in expressions {
        output.push_str(&template[literal_start..expression.start]);

        let mut fields = Vec::new();
        for (name, _) in expression.variables {
            if let Some((_, value)) = vars.iter().rev().find(|(binding, _)| *binding == name) {
                fields.push(encode_value(value));
            }
        }

        if !fields.is_empty() {
            output.push_str(&fields.join(","));
        }

        literal_start = expression.end + 1;
    }

    output.push_str(&template[literal_start..]);
    Ok(output)
}