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
    start: usize,
}

#[derive(Debug)]
struct Expression<'a> {
    operator: Option<char>,
    operator_pos: Option<usize>,
    specifiers: Vec<Specifier<'a>>,
}

#[derive(Debug)]
enum Part<'a> {
    Literal(&'a str),
    Expression(Expression<'a>),
}

fn parse_expression<'a>(template: &'a str, open: usize, close: usize) -> Result<Expression<'a>, Error> {
    let content_start = open + 1;
    let mut operator = None;
    let mut operator_pos = None;
    let mut spec_start = content_start;

    if content_start < close {
        let ch = template[content_start..close].chars().next().unwrap();
        if matches!(ch, '+' | '#' | '.' | '/' | ';' | '?' | '&') {
            operator = Some(ch);
            operator_pos = Some(content_start);
            spec_start += ch.len_utf8();
            if !matches!(ch, '+' | '#') {
                return Err(Error::UnsupportedOperator { pos: content_start });
            }
        }
    }

    if spec_start == close {
        return Err(Error::EmptyExpression { pos: close });
    }

    let mut specifiers = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    let mut cursor = spec_start;

    loop {
        let comma = template[cursor..close]
            .find(',')
            .map(|offset| cursor + offset)
            .unwrap_or(close);

        if comma == cursor {
            return Err(Error::EmptyVariable { pos: cursor });
        }

        let name = &template[cursor..comma];
        let bytes = name.as_bytes();
        let valid_first = |b: u8| b.is_ascii_alphabetic() || b == b'_';
        let valid_rest = |b: u8| valid_first(b) || b.is_ascii_digit();

        if !valid_first(bytes[0]) {
            return Err(Error::InvalidName { pos: cursor });
        }
        if let Some(offset) = bytes.iter().enumerate().skip(1).find_map(|(i, &b)| {
            if valid_rest(b) { None } else { Some(i) }
        }) {
            return Err(Error::InvalidName { pos: cursor + offset });
        }

        if seen.iter().any(|&prior| prior == name) {
            return Err(Error::DuplicateVariable { pos: cursor });
        }
        seen.push(name);
        specifiers.push(Specifier { name, start: cursor });

        if comma == close {
            break;
        }
        cursor = comma + 1;
    }

    Ok(Expression {
        operator,
        operator_pos,
        specifiers,
    })
}

fn parse_template(template: &str) -> Result<Vec<Part<'_>>, Error> {
    let mut parts = Vec::new();
    let mut cursor = 0;

    while cursor < template.len() {
        match template.as_bytes()[cursor] {
            b'}' => return Err(Error::UnexpectedClose { pos: cursor }),
            b'{' => {
                let open = cursor;
                let Some(relative_close) = template[cursor + 1..].find('}') else {
                    return Err(Error::Unclosed { pos: open });
                };
                let close = cursor + 1 + relative_close;

                if let Some(relative_nested) = template[cursor + 1..close].find('{') {
                    return Err(Error::NestedOpen {
                        pos: cursor + 1 + relative_nested,
                    });
                }

                if open > 0 {
                    // Literal text before the opening brace is emitted as its own span.
                    // The cursor always points at a character boundary.
                }
                let parsed = parse_expression(template, open, close)?;
                parts.push(Part::Expression(parsed));
                cursor = close + 1;
            }
            _ => {
                let next = template[cursor..].chars().next().unwrap();
                let start = cursor;
                cursor += next.len_utf8();

                while cursor < template.len()
                    && template.as_bytes()[cursor] != b'{'
                    && template.as_bytes()[cursor] != b'}'
                {
                    let ch = template[cursor..].chars().next().unwrap();
                    cursor += ch.len_utf8();
                }
                parts.push(Part::Literal(&template[start..cursor]));
            }
        }
    }

    Ok(parts)
}

fn encode_value(value: &str, reserved: bool) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    let bytes = value.as_bytes();
    let mut output = String::new();
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
            continue;
        }

        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
            || (reserved && matches!(byte, b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@'
                | b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='))
        {
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

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let parts = parse_template(template)?;
    let mut output = String::new();

    for part in parts {
        match part {
            Part::Literal(text) => output.push_str(text),
            Part::Expression(expression) => {
                let reserved = matches!(expression.operator, Some('+' | '#'));
                let mut fields = Vec::new();

                for specifier in expression.specifiers {
                    let _source_pos = specifier.start;
                    if let Some((_, value)) = vars.iter().rev().find(|(name, _)| *name == specifier.name) {
                        fields.push(encode_value(value, reserved));
                    }
                }

                if !fields.is_empty() {
                    if expression.operator == Some('#') {
                        output.push('#');
                    }
                    output.push_str(&fields.join(","));
                } else if expression.operator_pos.is_some() {
                    // An expression without contributed fields emits nothing.
                }
            }
        }
    }

    Ok(output)
}

fn main() {}