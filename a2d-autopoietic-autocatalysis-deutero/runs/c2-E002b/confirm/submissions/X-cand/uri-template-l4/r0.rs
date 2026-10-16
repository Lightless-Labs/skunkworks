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
    Hash,
    Dot,
    Slash,
    Semicolon,
    Question,
    Ampersand,
}

#[derive(Clone)]
struct Specifier {
    name: String,
    start: usize,
    modifier: Modifier,
}

#[derive(Clone, Copy)]
enum Modifier {
    None,
    Explode,
    Prefix(usize),
}

struct Expression {
    operator: Operator,
    specs: Vec<Specifier>,
}

fn is_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_name_continue(b: u8) -> bool {
    is_name_start(b) || b.is_ascii_digit()
}

fn parse_expression(content: &str, base: usize, close: usize) -> Result<Expression, Error> {
    let bytes = content.as_bytes();
    let mut operator = Operator::Simple;
    let mut offset = 0;

    if let Some(&b) = bytes.first() {
        operator = match b {
            b'+' => Operator::Plus,
            b'#' => Operator::Hash,
            b'.' => Operator::Dot,
            b'/' => Operator::Slash,
            b';' => Operator::Semicolon,
            b'?' => Operator::Question,
            b'&' => Operator::Ampersand,
            _ => Operator::Simple,
        };
        if operator != Operator::Simple {
            offset = 1;
        }
    }

    if offset == bytes.len() {
        return Err(Error::EmptyExpression { pos: close });
    }

    let mut specs = Vec::new();
    let mut used = Vec::<String>::new();
    let mut start = offset;

    loop {
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b',')
            .map(|p| start + p)
            .unwrap_or(bytes.len());

        if start == end {
            return Err(Error::EmptyVariable { pos: base + start });
        }

        let part = &bytes[start..end];
        let modifier_index = part.iter().position(|&b| b == b'*' || b == b':');
        let name_end = modifier_index.map(|i| start + i).unwrap_or(end);

        if name_end == start {
            return Err(Error::InvalidName { pos: base + start });
        }

        let name_bytes = &bytes[start..name_end];
        if !is_name_start(name_bytes[0]) {
            return Err(Error::InvalidName { pos: base + start });
        }
        for (i, &b) in name_bytes.iter().enumerate().skip(1) {
            if !is_name_continue(b) {
                return Err(Error::InvalidName { pos: base + start + i });
            }
        }

        let name = content[start..name_end].to_owned();
        let modifier = if let Some(mi) = modifier_index {
            let mod_start = start + mi;
            let suffix = &bytes[mod_start..end];
            if suffix == b"*" {
                Modifier::Explode
            } else if suffix.len() >= 2
                && suffix[0] == b':'
                && suffix.len() <= 5
                && suffix[1].is_ascii_digit()
                && suffix[1] != b'0'
                && suffix[1..].iter().all(u8::is_ascii_digit)
            {
                let n = suffix[1..]
                    .iter()
                    .fold(0usize, |acc, b| acc * 10 + (b - b'0') as usize);
                if (1..=9999).contains(&n) {
                    Modifier::Prefix(n)
                } else {
                    return Err(Error::InvalidModifier { pos: base + mod_start });
                }
            } else {
                return Err(Error::InvalidModifier { pos: base + mod_start });
            }
        } else {
            Modifier::None
        };

        if used.iter().any(|existing| existing == &name) {
            return Err(Error::DuplicateVariable { pos: base + start });
        }
        used.push(name.clone());
        specs.push(Specifier {
            name,
            start: base + start,
            modifier,
        });

        if end == bytes.len() {
            break;
        }
        start = end + 1;
        if start == bytes.len() {
            return Err(Error::EmptyVariable { pos: close });
        }
    }

    Ok(Expression { operator, specs })
}

fn encode(input: &str, reserved: bool) -> String {
    let bytes = input.as_bytes();
    let mut out = String::new();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%'
            && i + 2 < bytes.len()
            && bytes[i + 1].is_ascii_hexdigit()
            && bytes[i + 2].is_ascii_hexdigit()
        {
            out.push('%');
            out.push((bytes[i + 1] as char).to_ascii_uppercase());
            out.push((bytes[i + 2] as char).to_ascii_uppercase());
            i += 3;
            continue;
        }

        let unreserved = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~');
        let allowed_reserved = reserved
            && matches!(
                b,
                b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&'
                    | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
            );

        if unreserved || allowed_reserved {
            out.push(b as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
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

fn encode_named(name: &str, value: String, operator: Operator, reserved: bool) -> String {
    let encoded = encode(&value, reserved);
    match operator {
        Operator::Semicolon if encoded.is_empty() => name.to_owned(),
        Operator::Semicolon => format!("{name}={encoded}"),
        Operator::Question | Operator::Ampersand => format!("{name}={encoded}"),
        _ => encoded,
    }
}

fn expand_expression(expression: &Expression, vars: &[(&str, Value)]) -> Result<String, Error> {
    let reserved = matches!(expression.operator, Operator::Plus | Operator::Hash);
    let mut fields = Vec::<String>::new();

    for spec in &expression.specs {
        let Some(value) = selected_value(&spec.name, vars) else {
            continue;
        };

        let prefix = match spec.modifier {
            Modifier::Prefix(n) => Some(n),
            _ => None,
        };

        if prefix.is_some() && !matches!(value, Value::Scalar(_)) {
            return Err(Error::TypeMismatch { pos: spec.start });
        }

        match value {
            Value::Scalar(s) => {
                let selected = if let Some(n) = prefix {
                    s.chars().take(n).collect::<String>()
                } else {
                    s.clone()
                };
                fields.push(encode_named(&spec.name, selected, expression.operator, reserved));
            }
            Value::List(items) => {
                if items.is_empty() {
                    continue;
                }
                if matches!(spec.modifier, Modifier::Explode) {
                    for item in items {
                        fields.push(encode_named(&spec.name, item.clone(), expression.operator, reserved));
                    }
                } else {
                    let joined = items
                        .iter()
                        .map(|item| encode(item, reserved))
                        .collect::<Vec<_>>()
                        .join(",");
                    fields.push(match expression.operator {
                        Operator::Semicolon if joined.is_empty() => spec.name.clone(),
                        Operator::Semicolon => format!("{}={joined}", spec.name),
                        Operator::Question | Operator::Ampersand => {
                            format!("{}={joined}", spec.name)
                        }
                        _ => joined,
                    });
                }
            }
            Value::Map(entries) => {
                let mut seen = Vec::<&str>::new();
                for (key, _) in entries {
                    if seen.iter().any(|existing| *existing == key) {
                        return Err(Error::DuplicateKey { pos: spec.start });
                    }
                    seen.push(key);
                }
                if entries.is_empty() {
                    continue;
                }

                if matches!(spec.modifier, Modifier::Explode) {
                    for (key, value) in entries {
                        let encoded_key = encode(key, reserved);
                        let encoded_value = encode(value, reserved);
                        let field = if expression.operator == Operator::Semicolon
                            && encoded_value.is_empty()
                        {
                            encoded_key
                        } else {
                            format!("{encoded_key}={encoded_value}")
                        };
                        fields.push(field);
                    }
                } else {
                    let mut components = Vec::with_capacity(entries.len() * 2);
                    for (key, value) in entries {
                        components.push(encode(key, reserved));
                        components.push(encode(value, reserved));
                    }
                    let joined = components.join(",");
                    fields.push(match expression.operator {
                        Operator::Semicolon if joined.is_empty() => spec.name.clone(),
                        Operator::Semicolon => format!("{}={joined}", spec.name),
                        Operator::Question | Operator::Ampersand => {
                            format!("{}={joined}", spec.name)
                        }
                        _ => joined,
                    });
                }
            }
        }
    }

    if fields.is_empty() {
        return Ok(String::new());
    }

    let (separator, prefix) = match expression.operator {
        Operator::Simple | Operator::Plus => (",", ""),
        Operator::Hash => (",", "#"),
        Operator::Dot => (".", "."),
        Operator::Slash => ("/", "/"),
        Operator::Semicolon => (";", ";"),
        Operator::Question => ("&", "?"),
        Operator::Ampersand => ("&", "&"),
    };

    Ok(format!("{prefix}{}", fields.join(separator)))
}

pub fn expand(template: &str, vars: &[(&str, Value)]) -> Result<String, Error> {
    let mut parsed = Vec::<(usize, usize, Expression)>::new();
    let mut i = 0;
    let bytes = template.as_bytes();

    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
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
                let expression = parse_expression(
                    &template[open + 1..close],
                    open + 1,
                    close,
                )?;
                parsed.push((open, close + 1, expression));
                i = close + 1;
            }
            _ => i += 1,
        }
    }

    let mut out = String::new();
    let mut cursor = 0;
    for (start, end, expression) in parsed {
        out.push_str(&template[cursor..start]);
        out.push_str(&expand_expression(&expression, vars)?);
        cursor = end;
    }
    out.push_str(&template[cursor..]);
    Ok(out)
}