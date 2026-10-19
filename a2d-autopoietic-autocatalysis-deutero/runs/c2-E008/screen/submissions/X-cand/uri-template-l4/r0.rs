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
    Plus,
    Hash,
    Dot,
    Slash,
    Semicolon,
    Question,
    Ampersand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Modifier {
    None,
    Explode,
    Prefix(usize),
}

#[derive(Debug)]
struct Specifier {
    name: String,
    pos: usize,
    modifier: Modifier,
}

#[derive(Debug)]
struct Expression {
    start: usize,
    end: usize,
    operator: Operator,
    specs: Vec<Specifier>,
}

fn is_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_name_continue(b: u8) -> bool {
    is_name_start(b) || b.is_ascii_digit()
}

fn operator_from_byte(b: u8) -> Option<Operator> {
    match b {
        b'+' => Some(Operator::Plus),
        b'#' => Some(Operator::Hash),
        b'.' => Some(Operator::Dot),
        b'/' => Some(Operator::Slash),
        b';' => Some(Operator::Semicolon),
        b'?' => Some(Operator::Question),
        b'&' => Some(Operator::Ampersand),
        _ => None,
    }
}

fn parse_expression(
    template: &str,
    open_pos: usize,
    close_pos: usize,
) -> Result<Expression, Error> {
    let bytes = template.as_bytes();
    let mut content_pos = open_pos + 1;
    let operator = if content_pos < close_pos {
        if let Some(operator) = operator_from_byte(bytes[content_pos]) {
            content_pos += 1;
            operator
        } else {
            Operator::Simple
        }
    } else {
        Operator::Simple
    };

    if content_pos == close_pos {
        return Err(Error::EmptyExpression { pos: close_pos });
    }

    let mut specs = Vec::new();
    let mut seen = Vec::<String>::new();
    let mut spec_start = content_pos;

    loop {
        let comma_pos = bytes[spec_start..close_pos]
            .iter()
            .position(|&b| b == b',')
            .map(|offset| spec_start + offset)
            .unwrap_or(close_pos);

        if spec_start == comma_pos {
            return Err(Error::EmptyVariable { pos: spec_start });
        }

        let modifier_pos = bytes[spec_start..comma_pos]
            .iter()
            .position(|&b| b == b'*' || b == b':')
            .map(|offset| spec_start + offset)
            .unwrap_or(comma_pos);

        if modifier_pos == spec_start {
            return Err(Error::InvalidName { pos: spec_start });
        }

        if !is_name_start(bytes[spec_start]) {
            return Err(Error::InvalidName { pos: spec_start });
        }
        for pos in spec_start + 1..modifier_pos {
            if !is_name_continue(bytes[pos]) {
                return Err(Error::InvalidName { pos });
            }
        }

        let name = template[spec_start..modifier_pos].to_owned();
        let modifier = if modifier_pos == comma_pos {
            Modifier::None
        } else if bytes[modifier_pos] == b'*' {
            if modifier_pos + 1 != comma_pos {
                return Err(Error::InvalidModifier { pos: modifier_pos });
            }
            Modifier::Explode
        } else {
            let digits = &bytes[modifier_pos + 1..comma_pos];
            if digits.is_empty()
                || digits.len() > 4
                || digits[0] == b'0'
                || !digits.iter().all(u8::is_ascii_digit)
            {
                return Err(Error::InvalidModifier { pos: modifier_pos });
            }

            let value = digits
                .iter()
                .fold(0usize, |acc, b| acc * 10 + (b - b'0') as usize);
            if !(1..=9999).contains(&value) {
                return Err(Error::InvalidModifier { pos: modifier_pos });
            }
            Modifier::Prefix(value)
        };

        if seen.iter().any(|existing| existing == &name) {
            return Err(Error::DuplicateVariable { pos: spec_start });
        }
        seen.push(name.clone());
        specs.push(Specifier {
            name,
            pos: spec_start,
            modifier,
        });

        if comma_pos == close_pos {
            break;
        }
        spec_start = comma_pos + 1;
        if spec_start == close_pos {
            return Err(Error::EmptyVariable { pos: close_pos });
        }
    }

    Ok(Expression {
        start: open_pos,
        end: close_pos + 1,
        operator,
        specs,
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

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~')
}

fn is_reserved(b: u8) -> bool {
    matches!(
        b,
        b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&'
            | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

fn encode(input: &str, allow_reserved: bool) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    let bytes = input.as_bytes();
    let mut output = String::new();
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'%' && i + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                output.push('%');
                output.push(HEX[high as usize] as char);
                output.push(HEX[low as usize] as char);
                i += 3;
                continue;
            }
        }

        if is_unreserved(byte) || (allow_reserved && is_reserved(byte)) {
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

fn lookup<'a>(name: &str, vars: &'a [(&str, Value)]) -> Option<&'a Value> {
    vars.iter()
        .rev()
        .find(|(binding_name, _)| *binding_name == name)
        .map(|(_, value)| value)
}

fn named_field(name: &str, value: String, semicolon: bool) -> String {
    if semicolon && value.is_empty() {
        name.to_owned()
    } else {
        let mut field = String::with_capacity(name.len() + value.len() + 1);
        field.push_str(name);
        field.push('=');
        field.push_str(&value);
        field
    }
}

fn join_components<I>(components: I) -> String
where
    I: IntoIterator<Item = String>,
{
    components.into_iter().collect::<Vec<_>>().join(",")
}

fn expand_expression(
    expression: &Expression,
    vars: &[(&str, Value)],
) -> Result<String, Error> {
    let operator = expression.operator;
    let allow_reserved = matches!(operator, Operator::Plus | Operator::Hash);
    let named = matches!(
        operator,
        Operator::Semicolon | Operator::Question | Operator::Ampersand
    );
    let semicolon = operator == Operator::Semicolon;
    let mut fields = Vec::<String>::new();

    for spec in &expression.specs {
        let Some(value) = lookup(&spec.name, vars) else {
            continue;
        };

        if matches!(spec.modifier, Modifier::Prefix(_))
            && !matches!(value, Value::Scalar(_))
        {
            return Err(Error::TypeMismatch { pos: spec.pos });
        }

        match value {
            Value::Scalar(scalar) => {
                let selected = match spec.modifier {
                    Modifier::Prefix(count) => scalar.chars().take(count).collect::<String>(),
                    Modifier::None | Modifier::Explode => scalar.clone(),
                };
                let encoded = encode(&selected, allow_reserved);
                fields.push(if named {
                    named_field(&spec.name, encoded, semicolon)
                } else {
                    encoded
                });
            }
            Value::List(items) => {
                if items.is_empty() {
                    continue;
                }

                if spec.modifier == Modifier::Explode {
                    for item in items {
                        let encoded = encode(item, allow_reserved);
                        fields.push(if named {
                            named_field(&spec.name, encoded, semicolon)
                        } else {
                            encoded
                        });
                    }
                } else {
                    let joined = join_components(
                        items.iter().map(|item| encode(item, allow_reserved)),
                    );
                    fields.push(if named {
                        named_field(&spec.name, joined, semicolon)
                    } else {
                        joined
                    });
                }
            }
            Value::Map(entries) => {
                let mut keys = Vec::<&str>::new();
                for (key, _) in entries {
                    if keys.contains(&key.as_str()) {
                        return Err(Error::DuplicateKey { pos: spec.pos });
                    }
                    keys.push(key);
                }

                if entries.is_empty() {
                    continue;
                }

                if spec.modifier == Modifier::Explode {
                    for (key, value) in entries {
                        let encoded_key = encode(key, allow_reserved);
                        let encoded_value = encode(value, allow_reserved);
                        let field = if semicolon && encoded_value.is_empty() {
                            encoded_key
                        } else {
                            format!("{encoded_key}={encoded_value}")
                        };
                        fields.push(field);
                    }
                } else {
                    let joined = join_components(entries.iter().flat_map(|(key, value)| {
                        [
                            encode(key, allow_reserved),
                            encode(value, allow_reserved),
                        ]
                    }));
                    fields.push(if named {
                        named_field(&spec.name, joined, semicolon)
                    } else {
                        joined
                    });
                }
            }
        }
    }

    if fields.is_empty() {
        return Ok(String::new());
    }

    let (prefix, separator) = match operator {
        Operator::Simple | Operator::Plus => ("", ","),
        Operator::Hash => ("#", ","),
        Operator::Dot => (".", "."),
        Operator::Slash => ("/", "/"),
        Operator::Semicolon => (";", ";"),
        Operator::Question => ("?", "&"),
        Operator::Ampersand => ("&", "&"),
    };

    let mut output = String::from(prefix);
    output.push_str(&fields.join(separator));
    Ok(output)
}

pub fn expand(template: &str, vars: &[(&str, Value)]) -> Result<String, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::<Expression>::new();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                let open_pos = i;
                let Some(relative_close) = bytes[i + 1..].iter().position(|&b| b == b'}') else {
                    return Err(Error::Unclosed { pos: open_pos });
                };
                let close_pos = i + 1 + relative_close;

                if let Some(relative_open) =
                    bytes[i + 1..close_pos].iter().position(|&b| b == b'{')
                {
                    return Err(Error::NestedOpen {
                        pos: i + 1 + relative_open,
                    });
                }

                expressions.push(parse_expression(template, open_pos, close_pos)?);
                i = close_pos + 1;
            }
            _ => i += 1,
        }
    }

    let mut output = String::new();
    let mut cursor = 0;

    for expression in &expressions {
        output.push_str(&template[cursor..expression.start]);
        output.push_str(&expand_expression(expression, vars)?);
        cursor = expression.end;
    }

    output.push_str(&template[cursor..]);
    Ok(output)
}