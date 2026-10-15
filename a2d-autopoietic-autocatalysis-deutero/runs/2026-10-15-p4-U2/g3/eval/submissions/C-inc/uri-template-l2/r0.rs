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

#[derive(Clone, Copy)]
struct Variable<'a> {
    name: &'a str,
    start: usize,
}

struct Expression<'a> {
    operator: Option<char>,
    variables: Vec<Variable<'a>>,
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn parse_expression<'a>(
    content: &'a str,
    content_start: usize,
    close_pos: usize,
) -> Result<Expression<'a>, Error> {
    let mut offset = 0;
    let mut operator = None;

    if let Some(&first) = content.as_bytes().first() {
        if matches!(first, b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&') {
            operator = Some(first as char);
            offset = 1;
            if !matches!(first, b'+' | b'#') {
                return Err(Error::UnsupportedOperator {
                    pos: content_start,
                });
            }
        }
    }

    if offset == content.len() {
        return Err(Error::EmptyExpression { pos: close_pos });
    }

    let bytes = content.as_bytes();
    let mut variables = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    let mut entry_start = offset;

    loop {
        let entry_end = content[entry_start..]
            .find(',')
            .map(|relative| entry_start + relative)
            .unwrap_or(content.len());

        if entry_start == entry_end {
            return Err(Error::EmptyVariable {
                pos: content_start + entry_start,
            });
        }

        let name_bytes = &bytes[entry_start..entry_end];
        if !is_name_start(name_bytes[0]) {
            return Err(Error::InvalidName {
                pos: content_start + entry_start,
            });
        }
        for (index, &byte) in name_bytes.iter().enumerate().skip(1) {
            if !is_name_continue(byte) {
                return Err(Error::InvalidName {
                    pos: content_start + entry_start + index,
                });
            }
        }

        let name = &content[entry_start..entry_end];
        if seen.iter().any(|previous| *previous == name) {
            return Err(Error::DuplicateVariable {
                pos: content_start + entry_start,
            });
        }
        seen.push(name);
        variables.push(Variable {
            name,
            start: content_start + entry_start,
        });

        if entry_end == content.len() {
            break;
        }
        entry_start = entry_end + 1;
    }

    Ok(Expression {
        operator,
        variables,
    })
}

fn is_hex(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

fn hex_value(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => byte - b'A' + 10,
    }
}

fn push_percent_encoded(output: &mut String, byte: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    output.push('%');
    output.push(HEX[(byte >> 4) as usize] as char);
    output.push(HEX[(byte & 0x0f) as usize] as char);
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn is_reserved(byte: u8) -> bool {
    matches!(
        byte,
        b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&'
            | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

fn encode_value(value: &str, allow_reserved: bool) -> String {
    let bytes = value.as_bytes();
    let mut output = String::with_capacity(value.len());

    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];

        if byte == b'%'
            && index + 2 < bytes.len()
            && is_hex(bytes[index + 1])
            && is_hex(bytes[index + 2])
        {
            output.push('%');
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            output.push(HEX[hex_value(bytes[index + 1]) as usize] as char);
            output.push(HEX[hex_value(bytes[index + 2]) as usize] as char);
            index += 3;
            continue;
        }

        if is_unreserved(byte) || (allow_reserved && is_reserved(byte)) {
            output.push(byte as char);
        } else {
            push_percent_encoded(&mut output, byte);
        }
        index += 1;
    }

    output
}

fn lookup<'a>(name: &str, vars: &'a [(&str, &str)]) -> Option<&'a str> {
    vars.iter()
        .rev()
        .find(|(binding_name, _)| *binding_name == name)
        .map(|(_, value)| *value)
}

fn expand_expression(expression: &Expression<'_>, vars: &[(&str, &str)]) -> String {
    let allow_reserved = matches!(expression.operator, Some('+' | '#'));
    let mut fields = Vec::new();

    for variable in &expression.variables {
        if let Some(value) = lookup(variable.name, vars) {
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
    let mut literal_spans = Vec::new();
    let mut cursor = 0;

    while cursor < template.len() {
        let remaining = &template[cursor..];
        let next_open = remaining.find('{');
        let next_close = remaining.find('}');

        match (next_open, next_close) {
            (None, None) => {
                literal_spans.push((cursor, template.len()));
                cursor = template.len();
            }
            (None, Some(close)) => {
                return Err(Error::UnexpectedClose {
                    pos: cursor + close,
                });
            }
            (Some(open), Some(close)) if close < open => {
                return Err(Error::UnexpectedClose {
                    pos: cursor + close,
                });
            }
            (Some(open), _) => {
                let open_pos = cursor + open;
                if cursor < open_pos {
                    literal_spans.push((cursor, open_pos));
                }

                let content_start = open_pos + 1;
                let after_open = &template[content_start..];
                let close_relative = match after_open.find('}') {
                    Some(position) => position,
                    None => return Err(Error::Unclosed { pos: open_pos }),
                };
                let close_pos = content_start + close_relative;
                if let Some(inner_open) = template[content_start..close_pos].find('{') {
                    return Err(Error::NestedOpen {
                        pos: content_start + inner_open,
                    });
                }

                let content = &template[content_start..close_pos];
                let expression = parse_expression(content, content_start, close_pos)?;
                expressions.push((open_pos, close_pos + 1, expression));
                cursor = close_pos + 1;
            }
        }
    }

    if template.is_empty() {
        return Ok(String::new());
    }

    let mut output = String::with_capacity(template.len());
    let mut cursor = 0;
    let mut expression_index = 0;

    while cursor < template.len() {
        if expression_index < expressions.len()
            && expressions[expression_index].0 == cursor
        {
            let (_, end, expression) = &expressions[expression_index];
            output.push_str(&expand_expression(expression, vars));
            cursor = *end;
            expression_index += 1;
        } else {
            let next_expression = expressions
                .get(expression_index)
                .map(|(start, _, _)| *start)
                .unwrap_or(template.len());
            output.push_str(&template[cursor..next_expression]);
            cursor = next_expression;
        }
    }

    Ok(output)
}

fn main() {}