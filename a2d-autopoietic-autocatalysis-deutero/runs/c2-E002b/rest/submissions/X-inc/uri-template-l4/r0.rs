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

#[derive(Debug, Clone)]
struct Specifier {
    name: String,
    start: usize,
    modifier: Modifier,
}

#[derive(Debug, Clone, Copy)]
enum Modifier {
    None,
    Explode,
    Prefix(usize),
}

#[derive(Debug, Clone)]
struct Expression {
    operator: Operator,
    specs: Vec<Specifier>,
}

fn is_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_name_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn syntax(template: &str) -> Result<Vec<(usize, usize, Operator, Vec<Specifier>)>, Error> {
    let bytes = template.as_bytes();
    let mut result = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                let open = i;
                let close = match bytes[i + 1..].iter().position(|&b| b == b'}') {
                    Some(offset) => i + 1 + offset,
                    None => return Err(Error::Unclosed { pos: open }),
                };

                if let Some(offset) = bytes[open + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen { pos: open + 1 + offset });
                }

                let mut content_start = open + 1;
                let operator = if content_start < close {
                    match bytes[content_start] {
                        b'+' => { content_start += 1; Operator::Reserved }
                        b'#' => { content_start += 1; Operator::Fragment }
                        b'.' => { content_start += 1; Operator::Dot }
                        b'/' => { content_start += 1; Operator::Slash }
                        b';' => { content_start += 1; Operator::Semicolon }
                        b'?' => { content_start += 1; Operator::Query }
                        b'&' => { content_start += 1; Operator::Continuation }
                        _ => Operator::Simple,
                    }
                } else {
                    Operator::Simple
                };

                if content_start == close {
                    return Err(Error::EmptyExpression { pos: close });
                }

                let mut specs = Vec::new();
                let mut start = content_start;
                while start <= close {
                    let end = bytes[start..close]
                        .iter()
                        .position(|&b| b == b',')
                        .map(|offset| start + offset)
                        .unwrap_or(close);

                    if start == end {
                        return Err(Error::EmptyVariable { pos: start });
                    }

                    let spec = &bytes[start..end];
                    let modifier_at = spec.iter().position(|&b| b == b'*' || b == b':');
                    let name_end = modifier_at.unwrap_or(spec.len());

                    if name_end == 0 {
                        return Err(Error::InvalidName { pos: start });
                    }

                    for (offset, &b) in spec[..name_end].iter().enumerate() {
                        let valid = if offset == 0 {
                            is_name_start(b)
                        } else {
                            is_name_continue(b)
                        };
                        if !valid {
                            return Err(Error::InvalidName { pos: start + offset });
                        }
                    }

                    let modifier = if let Some(offset) = modifier_at {
                        let suffix = &spec[offset..];
                        if suffix == b"*" {
                            Modifier::Explode
                        } else if suffix.first() == Some(&b':')
                            && (2..=5).contains(&suffix.len())
                            && suffix[1].is_ascii_digit()
                            && suffix[1] != b'0'
                            && suffix[1..].iter().all(u8::is_ascii_digit)
                        {
                            let n = suffix[1..].iter().fold(0usize, |acc, b| {
                                acc * 10 + (b - b'0') as usize
                            });
                            if (1..=9999).contains(&n) {
                                Modifier::Prefix(n)
                            } else {
                                return Err(Error::InvalidModifier { pos: start + offset });
                            }
                        } else {
                            return Err(Error::InvalidModifier { pos: start + offset });
                        }
                    } else {
                        Modifier::None
                    };

                    let name = String::from_utf8(spec[..name_end].to_vec()).unwrap();
                    if specs.iter().any(|s: &Specifier| s.name == name) {
                        return Err(Error::DuplicateVariable { pos: start });
                    }
                    specs.push(Specifier { name, start, modifier });

                    if end == close {
                        break;
                    }
                    start = end + 1;
                }

                result.push((open, close, operator, specs));
                i = close + 1;
            }
            _ => i += 1,
        }
    }

    Ok(result)
}

fn is_hex(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

fn upper_hex(b: u8) -> u8 {
    b.to_ascii_uppercase()
}

fn reserved(b: u8) -> bool {
    matches!(
        b,
        b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&'
            | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

fn encode(input: &str, allow_reserved: bool) -> String {
    let bytes = input.as_bytes();
    let mut out = String::new();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' && i + 2 < bytes.len() && is_hex(bytes[i + 1]) && is_hex(bytes[i + 2]) {
            out.push('%');
            out.push(upper_hex(bytes[i + 1]) as char);
            out.push(upper_hex(bytes[i + 2]) as char);
            i += 3;
        } else if b.is_ascii_alphanumeric()
            || matches!(b, b'-' | b'.' | b'_' | b'~')
            || (allow_reserved && reserved(b))
        {
            out.push(b as char);
            i += 1;
        } else {
            out.push('%');
            out.push(b"0123456789ABCDEF"[(b >> 4) as usize] as char);
            out.push(b"0123456789ABCDEF"[(b & 0x0f) as usize] as char);
            i += 1;
        }
    }

    out
}

fn lookup<'a>(name: &str, vars: &'a [(&str, Value)]) -> Option<&'a Value> {
    vars.iter().rev().find(|(key, _)| *key == name).map(|(_, value)| value)
}

fn join_named(name: &str, value: String, operator: Operator) -> String {
    match operator {
        Operator::Semicolon if value.is_empty() => name.to_owned(),
        Operator::Semicolon | Operator::Query | Operator::Continuation => {
            format!("{name}={value}")
        }
        _ => value,
    }
}

fn expand_expression(operator: Operator, specs: &[Specifier], vars: &[(&str, Value)]) -> Result<String, Error> {
    let allow_reserved = matches!(operator, Operator::Reserved | Operator::Fragment);
    let mut fields = Vec::new();

    for spec in specs {
        let Some(value) = lookup(&spec.name, vars) else {
            continue;
        };

        match value {
            Value::Scalar(s) => {
                let value = match spec.modifier {
                    Modifier::Prefix(n) => s.chars().take(n).collect::<String>(),
                    _ => s.clone(),
                };
                fields.push(join_named(&spec.name, encode(&value, allow_reserved), operator));
            }
            Value::List(items) => {
                if let Modifier::Prefix(_) = spec.modifier {
                    return Err(Error::TypeMismatch { pos: spec.start });
                }
                if items.is_empty() {
                    continue;
                }
                let exploded = matches!(spec.modifier, Modifier::Explode);
                if exploded {
                    for item in items {
                        let encoded = encode(item, allow_reserved);
                        fields.push(join_named(&spec.name, encoded, operator));
                    }
                } else {
                    let joined = items.iter()
                        .map(|item| encode(item, allow_reserved))
                        .collect::<Vec<_>>()
                        .join(",");
                    fields.push(join_named(&spec.name, joined, operator));
                }
            }
            Value::Map(entries) => {
                if let Modifier::Prefix(_) = spec.modifier {
                    return Err(Error::TypeMismatch { pos: spec.start });
                }
                let mut seen = Vec::<&str>::new();
                for (key, _) in entries {
                    if seen.contains(&key.as_str()) {
                        return Err(Error::DuplicateKey { pos: spec.start });
                    }
                    seen.push(key);
                }
                if entries.is_empty() {
                    continue;
                }

                let exploded = matches!(spec.modifier, Modifier::Explode);
                if exploded {
                    for (key, value) in entries {
                        let k = encode(key, allow_reserved);
                        let v = encode(value, allow_reserved);
                        if operator == Operator::Semicolon && v.is_empty() {
                            fields.push(k);
                        } else {
                            fields.push(format!("{k}={v}"));
                        }
                    }
                } else {
                    let mut components = Vec::with_capacity(entries.len() * 2);
                    for (key, value) in entries {
                        components.push(encode(key, allow_reserved));
                        components.push(encode(value, allow_reserved));
                    }
                    fields.push(join_named(&spec.name, components.join(","), operator));
                }
            }
        }
    }

    if fields.is_empty() {
        return Ok(String::new());
    }

    let separator = match operator {
        Operator::Dot => ".",
        Operator::Slash => "/",
        Operator::Semicolon | Operator::Continuation => ";",
        Operator::Query => "&",
        _ => ",",
    };
    let prefix = match operator {
        Operator::Fragment => "#",
        Operator::Dot => ".",
        Operator::Slash => "/",
        Operator::Semicolon => ";",
        Operator::Query => "?",
        Operator::Continuation => "&",
        _ => "",
    };

    Ok(format!("{prefix}{}", fields.join(separator)))
}

pub fn expand(template: &str, vars: &[(&str, Value)]) -> Result<String, Error> {
    let expressions = syntax(template)?;
    let mut out = String::new();
    let mut cursor = 0;

    for (open, close, operator, specs) in expressions {
        out.push_str(&template[cursor..open]);
        out.push_str(&expand_expression(operator, &specs, vars)?);
        cursor = close + 1;
    }

    out.push_str(&template[cursor..]);
    Ok(out)
}