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
    let expressions = parse(template)?;
    let mut output = String::new();
    let mut cursor = 0;

    for expression in expressions {
        output.push_str(&template[cursor..expression.start]);
        append_expression(&mut output, &expression, vars);
        cursor = expression.end + 1;
    }

    output.push_str(&template[cursor..]);
    Ok(output)
}

fn parse(template: &str) -> Result<Vec<Expression<'_>>, Error> {
    let mut expressions = Vec::new();
    let mut cursor = 0;

    while cursor < template.len() {
        let ch = template[cursor..].chars().next().expect("valid UTF-8 boundary");
        match ch {
            '}' => return Err(Error::UnexpectedClose { pos: cursor }),
            '{' => {
                let start = cursor;
                let content_start = start + 1;
                let close = match template[content_start..].find('}') {
                    Some(offset) => content_start + offset,
                    None => return Err(Error::Unclosed { pos: start }),
                };

                if let Some(offset) = template[content_start..close].find('{') {
                    return Err(Error::NestedOpen {
                        pos: content_start + offset,
                    });
                }

                let content = &template[content_start..close];
                let mut body_start = content_start;
                let mut operator = None;

                if let Some(first) = content.chars().next() {
                    if matches!(first, '+' | '#' | '.' | '/' | ';' | '?' | '&') {
                        let operator_pos = content_start;
                        if !matches!(first, '+' | '#') {
                            return Err(Error::UnsupportedOperator { pos: operator_pos });
                        }
                        operator = Some(first);
                        body_start += first.len_utf8();
                    }
                }

                if body_start == close {
                    return Err(Error::EmptyExpression { pos: close });
                }

                let body = &template[body_start..close];
                let mut names = Vec::new();
                let mut seen = Vec::<&str>::new();
                let mut spec_start = body_start;

                for specifier in body.split(',') {
                    if specifier.is_empty() {
                        return Err(Error::EmptyVariable { pos: spec_start });
                    }

                    if let Some(offset) = invalid_name_offset(specifier) {
                        return Err(Error::InvalidName {
                            pos: spec_start + offset,
                        });
                    }

                    if seen.iter().any(|name| *name == specifier) {
                        return Err(Error::DuplicateVariable { pos: spec_start });
                    }

                    seen.push(specifier);
                    names.push((specifier, spec_start));
                    spec_start += specifier.len() + 1;
                }

                expressions.push(Expression {
                    start,
                    end: close,
                    operator,
                    names,
                });
                cursor = close + 1;
            }
            _ => cursor += ch.len_utf8(),
        }
    }

    Ok(expressions)
}

fn invalid_name_offset(name: &str) -> Option<usize> {
    let mut bytes = name.bytes();
    let first = bytes.next()?;

    if !first.is_ascii_alphabetic() && first != b'_' {
        return Some(0);
    }

    for (offset, byte) in name.bytes().enumerate().skip(1) {
        if !byte.is_ascii_alphanumeric() && byte != b'_' {
            return Some(offset);
        }
    }

    None
}

fn append_expression(output: &mut String, expression: &Expression<'_>, vars: &[(&str, &str)]) {
    let mut fields = Vec::new();

    for (name, _) in &expression.names {
        if let Some((_, value)) = vars.iter().rev().find(|(binding, _)| binding == name) {
            fields.push(encode_value(value, matches!(expression.operator, Some('+' | '#'))));
        }
    }

    if fields.is_empty() {
        return;
    }

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

fn encode_value(value: &str, allow_reserved: bool) -> String {
    let mut encoded = String::new();
    let bytes = value.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];

        if byte == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex_value(bytes[index + 1]), hex_value(bytes[index + 2])) {
                encoded.push('%');
                encoded.push(hex_digit(high));
                encoded.push(hex_digit(low));
                index += 3;
                continue;
            }
        }

        if is_unreserved(byte) || (allow_reserved && is_reserved(byte)) {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push(hex_digit(byte >> 4));
            encoded.push(hex_digit(byte & 0x0f));
        }
        index += 1;
    }

    encoded
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
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

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'A' + value - 10) as char,
    }
}

fn main() {}