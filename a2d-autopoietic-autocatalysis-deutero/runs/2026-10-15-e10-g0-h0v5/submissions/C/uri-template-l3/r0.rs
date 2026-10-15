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
enum Operator {
    Simple,
    Reserved,
    Fragment,
    Dot,
    Slash,
    Semicolon,
    Query,
    Continuation,
}

struct Specifier<'a> {
    name: &'a str,
    pos: usize,
    operator: Operator,
}

struct Expression<'a> {
    specifiers: Vec<Specifier<'a>>,
    end: usize,
}

fn parse_expression<'a>(template: &'a str, open: usize) -> Result<Expression<'a>, Error> {
    let bytes = template.as_bytes();
    let mut close = open + 1;

    while close < bytes.len() && bytes[close] != b'}' {
        close += 1;
    }

    if close == bytes.len() {
        return Err(Error::Unclosed { pos: open });
    }

    if let Some(offset) = bytes[open + 1..close].iter().position(|&b| b == b'{') {
        return Err(Error::NestedOpen {
            pos: open + 1 + offset,
        });
    }

    let mut content_start = open + 1;
    let mut operator = Operator::Simple;

    if content_start < close {
        operator = match bytes[content_start] {
            b'+' => Operator::Reserved,
            b'#' => Operator::Fragment,
            b'.' => Operator::Dot,
            b'/' => Operator::Slash,
            b';' => Operator::Semicolon,
            b'?' => Operator::Query,
            b'&' => Operator::Continuation,
            b'=' | b',' | b'!' | b'@' | b'|' => {
                return Err(Error::UnsupportedOperator { pos: content_start });
            }
            _ => Operator::Simple,
        };

        if !matches!(operator, Operator::Simple) {
            content_start += 1;
        }
    }

    if content_start == close {
        return Err(Error::EmptyExpression { pos: close });
    }

    let mut specifiers = Vec::new();
    let mut segment_start = content_start;

    loop {
        let segment_end = bytes[segment_start..close]
            .iter()
            .position(|&b| b == b',')
            .map(|offset| segment_start + offset)
            .unwrap_or(close);

        if segment_start == segment_end {
            return Err(Error::EmptyVariable { pos: segment_start });
        }

        let name_bytes = &bytes[segment_start..segment_end];
        for (index, &byte) in name_bytes.iter().enumerate() {
            let valid = if index == 0 {
                byte.is_ascii_alphabetic() || byte == b'_'
            } else {
                byte.is_ascii_alphanumeric() || byte == b'_'
            };
            if !valid {
                return Err(Error::InvalidName {
                    pos: segment_start + index,
                });
            }
        }

        let name = &template[segment_start..segment_end];
        if specifiers.iter().any(|s: &Specifier<'_>| s.name == name) {
            return Err(Error::DuplicateVariable { pos: segment_start });
        }

        specifiers.push(Specifier {
            name,
            pos: segment_start,
            operator,
        });

        if segment_end == close {
            break;
        }
        segment_start = segment_end + 1;
    }

    Ok(Expression {
        specifiers,
        end: close,
    })
}

fn encode(value: &str, reserved: bool) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = value.as_bytes();
    let mut result = String::new();
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];

        if byte == b'%'
            && index + 2 < bytes.len()
            && bytes[index + 1].is_ascii_hexdigit()
            && bytes[index + 2].is_ascii_hexdigit()
        {
            result.push('%');
            result.push((bytes[index + 1] as char).to_ascii_uppercase());
            result.push((bytes[index + 2] as char).to_ascii_uppercase());
            index += 3;
            continue;
        }

        let unreserved = byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'.' | b'_' | b'~');
        let reserved_byte = reserved
            && matches!(
                byte,
                b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&'
                    | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
            );

        if unreserved || reserved_byte {
            result.push(byte as char);
        } else {
            result.push('%');
            result.push(HEX[(byte >> 4) as usize] as char);
            result.push(HEX[(byte & 0x0f) as usize] as char);
        }
        index += 1;
    }

    result
}

fn lookup<'a>(vars: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    vars.iter()
        .rev()
        .find(|(binding, _)| *binding == name)
        .map(|(_, value)| *value)
}

fn render_expression(expression: &Expression<'_>, vars: &[(&str, &str)]) -> String {
    let operator = expression.specifiers[0].operator;
    let reserved = matches!(operator, Operator::Reserved | Operator::Fragment);
    let mut fields = Vec::new();

    for specifier in &expression.specifiers {
        let _ = specifier.pos;
        if let Some(value) = lookup(vars, specifier.name) {
            let encoded = encode(value, reserved);
            let field = match operator {
                Operator::Semicolon if encoded.is_empty() => specifier.name.to_owned(),
                Operator::Semicolon | Operator::Query | Operator::Continuation => {
                    format!("{}={}", specifier.name, encoded)
                }
                _ => encoded,
            };
            fields.push(field);
        }
    }

    if fields.is_empty() {
        return String::new();
    }

    let (prefix, separator) = match operator {
        Operator::Simple | Operator::Reserved => ("", ","),
        Operator::Fragment => ("#", ","),
        Operator::Dot => (".", "."),
        Operator::Slash => ("/", "/"),
        Operator::Semicolon => (";", ";"),
        Operator::Query => ("?", "&"),
        Operator::Continuation => ("&", "&"),
    };

    let mut result = String::from(prefix);
    result.push_str(&fields.join(separator));
    result
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let bytes = template.as_bytes();
    let mut output = String::new();
    let mut cursor = 0;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'}' => return Err(Error::UnexpectedClose { pos: cursor }),
            b'{' => {
                let expression = parse_expression(template, cursor)?;
                output.push_str(&template[cursor..=expression.end]);
                let rendered = render_expression(&expression, vars);
                output.truncate(output.len() - (expression.end + 1 - cursor));
                output.push_str(&rendered);
                cursor = expression.end + 1;
            }
            _ => {
                let next = bytes[cursor..]
                    .iter()
                    .position(|&b| b == b'{' || b == b'}')
                    .map(|offset| cursor + offset)
                    .unwrap_or(bytes.len());
                output.push_str(&template[cursor..next]);
                cursor = next;
            }
        }
    }

    Ok(output)
}

fn main() {}