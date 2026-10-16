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
    InvalidModifier { pos: usize },
    TypeMismatch { pos: usize },
    DuplicateKey { pos: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Scalar(String),
    List(Vec<String>),
    Map(Vec<(String, String)>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Operator {
    Simple,
    Plus,
    Fragment,
    Dot,
    Slash,
    Semicolon,
    Query,
    Continuation,
}

impl Operator {
    fn reserved(self) -> bool {
        matches!(self, Self::Plus | Self::Fragment)
    }

    fn separator(self) -> char {
        match self {
            Self::Dot => '.',
            Self::Slash => '/',
            Self::Semicolon => ';',
            Self::Query | Self::Continuation => '&',
            _ => ',',
        }
    }

    fn prefix(self) -> Option<char> {
        match self {
            Self::Fragment => Some('#'),
            Self::Dot => Some('.'),
            Self::Slash => Some('/'),
            Self::Semicolon => Some(';'),
            Self::Query => Some('?'),
            Self::Continuation => Some('&'),
            _ => None,
        }
    }

    fn named(self) -> bool {
        matches!(self, Self::Semicolon | Self::Query | Self::Continuation)
    }
}

struct Specifier<'a> {
    name: &'a str,
    explode: bool,
    prefix: Option<usize>,
    start: usize,
}

fn is_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_name_continue(b: u8) -> bool {
    is_name_start(b) || b.is_ascii_digit()
}

fn parse_specifier<'a>(raw: &'a str, start: usize) -> Result<Specifier<'a>, Error> {
    if raw.is_empty() {
        return Err(Error::EmptyVariable { pos: start });
    }

    let bytes = raw.as_bytes();
    let split = bytes.iter().position(|&b| b == b'*' || b == b':').unwrap_or(bytes.len());
    let name_bytes = &bytes[..split];

    if name_bytes.is_empty() || !is_name_start(name_bytes[0]) {
        return Err(Error::InvalidName { pos: start });
    }

    for (i, &b) in name_bytes.iter().enumerate().skip(1) {
        if !is_name_continue(b) {
            return Err(Error::InvalidName { pos: start + i });
        }
    }

    let suffix = &raw[split..];
    let (explode, prefix) = if suffix.is_empty() {
        (false, None)
    } else if suffix == "*" {
        (true, None)
    } else if suffix.as_bytes().first() == Some(&b':') {
        let digits = &suffix.as_bytes()[1..];
        let valid = (1..=4).contains(&digits.len())
            && digits[0].is_ascii_digit()
            && digits[0] != b'0'
            && digits.iter().all(u8::is_ascii_digit);
        if !valid {
            return Err(Error::InvalidModifier { pos: start + split });
        }
        let n = digits.iter().fold(0usize, |n, b| n * 10 + (b - b'0') as usize);
        if !(1..=9999).contains(&n) {
            return Err(Error::InvalidModifier { pos: start + split });
        }
        (false, Some(n))
    } else {
        return Err(Error::InvalidModifier { pos: start + split });
    };

    Ok(Specifier {
        name: &raw[..split],
        explode,
        prefix,
        start,
    })
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn encode(input: &str, reserved: bool) -> String {
    let bytes = input.as_bytes();
    let mut out = String::new();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' && i + 2 < bytes.len() {
            if let (Some(h1), Some(h2)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                out.push('%');
                out.push(char::from(b"0123456789ABCDEF"[h1 as usize]));
                out.push(char::from(b"0123456789ABCDEF"[h2 as usize]));
                i += 3;
                continue;
            }
        }

        let unreserved = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~');
        let reserved_byte = reserved
            && matches!(
                b,
                b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&'
                    | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
            );

        if unreserved || reserved_byte {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push(char::from(b"0123456789ABCDEF"[(b >> 4) as usize]));
            out.push(char::from(b"0123456789ABCDEF"[(b & 0x0f) as usize]));
        }
        i += 1;
    }

    out
}

fn selected_value<'a>(name: &str, vars: &'a [(&str, Value)]) -> Option<&'a Value> {
    vars.iter()
        .rev()
        .find(|(binding, _)| *binding == name)
        .map(|(_, value)| value)
}

fn scalar_prefix(value: &str, prefix: Option<usize>) -> String {
    match prefix {
        Some(n) => value.chars().take(n).collect(),
        None => value.to_owned(),
    }
}

fn named_field(op: Operator, name: &str, value: String) -> String {
    if op == Operator::Semicolon && value.is_empty() {
        name.to_owned()
    } else {
        format!("{name}={value}")
    }
}

fn expand_expression(
    op: Operator,
    specs: &[Specifier<'_>],
    vars: &[(&str, Value)],
) -> Result<String, Error> {
    let mut fields = Vec::new();

    for spec in specs {
        let Some(value) = selected_value(spec.name, vars) else {
            continue;
        };

        match value {
            Value::Scalar(s) => {
                let truncated = scalar_prefix(s, spec.prefix);
                let encoded = encode(&truncated, op.reserved());
                fields.push(if op.named() {
                    named_field(op, spec.name, encoded)
                } else {
                    encoded
                });
            }
            Value::List(items) => {
                if spec.prefix.is_some() {
                    return Err(Error::TypeMismatch { pos: spec.start });
                }
                if items.is_empty() {
                    continue;
                }

                if spec.explode {
                    for item in items {
                        let encoded = encode(item, op.reserved());
                        fields.push(if op.named() {
                            named_field(op, spec.name, encoded)
                        } else {
                            encoded
                        });
                    }
                } else {
                    let joined = items
                        .iter()
                        .map(|item| encode(item, op.reserved()))
                        .collect::<Vec<_>>()
                        .join(",");
                    fields.push(if op.named() {
                        named_field(op, spec.name, joined)
                    } else {
                        joined
                    });
                }
            }
            Value::Map(entries) => {
                if spec.prefix.is_some() {
                    return Err(Error::TypeMismatch { pos: spec.start });
                }
                if entries.is_empty() {
                    continue;
                }

                for i in 0..entries.len() {
                    if entries[..i].iter().any(|(key, _)| key == &entries[i].0) {
                        return Err(Error::DuplicateKey { pos: spec.start });
                    }
                }

                if spec.explode {
                    for (key, value) in entries {
                        let k = encode(key, op.reserved());
                        let v = encode(value, op.reserved());
                        fields.push(if op == Operator::Semicolon && v.is_empty() {
                            k
                        } else {
                            format!("{k}={v}")
                        });
                    }
                } else {
                    let components = entries
                        .iter()
                        .flat_map(|(key, value)| {
                            [encode(key, op.reserved()), encode(value, op.reserved())]
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    fields.push(if op.named() {
                        named_field(op, spec.name, components)
                    } else {
                        components
                    });
                }
            }
        }
    }

    if fields.is_empty() {
        return Ok(String::new());
    }

    let mut out = String::new();
    if let Some(prefix) = op.prefix() {
        out.push(prefix);
    }
    out.push_str(&fields.join(&op.separator().to_string()));
    Ok(out)
}

pub fn expand(template: &str, vars: &[(&str, Value)]) -> Result<String, Error> {
    let bytes = template.as_bytes();
    let mut output = String::new();
    let mut literal_start = 0;
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                output.push_str(&template[literal_start..i]);
                let open = i;
                let Some(relative_close) = bytes[i + 1..].iter().position(|&b| b == b'}') else {
                    return Err(Error::Unclosed { pos: open });
                };
                let close = i + 1 + relative_close;

                if let Some(relative_nested) = bytes[i + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: i + 1 + relative_nested,
                    });
                }

                let mut content_start = i + 1;
                let mut op = Operator::Simple;
                if content_start < close {
                    op = match bytes[content_start] {
                        b'+' => Operator::Plus,
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
                    if op != Operator::Simple {
                        content_start += 1;
                    }
                }

                if content_start == close {
                    return Err(Error::EmptyExpression { pos: close });
                }

                let content = &template[content_start..close];
                let mut specs = Vec::new();
                let mut offset = 0;
                for part in content.split(',') {
                    let spec_start = content_start + offset;
                    let spec = parse_specifier(part, spec_start)?;
                    if specs.iter().any(|prior: &Specifier<'_>| prior.name == spec.name) {
                        return Err(Error::DuplicateVariable { pos: spec_start });
                    }
                    specs.push(spec);
                    offset += part.len() + 1;
                }

                output.push_str(&expand_expression(op, &specs, vars)?);
                i = close + 1;
                literal_start = i;
            }
            _ => i += 1,
        }
    }

    output.push_str(&template[literal_start..]);
    Ok(output)
}