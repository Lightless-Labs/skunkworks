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

#[derive(Debug, Clone, Copy)]
enum Operator {
    Simple,
    Reserved,
    Fragment,
    Label,
    Path,
    Matrix,
    Query,
    Continuation,
}

#[derive(Debug)]
struct Specifier<'a> {
    name: &'a str,
    start: usize,
}

#[derive(Debug)]
struct Expression<'a> {
    operator: Operator,
    specifiers: Vec<Specifier<'a>>,
}

fn is_supported_operator(byte: u8) -> bool {
    matches!(byte, b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&')
}

fn is_operator_byte(byte: u8) -> bool {
    matches!(byte, b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&')
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn parse_expression<'a>(template: &'a str, open: usize, close: usize) -> Result<Expression<'a>, Error> {
    let bytes = template.as_bytes();
    let mut content_start = open + 1;

    let operator = if content_start < close && is_operator_byte(bytes[content_start]) {
        let byte = bytes[content_start];
        if !is_supported_operator(byte) {
            return Err(Error::UnsupportedOperator { pos: content_start });
        }
        content_start += 1;
        match byte {
            b'+' => Operator::Reserved,
            b'#' => Operator::Fragment,
            b'.' => Operator::Label,
            b'/' => Operator::Path,
            b';' => Operator::Matrix,
            b'?' => Operator::Query,
            b'&' => Operator::Continuation,
            _ => unreachable!(),
        }
    } else {
        Operator::Simple
    };

    if content_start == close {
        return Err(Error::EmptyExpression { pos: close });
    }

    let mut specifiers = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    let mut start = content_start;

    loop {
        let mut end = start;
        while end < close && bytes[end] != b',' {
            end += 1;
        }

        if end == start {
            return Err(Error::EmptyVariable { pos: start });
        }

        let name_bytes = &bytes[start..end];
        if !is_name_start(name_bytes[0]) {
            return Err(Error::InvalidName { pos: start });
        }
        for (offset, &byte) in name_bytes.iter().enumerate().skip(1) {
            if !is_name_continue(byte) {
                return Err(Error::InvalidName { pos: start + offset });
            }
        }

        let name = &template[start..end];
        if seen.contains(&name) {
            return Err(Error::DuplicateVariable { pos: start });
        }
        seen.push(name);
        specifiers.push(Specifier { name, start });

        if end == close {
            break;
        }
        start = end + 1;
    }

    Ok(Expression {
        operator,
        specifiers,
    })
}

fn validate_template(template: &str) -> Result<Vec<(usize, usize, Expression<'_>)>, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'}' => return Err(Error::UnexpectedClose { pos: index }),
            b'{' => {
                let open = index;
                let mut close = open + 1;
                while close < bytes.len() && bytes[close] != b'}' {
                    close += 1;
                }
                if close == bytes.len() {
                    return Err(Error::Unclosed { pos: open });
                }
                if let Some(inner) = (open + 1..close).find(|&i| bytes[i] == b'{') {
                    return Err(Error::NestedOpen { pos: inner });
                }
                let expression = parse_expression(template, open, close)?;
                expressions.push((open, close + 1, expression));
                index = close + 1;
            }
            _ => index += 1,
        }
    }

    Ok(expressions)
}

fn lookup<'a>(name: &str, vars: &'a [(&str, &str)]) -> Option<&'a str> {
    vars.iter()
        .rev()
        .find(|(binding_name, _)| *binding_name == name)
        .map(|(_, value)| *value)
}

fn is_hex(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

fn hex_value(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte,
        b'a'..=b'f' => byte - b'a' + b'A',
        _ => byte,
    }
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

fn encode(value: &str, allow_reserved: bool, out: &mut String) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = value.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%' && index + 2 < bytes.len() && is_hex(bytes[index + 1]) && is_hex(bytes[index + 2]) {
            out.push('%');
            out.push(hex_value(bytes[index + 1]) as char);
            out.push(hex_value(bytes[index + 2]) as char);
            index += 3;
        } else if is_unreserved(byte) || (allow_reserved && is_reserved(byte)) {
            out.push(byte as char);
            index += 1;
        } else {
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
            index += 1;
        }
    }
}

fn expand_expression(expression: &Expression<'_>, vars: &[(&str, &str)], out: &mut String) {
    let mut fields: Vec<(String, Option<&str>)> = Vec::new();

    for specifier in &expression.specifiers {
        if let Some(value) = lookup(specifier.name, vars) {
            fields.push((specifier.name.to_owned(), Some(value)));
        }
    }

    if fields.is_empty() {
        return;
    }

    let (separator, prefix, allow_reserved) = match expression.operator {
        Operator::Simple => (",", "", false),
        Operator::Reserved => (",", "", true),
        Operator::Fragment => (",", "#", true),
        Operator::Label => (".", ".", false),
        Operator::Path => ("/", "/", false),
        Operator::Matrix => (";", ";", false),
        Operator::Query => ("&", "?", false),
        Operator::Continuation => ("&", "&", false),
    };
    out.push_str(prefix);

    for (index, (name, value)) in fields.iter().enumerate() {
        if index > 0 {
            out.push_str(separator);
        }

        match expression.operator {
            Operator::Matrix => {
                out.push_str(name);
                let value = value.expect("present fields have values");
                if !value.is_empty() {
                    out.push('=');
                    encode(value, allow_reserved, out);
                }
            }
            Operator::Query | Operator::Continuation => {
                out.push_str(name);
                out.push('=');
                encode(value.expect("present fields have values"), allow_reserved, out);
            }
            _ => encode(value.expect("present fields have values"), allow_reserved, out),
        }
    }
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let expressions = validate_template(template)?;
    let mut output = String::new();
    let mut cursor = 0;

    for (open, after_close, expression) in &expressions {
        output.push_str(&template[cursor..*open]);
        expand_expression(expression, vars, &mut output);
        cursor = *after_close;
    }
    output.push_str(&template[cursor..]);

    Ok(output)
}

fn main() {}