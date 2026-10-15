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

struct Expression {
    start: usize,
    end: usize,
    operator: Operator,
    names: Vec<(usize, usize)>,
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                let open = i;
                let Some(relative_close) = bytes[i + 1..].iter().position(|&b| b == b'}') else {
                    return Err(Error::Unclosed { pos: open });
                };
                let close = i + 1 + relative_close;

                if let Some(relative_open) = bytes[open + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: open + 1 + relative_open,
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
                        _ => {
                            content_start = content_start;
                            Operator::Simple
                        }
                    };
                    if !matches!(operator, Operator::Simple) {
                        content_start += 1;
                    }
                }

                if content_start == close {
                    return Err(Error::EmptyExpression { pos: close });
                }

                let mut names = Vec::new();
                let mut seen: Vec<&str> = Vec::new();
                let mut spec_start = content_start;

                loop {
                    let spec_end = bytes[spec_start..close]
                        .iter()
                        .position(|&b| b == b',')
                        .map(|offset| spec_start + offset)
                        .unwrap_or(close);

                    if spec_start == spec_end {
                        return Err(Error::EmptyVariable { pos: spec_start });
                    }

                    let spec = &template[spec_start..spec_end];
                    let spec_bytes = spec.as_bytes();
                    let valid_first = |b: u8| b.is_ascii_alphabetic() || b == b'_';

                    if !valid_first(spec_bytes[0]) {
                        return Err(Error::InvalidName { pos: spec_start });
                    }
                    for (offset, &b) in spec_bytes.iter().enumerate().skip(1) {
                        if !(b.is_ascii_alphanumeric() || b == b'_') {
                            return Err(Error::InvalidName {
                                pos: spec_start + offset,
                            });
                        }
                    }

                    if seen.iter().any(|&name| name == spec) {
                        return Err(Error::DuplicateVariable { pos: spec_start });
                    }
                    seen.push(spec);
                    names.push((spec_start, spec_end));

                    if spec_end == close {
                        break;
                    }
                    spec_start = spec_end + 1;
                }

                expressions.push(Expression {
                    start: open,
                    end: close + 1,
                    operator,
                    names,
                });
                i = close + 1;
            }
            _ => i += 1,
        }
    }

    let mut output = String::with_capacity(template.len());
    let mut literal_start = 0;

    for expression in expressions {
        output.push_str(&template[literal_start..expression.start]);

        let mut fields = Vec::new();
        for (name_start, name_end) in expression.names {
            let name = &template[name_start..name_end];
            let value = vars
                .iter()
                .rev()
                .find(|&&(binding_name, _)| binding_name == name)
                .map(|&(_, value)| value);

            if let Some(value) = value {
                let encoded = encode(value, matches!(expression.operator, Operator::Reserved | Operator::Fragment));
                match expression.operator {
                    Operator::Semicolon if encoded.is_empty() => fields.push(name.to_owned()),
                    Operator::Semicolon | Operator::Query | Operator::Continuation => {
                        fields.push(format!("{name}={encoded}"));
                    }
                    _ => fields.push(encoded),
                }
            }
        }

        if !fields.is_empty() {
            let (prefix, separator) = match expression.operator {
                Operator::Simple | Operator::Reserved => ("", ","),
                Operator::Fragment => ("#", ","),
                Operator::Dot => (".", "."),
                Operator::Slash => ("/", "/"),
                Operator::Semicolon => (";", ";"),
                Operator::Query => ("?", "&"),
                Operator::Continuation => ("&", "&"),
            };
            output.push_str(prefix);
            output.push_str(&fields.join(separator));
        }

        literal_start = expression.end;
    }

    output.push_str(&template[literal_start..]);
    Ok(output)
}

fn encode(value: &str, allow_reserved: bool) -> String {
    let bytes = value.as_bytes();
    let mut output = String::with_capacity(bytes.len());

    for i in 0..bytes.len() {
        let byte = bytes[i];

        if byte == b'%' && i + 2 < bytes.len() && is_hex(bytes[i + 1]) && is_hex(bytes[i + 2]) {
            output.push('%');
            output.push((bytes[i + 1] as char).to_ascii_uppercase());
            output.push((bytes[i + 2] as char).to_ascii_uppercase());
            continue;
        }

        if is_unreserved(byte) || (allow_reserved && is_reserved(byte)) {
            output.push(byte as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }

    output
}

fn is_hex(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
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

fn main() {}