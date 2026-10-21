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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug)]
struct Expression<'a> {
    operator: Operator,
    specs: Vec<(&'a str, usize)>,
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_byte(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn parse_expression<'a>(
    template: &'a str,
    open: usize,
    close: usize,
) -> Result<Expression<'a>, Error> {
    let bytes = template.as_bytes();
    let mut start = open + 1;
    let mut operator = Operator::Simple;

    if start < close {
        operator = match bytes[start] {
            b'+' => Operator::Reserved,
            b'#' => Operator::Fragment,
            b'.' => Operator::Dot,
            b'/' => Operator::Slash,
            b';' => Operator::Semicolon,
            b'?' => Operator::Query,
            b'&' => Operator::Continuation,
            _ => Operator::Simple,
        };
        if operator != Operator::Simple {
            start += 1;
        }
    }

    if start == close {
        return Err(Error::EmptyExpression { pos: close });
    }

    let mut specs = Vec::new();
    let mut spec_start = start;
    while spec_start <= close {
        let spec_end = bytes[spec_start..close]
            .iter()
            .position(|&b| b == b',')
            .map(|offset| spec_start + offset)
            .unwrap_or(close);

        if spec_start == spec_end {
            return Err(Error::EmptyVariable { pos: spec_start });
        }

        let name = &template[spec_start..spec_end];
        let name_bytes = name.as_bytes();
        if !is_name_start(name_bytes[0]) {
            return Err(Error::InvalidName { pos: spec_start });
        }
        for (offset, &byte) in name_bytes.iter().enumerate().skip(1) {
            if !is_name_byte(byte) {
                return Err(Error::InvalidName {
                    pos: spec_start + offset,
                });
            }
        }

        if specs.iter().any(|(seen, _)| *seen == name) {
            return Err(Error::DuplicateVariable { pos: spec_start });
        }
        specs.push((name, spec_start));

        if spec_end == close {
            break;
        }
        spec_start = spec_end + 1;
    }

    Ok(Expression { operator, specs })
}

fn encode_value(value: &str, reserved: bool) -> String {
    let bytes = value.as_bytes();
    let mut out = String::new();
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'%' && i + 2 < bytes.len() {
            let a = bytes[i + 1];
            let b = bytes[i + 2];
            if a.is_ascii_hexdigit() && b.is_ascii_hexdigit() {
                out.push('%');
                out.push((a as char).to_ascii_uppercase());
                out.push((b as char).to_ascii_uppercase());
                i += 3;
                continue;
            }
        }

        let unreserved =
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~');
        let reserved_byte = matches!(
            byte,
            b':' | b'/'
                | b'?'
                | b'#'
                | b'['
                | b']'
                | b'@'
                | b'!'
                | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
        );

        if unreserved || (reserved && reserved_byte) {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(char::from_digit((byte >> 4) as u32, 16).unwrap().to_ascii_uppercase());
            out.push(char::from_digit((byte & 0x0f) as u32, 16).unwrap().to_ascii_uppercase());
        }
        i += 1;
    }

    out
}

fn expand_expression(expr: &Expression<'_>, vars: &[(&str, &str)]) -> String {
    let mut fields = Vec::new();

    for &(name, _) in &expr.specs {
        if let Some((_, value)) = vars.iter().rev().find(|(binding, _)| *binding == name) {
            let encoded = encode_value(
                value,
                matches!(expr.operator, Operator::Reserved | Operator::Fragment),
            );
            fields.push((name, encoded));
        }
    }

    if fields.is_empty() {
        return String::new();
    }

    match expr.operator {
        Operator::Simple | Operator::Reserved | Operator::Fragment => {
            let mut out = String::new();
            if expr.operator == Operator::Fragment {
                out.push('#');
            }
            for (index, (_, value)) in fields.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(value);
            }
            out
        }
        Operator::Dot | Operator::Slash => {
            let separator = if expr.operator == Operator::Dot {
                '.'
            } else {
                '/'
            };
            let mut out = String::new();
            out.push(separator);
            for (index, (_, value)) in fields.iter().enumerate() {
                if index > 0 {
                    out.push(separator);
                }
                out.push_str(value);
            }
            out
        }
        Operator::Semicolon | Operator::Query | Operator::Continuation => {
            let (prefix, separator) = match expr.operator {
                Operator::Semicolon => (';', ';'),
                Operator::Query => ('?', '&'),
                Operator::Continuation => ('&', '&'),
                _ => unreachable!(),
            };
            let mut out = String::new();
            out.push(prefix);
            for (index, (name, value)) in fields.iter().enumerate() {
                if index > 0 {
                    out.push(separator);
                }
                out.push_str(name);
                if expr.operator != Operator::Semicolon || !value.is_empty() {
                    out.push('=');
                    out.push_str(value);
                }
            }
            out
        }
    }
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut cursor = 0;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'}' => return Err(Error::UnexpectedClose { pos: cursor }),
            b'{' => {
                let open = cursor;
                let close = match bytes[open + 1..]
                    .iter()
                    .position(|&byte| byte == b'}')
                {
                    Some(offset) => open + 1 + offset,
                    None => return Err(Error::Unclosed { pos: open }),
                };

                if let Some(offset) = bytes[open + 1..close]
                    .iter()
                    .position(|&byte| byte == b'{')
                {
                    return Err(Error::NestedOpen {
                        pos: open + 1 + offset,
                    });
                }

                let expression = parse_expression(template, open, close)?;
                expressions.push((open, close, expression));
                cursor = close + 1;
            }
            _ => cursor += 1,
        }
    }

    let mut out = String::new();
    let mut literal_start = 0;
    for (open, close, expression) in expressions {
        out.push_str(&template[literal_start..open]);
        out.push_str(&expand_expression(&expression, vars));
        literal_start = close + 1;
    }
    out.push_str(&template[literal_start..]);
    Ok(out)
}