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
    Dot,
    Slash,
    Semicolon,
    Query,
    Continuation,
}

#[derive(Debug)]
struct Expression {
    start: usize,
    end: usize,
    operator: Operator,
    names: Vec<(String, usize)>,
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                let start = i;
                let Some(relative_end) = bytes[i + 1..].iter().position(|&b| b == b'}') else {
                    return Err(Error::Unclosed { pos: start });
                };
                let end = i + 1 + relative_end;

                if let Some(relative_open) = bytes[i + 1..end].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: i + 1 + relative_open,
                    });
                }

                let content_start = i + 1;
                let mut spec_start = content_start;
                let mut operator = Operator::Simple;

                if content_start < end {
                    operator = match bytes[content_start] {
                        b'+' => Operator::Reserved,
                        b'#' => Operator::Fragment,
                        b'.' => Operator::Dot,
                        b'/' => Operator::Slash,
                        b';' => Operator::Semicolon,
                        b'?' => Operator::Query,
                        b'&' => Operator::Continuation,
                        _ => Operator::Simple,
                    };
                    if !matches!(operator, Operator::Simple) {
                        spec_start += 1;
                    }
                }

                if spec_start == end {
                    return Err(Error::EmptyExpression { pos: end });
                }

                let mut names = Vec::new();
                let mut seen = Vec::<String>::new();
                let mut part_start = spec_start;

                loop {
                    let part_end = bytes[part_start..end]
                        .iter()
                        .position(|&b| b == b',')
                        .map(|offset| part_start + offset)
                        .unwrap_or(end);

                    if part_start == part_end {
                        return Err(Error::EmptyVariable { pos: part_start });
                    }

                    let name_bytes = &bytes[part_start..part_end];
                    let valid_first = |b: u8| b.is_ascii_alphabetic() || b == b'_';
                    let valid_rest = |b: u8| b.is_ascii_alphanumeric() || b == b'_';

                    if !valid_first(name_bytes[0]) {
                        return Err(Error::InvalidName { pos: part_start });
                    }
                    if let Some(offset) = name_bytes[1..].iter().position(|&b| !valid_rest(b)) {
                        return Err(Error::InvalidName {
                            pos: part_start + 1 + offset,
                        });
                    }

                    let name = std::str::from_utf8(name_bytes).unwrap().to_owned();
                    if seen.iter().any(|existing| existing == &name) {
                        return Err(Error::DuplicateVariable { pos: part_start });
                    }
                    seen.push(name.clone());
                    names.push((name, part_start));

                    if part_end == end {
                        break;
                    }
                    part_start = part_end + 1;
                }

                expressions.push(Expression {
                    start,
                    end,
                    operator,
                    names,
                });
                i = end + 1;
            }
            _ => i += 1,
        }
    }

    let mut output = String::new();
    let mut literal_start = 0;

    for expression in expressions {
        output.push_str(&template[literal_start..expression.start]);
        let fields: Vec<(String, String)> = expression
            .names
            .iter()
            .filter_map(|(name, _)| {
                vars.iter()
                    .rev()
                    .find(|(binding, _)| binding == name)
                    .map(|(_, value)| (name.clone(), (*value).to_owned()))
            })
            .collect();

        if !fields.is_empty() {
            let (separator, prefix) = match expression.operator {
                Operator::Simple | Operator::Reserved => (",", ""),
                Operator::Fragment => (",", "#"),
                Operator::Dot => (".", "."),
                Operator::Slash => ("/", "/"),
                Operator::Semicolon => (";", ";"),
                Operator::Query => ("&", "?"),
                Operator::Continuation => ("&", "&"),
            };
            output.push_str(prefix);

            for (index, (name, value)) in fields.iter().enumerate() {
                if index > 0 {
                    output.push_str(separator);
                }
                match expression.operator {
                    Operator::Semicolon => {
                        output.push_str(name);
                        let encoded = encode_value(value, false);
                        if !encoded.is_empty() {
                            output.push('=');
                            output.push_str(&encoded);
                        }
                    }
                    Operator::Query | Operator::Continuation => {
                        output.push_str(name);
                        output.push('=');
                        output.push_str(&encode_value(value, false));
                    }
                    _ => output.push_str(&encode_value(
                        value,
                        matches!(expression.operator, Operator::Reserved | Operator::Fragment),
                    )),
                }
            }
        }

        literal_start = expression.end + 1;
    }

    output.push_str(&template[literal_start..]);
    Ok(output)
}

fn encode_value(value: &str, allow_reserved: bool) -> String {
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

        if is_unreserved(byte) || (allow_reserved && is_reserved(byte)) {
            output.push(byte as char);
        } else {
            output.push('%');
            output.push(hex_digit(byte >> 4));
            output.push(hex_digit(byte & 0x0f));
        }
        i += 1;
    }

    output
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

fn is_hex(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'A' + value - 10) as char,
    }
}

fn main() {}