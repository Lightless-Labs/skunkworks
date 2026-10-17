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
    operator: Option<u8>,
    variables: Vec<(String, usize)>,
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn is_hex(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

fn parse_expression(template: &str, start: usize, end: usize) -> Result<Expression, Error> {
    let bytes = template.as_bytes();

    if let Some(inner) = (start + 1..end).find(|&i| bytes[i] == b'{') {
        return Err(Error::NestedOpen { pos: inner });
    }

    let mut content_start = start + 1;
    let first = bytes[content_start];
    let operator = match first {
        b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&' => {
            content_start += 1;
            Some(first)
        }
        _ => None,
    };

    if content_start == end {
        return Err(Error::EmptyExpression { pos: end });
    }

    let mut variables = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut item_start = content_start;

    for item_end in content_start..=end {
        if item_end != end && bytes[item_end] != b',' {
            continue;
        }

        if item_start == item_end {
            return Err(Error::EmptyVariable { pos: item_start });
        }

        let name_bytes = &bytes[item_start..item_end];
        if !is_name_start(name_bytes[0]) {
            return Err(Error::InvalidName { pos: item_start });
        }
        if let Some(offset) = name_bytes.iter().enumerate().skip(1).find_map(|(i, &b)| {
            (!is_name_continue(b)).then_some(i)
        }) {
            return Err(Error::InvalidName {
                pos: item_start + offset,
            });
        }

        let name = template[item_start..item_end].to_owned();
        if !seen.insert(name.clone()) {
            return Err(Error::DuplicateVariable { pos: item_start });
        }
        variables.push((name, item_start));
        item_start = item_end + 1;
    }

    if operator.is_some_and(|op| !matches!(op, b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&')) {
        return Err(Error::UnsupportedOperator { pos: start + 1 });
    }

    Ok(Expression {
        start,
        end,
        operator,
        variables,
    })
}

fn validate_template(template: &str) -> Result<Vec<Expression>, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                let Some(relative_end) = bytes[i + 1..].iter().position(|&b| b == b'}') else {
                    return Err(Error::Unclosed { pos: i });
                };
                let end = i + 1 + relative_end;
                expressions.push(parse_expression(template, i, end)?);
                i = end + 1;
            }
            _ => i += 1,
        }
    }

    Ok(expressions)
}

fn encode_value(value: &str, reserved: bool) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = value.as_bytes();
    let mut output = String::new();
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];

        if byte == b'%' && i + 2 < bytes.len() && is_hex(bytes[i + 1]) && is_hex(bytes[i + 2]) {
            output.push('%');
            output.push((bytes[i + 1] as char).to_ascii_uppercase());
            output.push((bytes[i + 2] as char).to_ascii_uppercase());
            i += 3;
            continue;
        }

        let unreserved = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~');
        let reserved_byte = matches!(
            byte,
            b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&'
                | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
        );

        if unreserved || (reserved && reserved_byte) {
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

fn lookup<'a>(name: &str, vars: &'a [(&str, &str)]) -> Option<&'a str> {
    vars.iter()
        .rev()
        .find_map(|&(binding_name, value)| (binding_name == name).then_some(value))
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let expressions = validate_template(template)?;
    let mut output = String::new();
    let mut cursor = 0;

    for expression in expressions {
        output.push_str(&template[cursor..expression.start]);

        let reserved = matches!(expression.operator, Some(b'+' | b'#'));
        let mut fields = Vec::new();

        for (name, _) in &expression.variables {
            if let Some(value) = lookup(name, vars) {
                let encoded = encode_value(value, reserved);
                let field = match expression.operator {
                    Some(b';') if encoded.is_empty() => name.clone(),
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

        cursor = expression.end + 1;
    }

    output.push_str(&template[cursor..]);
    Ok(output)
}