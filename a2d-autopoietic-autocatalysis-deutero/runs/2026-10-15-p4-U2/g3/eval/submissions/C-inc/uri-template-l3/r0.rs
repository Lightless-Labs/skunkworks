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
struct Specifier {
    name: String,
    start: usize,
}

#[derive(Debug, Clone)]
struct Expression {
    operator: Option<char>,
    specs: Vec<Specifier>,
}

#[derive(Debug, Clone)]
enum Part {
    Literal(String),
    Expression(Expression),
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn parse_expression(content: &str, content_pos: usize, close_pos: usize) -> Result<Expression, Error> {
    let bytes = content.as_bytes();
    let mut operator = None;
    let mut offset = 0;

    if let Some(&first) = bytes.first() {
        let ch = first as char;
        if matches!(ch, '+' | '#' | '.' | '/' | ';' | '?' | '&') {
            operator = Some(ch);
            offset = 1;
        } else if !first.is_ascii_alphanumeric() && first != b'_' {
            return Err(Error::UnsupportedOperator {
                pos: content_pos,
            });
        }
    }

    if offset == bytes.len() {
        return Err(Error::EmptyExpression { pos: close_pos });
    }

    let rest = &content[offset..];
    let rest_pos = content_pos + offset;
    let mut specs = Vec::new();
    let mut seen = Vec::<String>::new();
    let mut start = 0;

    for end in rest
        .match_indices(',')
        .map(|(index, _)| index)
        .chain(std::iter::once(rest.len()))
    {
        if end == start {
            return Err(Error::EmptyVariable {
                pos: rest_pos + start,
            });
        }

        let name = &rest[start..end];
        let name_bytes = name.as_bytes();
        if !is_name_start(name_bytes[0]) {
            return Err(Error::InvalidName {
                pos: rest_pos + start,
            });
        }
        for (index, &byte) in name_bytes.iter().enumerate().skip(1) {
            if !is_name_continue(byte) {
                return Err(Error::InvalidName {
                    pos: rest_pos + start + index,
                });
            }
        }

        if seen.iter().any(|previous| previous == name) {
            return Err(Error::DuplicateVariable {
                pos: rest_pos + start,
            });
        }
        seen.push(name.to_owned());
        specs.push(Specifier {
            name: name.to_owned(),
            start: rest_pos + start,
        });
        start = end + 1;
    }

    Ok(Expression { operator, specs })
}

fn parse_template(template: &str) -> Result<Vec<Part>, Error> {
    let mut parts = Vec::new();
    let mut literal_start = 0;
    let mut cursor = 0;

    while cursor < template.len() {
        let ch = template[cursor..].chars().next().unwrap();
        if ch == '}' {
            return Err(Error::UnexpectedClose { pos: cursor });
        }
        if ch != '{' {
            cursor += ch.len_utf8();
            continue;
        }

        if literal_start < cursor {
            parts.push(Part::Literal(template[literal_start..cursor].to_owned()));
        }

        let open_pos = cursor;
        let after_open = cursor + 1;
        let Some(relative_close) = template[after_open..].find('}') else {
            return Err(Error::Unclosed { pos: open_pos });
        };
        let close_pos = after_open + relative_close;
        if let Some(relative_open) = template[after_open..close_pos].find('{') {
            return Err(Error::NestedOpen {
                pos: after_open + relative_open,
            });
        }

        let expression = parse_expression(
            &template[after_open..close_pos],
            after_open,
            close_pos,
        )?;
        parts.push(Part::Expression(expression));
        cursor = close_pos + 1;
        literal_start = cursor;
    }

    if literal_start < template.len() {
        parts.push(Part::Literal(template[literal_start..].to_owned()));
    }
    Ok(parts)
}

fn lookup<'a>(name: &str, vars: &'a [(&str, &str)]) -> Option<&'a str> {
    vars.iter()
        .rev()
        .find(|(binding_name, _)| *binding_name == name)
        .map(|(_, value)| *value)
}

fn is_reserved(byte: u8) -> bool {
    matches!(
        byte,
        b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&' | b'\''
            | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn hex_digit(value: u8) -> u8 {
    if value < 10 {
        b'0' + value
    } else {
        b'A' + value - 10
    }
}

fn encode(value: &str, allow_reserved: bool) -> String {
    let bytes = value.as_bytes();
    let mut output = String::new();
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];

        if byte == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                output.push('%');
                output.push(hex_digit(high) as char);
                output.push(hex_digit(low) as char);
                index += 3;
                continue;
            }
        }

        if byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'.' | b'_' | b'~')
            || (allow_reserved && is_reserved(byte))
        {
            output.push(byte as char);
        } else {
            output.push('%');
            output.push(hex_digit(byte >> 4) as char);
            output.push(hex_digit(byte & 0x0f) as char);
        }
        index += 1;
    }

    output
}

fn expand_expression(expression: &Expression, vars: &[(&str, &str)]) -> String {
    let operator = expression.operator;
    let reserved = matches!(operator, Some('+' | '#'));
    let mut fields = Vec::new();

    for spec in &expression.specs {
        if let Some(value) = lookup(&spec.name, vars) {
            let encoded = encode(value, reserved);
            let field = match operator {
                Some(';' | '?' | '&') if operator == Some(';') && encoded.is_empty() => {
                    spec.name.clone()
                }
                Some(';' | '?' | '&') => format!("{}={}", spec.name, encoded),
                _ => encoded,
            };
            fields.push(field);
        }
    }

    if fields.is_empty() {
        return String::new();
    }

    let (prefix, separator) = match operator {
        Some('#') => ("#", ","),
        Some('.') => (".", "."),
        Some('/') => ("/", "/"),
        Some(';') => (";", ";"),
        Some('?') => ("?", "&"),
        Some('&') => ("&", "&"),
        _ => ("", ","),
    };

    let mut output = String::from(prefix);
    output.push_str(&fields.join(separator));
    output
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let parts = parse_template(template)?;
    let mut output = String::new();

    for part in parts {
        match part {
            Part::Literal(text) => output.push_str(&text),
            Part::Expression(expression) => {
                output.push_str(&expand_expression(&expression, vars));
            }
        }
    }

    Ok(output)
}

fn main() {}