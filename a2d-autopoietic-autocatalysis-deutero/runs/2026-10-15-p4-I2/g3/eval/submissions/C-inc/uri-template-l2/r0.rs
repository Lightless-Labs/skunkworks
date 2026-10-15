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
struct Specifier<'a> {
    name: &'a str,
}

#[derive(Debug)]
struct Expression<'a> {
    operator: Option<char>,
    specifiers: Vec<Specifier<'a>>,
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn parse_expression<'a>(
    template: &'a str,
    open_pos: usize,
    close_pos: usize,
) -> Result<Expression<'a>, Error> {
    let content_start = open_pos + 1;
    let content = &template[content_start..close_pos];
    let bytes = content.as_bytes();

    let operator = bytes.first().copied().filter(|byte| {
        matches!(*byte, b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&')
    });

    if let Some(operator) = operator {
        if !matches!(operator, b'+' | b'#') {
            return Err(Error::UnsupportedOperator {
                pos: content_start,
            });
        }
    }

    let specifiers_start = content_start + usize::from(operator.is_some());
    if specifiers_start == close_pos {
        return Err(Error::EmptyExpression { pos: close_pos });
    }

    let mut specifiers = Vec::new();
    let mut used_names: Vec<&str> = Vec::new();
    let mut entry_start = specifiers_start;

    loop {
        let entry_end = template[entry_start..close_pos]
            .find(',')
            .map(|relative| entry_start + relative)
            .unwrap_or(close_pos);

        if entry_start == entry_end {
            return Err(Error::EmptyVariable { pos: entry_start });
        }

        let entry = &template[entry_start..entry_end];
        let entry_bytes = entry.as_bytes();

        if !is_name_start(entry_bytes[0]) {
            return Err(Error::InvalidName { pos: entry_start });
        }

        for (index, byte) in entry_bytes.iter().copied().enumerate().skip(1) {
            if !is_name_continue(byte) {
                return Err(Error::InvalidName {
                    pos: entry_start + index,
                });
            }
        }

        if used_names.iter().any(|name| *name == entry) {
            return Err(Error::DuplicateVariable { pos: entry_start });
        }

        used_names.push(entry);
        specifiers.push(Specifier { name: entry });

        if entry_end == close_pos {
            break;
        }
        entry_start = entry_end + 1;
    }

    Ok(Expression {
        operator: operator.map(char::from),
        specifiers,
    })
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

fn hex_digit(value: u8) -> u8 {
    match value {
        0..=9 => b'0' + value,
        _ => b'A' + (value - 10),
    }
}

fn encode_value(value: &str, allow_reserved: bool) -> String {
    let bytes = value.as_bytes();
    let mut output = String::with_capacity(bytes.len());

    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];

        if byte == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                output.push('%');
                output.push(char::from(hex_digit(high)));
                output.push(char::from(hex_digit(low)));
                index += 3;
                continue;
            }
        }

        if is_unreserved(byte) || (allow_reserved && is_reserved(byte)) {
            output.push(char::from(byte));
        } else {
            output.push('%');
            output.push(char::from(hex_digit(byte >> 4)));
            output.push(char::from(hex_digit(byte & 0x0f)));
        }

        index += 1;
    }

    output
}

fn expand_expression(expression: &Expression<'_>, vars: &[(&str, &str)]) -> String {
    let allow_reserved = matches!(expression.operator, Some('+' | '#'));
    let mut fields = Vec::new();

    for specifier in &expression.specifiers {
        if let Some((_, value)) = vars.iter().rev().find(|(name, _)| *name == specifier.name) {
            fields.push(encode_value(value, allow_reserved));
        }
    }

    if fields.is_empty() {
        return String::new();
    }

    let joined = fields.join(",");
    if expression.operator == Some('#') {
        format!("#{joined}")
    } else {
        joined
    }
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let mut expressions = Vec::new();
    let mut position = 0;

    while position < template.len() {
        match template.as_bytes()[position] {
            b'{' => {
                let open_pos = position;
                let remainder = &template[open_pos + 1..];
                let Some(relative_close) = remainder.find('}') else {
                    return Err(Error::Unclosed { pos: open_pos });
                };
                let close_pos = open_pos + 1 + relative_close;

                if let Some(relative_open) = remainder[..relative_close].find('{') {
                    return Err(Error::NestedOpen {
                        pos: open_pos + 1 + relative_open,
                    });
                }

                let expression = parse_expression(template, open_pos, close_pos)?;
                expressions.push((open_pos, close_pos + 1, expression));
                position = close_pos + 1;
            }
            b'}' => return Err(Error::UnexpectedClose { pos: position }),
            _ => {
                let character = template[position..]
                    .chars()
                    .next()
                    .expect("position is within the template");
                position += character.len_utf8();
            }
        }
    }

    let mut output = String::with_capacity(template.len());
    let mut literal_start = 0;

    for (start, end, expression) in expressions {
        output.push_str(&template[literal_start..start]);
        output.push_str(&expand_expression(&expression, vars));
        literal_start = end;
    }

    output.push_str(&template[literal_start..]);
    Ok(output)
}

fn main() {}