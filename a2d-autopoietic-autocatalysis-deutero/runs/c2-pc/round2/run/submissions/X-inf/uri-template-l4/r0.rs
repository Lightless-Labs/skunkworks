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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Modifier {
    Explode,
    Prefix(usize),
}

struct Spec {
    name: String,
    start: usize,
    modifier: Option<Modifier>,
}

struct Expression {
    operator: Operator,
    specs: Vec<Spec>,
}

fn operator(byte: u8, pos: usize) -> Result<Option<Operator>, Error> {
    let op = match byte {
        b'+' => Operator::Plus,
        b'#' => Operator::Hash,
        b'.' => Operator::Dot,
        b'/' => Operator::Slash,
        b';' => Operator::Semicolon,
        b'?' => Operator::Question,
        b'&' => Operator::Ampersand,
        _ => return Ok(None),
    };
    let _ = pos;
    Ok(Some(op))
}

fn valid_name_byte(byte: u8, first: bool) -> bool {
    if first {
        byte.is_ascii_alphabetic() || byte == b'_'
    } else {
        byte.is_ascii_alphanumeric() || byte == b'_'
    }
}

fn parse_expression(content: &str, base: usize, close: usize) -> Result<Expression, Error> {
    let bytes = content.as_bytes();
    let (operator, offset) = if let Some(&first) = bytes.first() {
        match operator(first, base)? {
            Some(op) => (op, 1),
            None => (Operator::Simple, 0),
        }
    } else {
        return Err(Error::EmptyExpression { pos: close });
    };

    if offset == bytes.len() {
        return Err(Error::EmptyExpression { pos: close });
    }

    let mut specs = Vec::new();
    let mut names = std::collections::HashSet::new();
    let mut part_start = offset;

    loop {
        let relative_end = bytes[part_start..]
            .iter()
            .position(|&b| b == b',')
            .map(|i| part_start + i)
            .unwrap_or(bytes.len());

        if relative_end == part_start {
            return Err(Error::EmptyVariable {
                pos: base + part_start,
            });
        }

        let part = &bytes[part_start..relative_end];
        let modifier_at = part.iter().position(|&b| b == b'*' || b == b':');
        let name_end = modifier_at.map(|i| part_start + i).unwrap_or(relative_end);

        if name_end == part_start {
            return Err(Error::InvalidName {
                pos: base + part_start,
            });
        }

        let name_bytes = &bytes[part_start..name_end];
        for (i, &byte) in name_bytes.iter().enumerate() {
            if !valid_name_byte(byte, i == 0) {
                return Err(Error::InvalidName {
                    pos: base + part_start + i,
                });
            }
        }

        let modifier = if let Some(marker_index) = modifier_at {
            let marker = part[marker_index];
            let suffix = &part[marker_index..];
            let parsed = if marker == b'*' && suffix == b"*" {
                Some(Modifier::Explode)
            } else if marker == b':' {
                let digits = &suffix[1..];
                if (1..=4).contains(&digits.len())
                    && digits[0].is_ascii_digit()
                    && digits[0] != b'0'
                    && digits.iter().all(u8::is_ascii_digit)
                {
                    let value = digits.iter().fold(0usize, |n, d| {
                        n * 10 + usize::from(*d - b'0')
                    });
                    if (1..=9999).contains(&value) {
                        Some(Modifier::Prefix(value))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            Some(parsed.ok_or(Error::InvalidModifier {
                pos: base + part_start + marker_index,
            })?)
        } else {
            None
        };

        let name = std::str::from_utf8(name_bytes).expect("ASCII name");
        if !names.insert(name.to_owned()) {
            return Err(Error::DuplicateVariable {
                pos: base + part_start,
            });
        }
        specs.push(Spec {
            name: name.to_owned(),
            start: base + part_start,
            modifier,
        });

        if relative_end == bytes.len() {
            break;
        }
        part_start = relative_end + 1;
        if part_start == bytes.len() {
            return Err(Error::EmptyVariable {
                pos: base + part_start,
            });
        }
    }

    Ok(Expression { operator, specs })
}

fn encode(value: &str, reserved: bool) -> String {
    let bytes = value.as_bytes();
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
        if b.is_ascii_alphanumeric()
            || matches!(b, b'-' | b'.' | b'_' | b'~')
            || (reserved && b":/?#[]@!$&'()*+,;=".contains(&b))
        {
            out.push(b as char);
        } else {
            out.push('%');
            out.push_str(&format!("{:02X}", b));
        }
        i += 1;
    }
    out
}

fn selected<'a>(vars: &'a [(&str, Value)], name: &str) -> Option<&'a Value> {
    vars.iter()
        .rev()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value)
}

fn named_field(name: &str, value: String, op: Operator) -> String {
    match op {
        Operator::Semicolon if value.is_empty() => name.to_owned(),
        _ => format!("{name}={value}"),
    }
}

fn expand_expression(expr: &Expression, vars: &[(&str, Value)]) -> Result<String, Error> {
    let reserved = matches!(expr.operator, Operator::Plus | Operator::Hash);
    let separator = match expr.operator {
        Operator::Simple | Operator::Plus | Operator::Hash => ",",
        Operator::Dot => ".",
        Operator::Slash => "/",
        Operator::Semicolon => ";",
        Operator::Question | Operator::Ampersand => "&",
    };

    let mut fields = Vec::new();
    for spec in &expr.specs {
        let Some(value) = selected(vars, &spec.name) else {
            continue;
        };

        if matches!(spec.modifier, Some(Modifier::Prefix(_)))
            && !matches!(value, Value::Scalar(_))
        {
            return Err(Error::TypeMismatch { pos: spec.start });
        }

        match value {
            Value::Scalar(s) => {
                let selected_string = match spec.modifier {
                    Some(Modifier::Prefix(n)) => s.chars().take(n).collect::<String>(),
                    _ => s.clone(),
                };
                let encoded = encode(&selected_string, reserved);
                if matches!(
                    expr.operator,
                    Operator::Semicolon | Operator::Question | Operator::Ampersand
                ) {
                    fields.push(named_field(&spec.name, encoded, expr.operator));
                } else {
                    fields.push(encoded);
                }
            }
            Value::List(items) => {
                if items.is_empty() {
                    continue;
                }
                if spec.modifier == Some(Modifier::Explode) {
                    for item in items {
                        let encoded = encode(item, reserved);
                        if matches!(
                            expr.operator,
                            Operator::Semicolon | Operator::Question | Operator::Ampersand
                        ) {
                            fields.push(named_field(&spec.name, encoded, expr.operator));
                        } else {
                            fields.push(encoded);
                        }
                    }
                } else {
                    let joined = items
                        .iter()
                        .map(|item| encode(item, reserved))
                        .collect::<Vec<_>>()
                        .join(",");
                    if matches!(
                        expr.operator,
                        Operator::Semicolon | Operator::Question | Operator::Ampersand
                    ) {
                        fields.push(named_field(&spec.name, joined, expr.operator));
                    } else {
                        fields.push(joined);
                    }
                }
            }
            Value::Map(entries) => {
                if entries.is_empty() {
                    continue;
                }
                let mut keys = std::collections::HashSet::new();
                for (key, _) in entries {
                    if !keys.insert(key) {
                        return Err(Error::DuplicateKey { pos: spec.start });
                    }
                }
                if spec.modifier == Some(Modifier::Explode) {
                    for (key, val) in entries {
                        let key = encode(key, reserved);
                        let val = encode(val, reserved);
                        if expr.operator == Operator::Semicolon && val.is_empty() {
                            fields.push(key);
                        } else {
                            fields.push(format!("{key}={val}"));
                        }
                    }
                } else {
                    let mut components = Vec::with_capacity(entries.len() * 2);
                    for (key, val) in entries {
                        components.push(encode(key, reserved));
                        components.push(encode(val, reserved));
                    }
                    let joined = components.join(",");
                    if matches!(
                        expr.operator,
                        Operator::Semicolon | Operator::Question | Operator::Ampersand
                    ) {
                        fields.push(named_field(&spec.name, joined, expr.operator));
                    } else {
                        fields.push(joined);
                    }
                }
            }
        }
    }

    if fields.is_empty() {
        return Ok(String::new());
    }
    let joined = fields.join(separator);
    let prefix = match expr.operator {
        Operator::Simple | Operator::Plus => "",
        Operator::Hash => "#",
        Operator::Dot => ".",
        Operator::Slash => "/",
        Operator::Semicolon => ";",
        Operator::Question => "?",
        Operator::Ampersand => "&",
    };
    Ok(format!("{prefix}{joined}"))
}

pub fn expand(template: &str, vars: &[(&str, Value)]) -> Result<String, Error> {
    let mut expressions = Vec::new();
    let mut literal_spans = Vec::new();
    let bytes = template.as_bytes();
    let mut cursor = 0;
    let mut literal_start = 0;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'}' => return Err(Error::UnexpectedClose { pos: cursor }),
            b'{' => {
                let open = cursor;
                let Some(relative_close) = bytes[cursor + 1..].iter().position(|&b| b == b'}')
                else {
                    return Err(Error::Unclosed { pos: open });
                };
                let close = cursor + 1 + relative_close;
                if let Some(relative_nested) =
                    bytes[open + 1..close].iter().position(|&b| b == b'{')
                {
                    return Err(Error::NestedOpen {
                        pos: open + 1 + relative_nested,
                    });
                }
                let content = &template[open + 1..close];
                let expr = parse_expression(content, open + 1, close)?;
                literal_spans.push(&template[literal_start..open]);
                expressions.push(expr);
                cursor = close + 1;
                literal_start = cursor;
            }
            _ => cursor += 1,
        }
    }
    literal_spans.push(&template[literal_start..]);

    let mut output = String::new();
    for (i, expr) in expressions.iter().enumerate() {
        output.push_str(literal_spans[i]);
        output.push_str(&expand_expression(expr, vars)?);
    }
    output.push_str(literal_spans.last().copied().unwrap_or(""));
    Ok(output)
}