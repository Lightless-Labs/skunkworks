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
    Query,
    Ampersand,
}

impl Operator {
    fn from_byte(b: u8) -> Option<Self> {
        match b {
            b'+' => Some(Self::Plus),
            b'#' => Some(Self::Hash),
            b'.' => Some(Self::Dot),
            b'/' => Some(Self::Slash),
            b';' => Some(Self::Semicolon),
            b'?' => Some(Self::Query),
            b'&' => Some(Self::Ampersand),
            _ => None,
        }
    }

    fn separator(self) -> u8 {
        match self {
            Self::Simple | Self::Plus | Self::Hash => b',',
            Self::Dot => b'.',
            Self::Slash => b'/',
            Self::Semicolon => b';',
            Self::Query | Self::Ampersand => b'&',
        }
    }

    fn prefix(self) -> Option<u8> {
        match self {
            Self::Simple | Self::Plus => None,
            Self::Hash => Some(b'#'),
            Self::Dot => Some(b'.'),
            Self::Slash => Some(b'/'),
            Self::Semicolon => Some(b';'),
            Self::Query => Some(b'?'),
            Self::Ampersand => Some(b'&'),
        }
    }

    fn named_fields(self) -> bool {
        matches!(self, Self::Semicolon | Self::Query | Self::Ampersand)
    }

    fn reserved(self) -> bool {
        matches!(self, Self::Plus | Self::Hash)
    }
}

#[derive(Clone, Copy)]
enum Modifier {
    None,
    Explode,
    Prefix(usize),
}

struct Specifier<'a> {
    name: &'a str,
    name_pos: usize,
    start: usize,
    modifier: Modifier,
}

struct Expression<'a> {
    operator: Operator,
    specs: Vec<Specifier<'a>>,
}

fn is_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_name_continue(b: u8) -> bool {
    is_name_start(b) || b.is_ascii_digit()
}

fn parse_expression<'a>(
    template: &'a str,
    content_start: usize,
    close: usize,
) -> Result<Expression<'a>, Error> {
    let bytes = template.as_bytes();
    let mut start = content_start;
    let mut operator = Operator::Simple;

    if start < close {
        if let Some(op) = Operator::from_byte(bytes[start]) {
            operator = op;
            start += 1;
        } else if matches!(bytes[start], b'=' | b'!' | b'@' | b'|' | b'$' | b'%' | b'^') {
            return Err(Error::UnsupportedOperator { pos: start });
        }
    }

    if start == close {
        return Err(Error::EmptyExpression { pos: close });
    }

    let mut specs = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    let mut spec_start = start;

    loop {
        let spec_end = bytes[spec_start..close]
            .iter()
            .position(|&b| b == b',')
            .map(|offset| spec_start + offset)
            .unwrap_or(close);

        if spec_start == spec_end {
            return Err(Error::EmptyVariable { pos: spec_start });
        }

        let spec_bytes = &bytes[spec_start..spec_end];
        let modifier_offset = spec_bytes
            .iter()
            .position(|&b| b == b'*' || b == b':')
            .unwrap_or(spec_bytes.len());
        let name_end = spec_start + modifier_offset;

        if name_end == spec_start || !is_name_start(bytes[spec_start]) {
            return Err(Error::InvalidName { pos: spec_start });
        }

        let mut i = spec_start + 1;
        while i < name_end {
            if !is_name_continue(bytes[i]) {
                return Err(Error::InvalidName { pos: i });
            }
            i += 1;
        }

        let name = &template[spec_start..name_end];
        let modifier = if name_end == spec_end {
            Modifier::None
        } else {
            let modifier_pos = name_end;
            let suffix = &bytes[modifier_pos..spec_end];
            if suffix == b"*" {
                Modifier::Explode
            } else if suffix.first() == Some(&b':') {
                let digits = &suffix[1..];
                if digits.is_empty()
                    || digits.len() > 4
                    || digits[0] == b'0'
                    || !digits.iter().all(u8::is_ascii_digit)
                {
                    return Err(Error::InvalidModifier { pos: modifier_pos });
                }
                let mut value = 0usize;
                for &digit in digits {
                    value = value
                        .checked_mul(10)
                        .and_then(|n| n.checked_add((digit - b'0') as usize))
                        .ok_or(Error::InvalidModifier { pos: modifier_pos })?;
                }
                if !(1..=9999).contains(&value) {
                    return Err(Error::InvalidModifier { pos: modifier_pos });
                }
                Modifier::Prefix(value)
            } else {
                return Err(Error::InvalidModifier { pos: modifier_pos });
            }
        };

        if seen.iter().any(|&previous| previous == name) {
            return Err(Error::DuplicateVariable { pos: spec_start });
        }
        seen.push(name);
        specs.push(Specifier {
            name,
            name_pos: spec_start,
            start: spec_start,
            modifier,
        });

        if spec_end == close {
            break;
        }
        spec_start = spec_end + 1;
    }

    Ok(Expression { operator, specs })
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
        b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&' | b'\''
            | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

fn encode(input: &str, reserved: bool) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = input.as_bytes();
    let mut result = String::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' && i + 2 < bytes.len() {
            if hex_value(bytes[i + 1]).is_some() && hex_value(bytes[i + 2]).is_some() {
                result.push('%');
                result.push(HEX[hex_value(bytes[i + 1]).unwrap() as usize] as char);
                result.push(HEX[hex_value(bytes[i + 2]).unwrap() as usize] as char);
                i += 3;
                continue;
            }
        }

        if is_unreserved(b) || (reserved && is_reserved(b)) {
            result.push(b as char);
        } else {
            result.push('%');
            result.push(HEX[(b >> 4) as usize] as char);
            result.push(HEX[(b & 0x0f) as usize] as char);
        }
        i += 1;
    }

    result
}

fn truncate_chars(value: &str, count: usize) -> &str {
    match value.char_indices().nth(count) {
        Some((index, _)) => &value[..index],
        None => value,
    }
}

fn selected_value<'a>(name: &str, vars: &'a [(&str, Value)]) -> Option<&'a Value> {
    vars.iter()
        .rev()
        .find(|(binding_name, _)| *binding_name == name)
        .map(|(_, value)| value)
}

fn encode_named(name: &str, value: &str, operator: Operator) -> String {
    let encoded = encode(value, operator.reserved());
    if operator == Operator::Semicolon && encoded.is_empty() {
        name.to_owned()
    } else {
        let mut field = String::with_capacity(name.len() + encoded.len() + 1);
        field.push_str(name);
        field.push('=');
        field.push_str(&encoded);
        field
    }
}

fn expand_expression(expression: &Expression<'_>, vars: &[(&str, Value)]) -> Result<String, Error> {
    let operator = expression.operator;
    let mut fields: Vec<String> = Vec::new();

    for spec in &expression.specs {
        let Some(value) = selected_value(spec.name, vars) else {
            continue;
        };

        match (spec.modifier, value) {
            (Modifier::Prefix(_), Value::List(_) | Value::Map(_)) => {
                return Err(Error::TypeMismatch { pos: spec.start });
            }
            (_, Value::Scalar(scalar)) => {
                let value = match spec.modifier {
                    Modifier::Prefix(count) => truncate_chars(scalar, count),
                    Modifier::None | Modifier::Explode => scalar.as_str(),
                };
                if operator.named_fields() {
                    fields.push(encode_named(spec.name, value, operator));
                } else {
                    fields.push(encode(value, operator.reserved()));
                }
            }
            (modifier, Value::List(items)) => {
                let explode = matches!(modifier, Modifier::Explode);
                if !explode {
                    if !items.is_empty() {
                        let joined = items
                            .iter()
                            .map(|item| encode(item, operator.reserved()))
                            .collect::<Vec<_>>()
                            .join(",");
                        if operator.named_fields() {
                            fields.push(if operator == Operator::Semicolon && joined.is_empty() {
                                spec.name.to_owned()
                            } else {
                                format!("{}={}", spec.name, joined)
                            });
                        } else {
                            fields.push(joined);
                        }
                    }
                } else {
                    for item in items {
                        if operator.named_fields() {
                            fields.push(encode_named(spec.name, item, operator));
                        } else {
                            fields.push(encode(item, operator.reserved()));
                        }
                    }
                }
            }
            (modifier, Value::Map(entries)) => {
                let mut keys: Vec<&str> = Vec::new();
                for (key, _) in entries {
                    if keys.iter().any(|&previous| previous == key) {
                        return Err(Error::DuplicateKey { pos: spec.start });
                    }
                    keys.push(key);
                }

                if entries.is_empty() {
                    continue;
                }

                if matches!(modifier, Modifier::Explode) {
                    for (key, value) in entries {
                        let encoded_key = encode(key, operator.reserved());
                        let encoded_value = encode(value, operator.reserved());
                        if operator == Operator::Semicolon && encoded_value.is_empty() {
                            fields.push(encoded_key);
                        } else {
                            fields.push(format!("{}={}", encoded_key, encoded_value));
                        }
                    }
                } else {
                    let mut components = Vec::with_capacity(entries.len() * 2);
                    for (key, value) in entries {
                        components.push(encode(key, operator.reserved()));
                        components.push(encode(value, operator.reserved()));
                    }
                    let joined = components.join(",");
                    if operator.named_fields() {
                        fields.push(if operator == Operator::Semicolon && joined.is_empty() {
                            spec.name.to_owned()
                        } else {
                            format!("{}={}", spec.name, joined)
                        });
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

    let mut output = String::new();
    if let Some(prefix) = operator.prefix() {
        output.push(prefix as char);
    }
    output.push_str(&fields.join(&(operator.separator() as char).to_string()));
    Ok(output)
}

pub fn expand(template: &str, vars: &[(&str, Value)]) -> Result<String, Error> {
    let bytes = template.as_bytes();
    let mut expressions: Vec<(usize, usize, Expression<'_>)> = Vec::new();
    let mut cursor = 0;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'}' => return Err(Error::UnexpectedClose { pos: cursor }),
            b'{' => {
                let open = cursor;
                let close = match bytes[open + 1..].iter().position(|&b| b == b'}') {
                    Some(offset) => open + 1 + offset,
                    None => return Err(Error::Unclosed { pos: open }),
                };

                if let Some(offset) = bytes[open + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: open + 1 + offset,
                    });
                }

                let expression = parse_expression(template, open + 1, close)?;
                expressions.push((open, close + 1, expression));
                cursor = close + 1;
            }
            _ => cursor += 1,
        }
    }

    let mut output = String::new();
    let mut literal_start = 0;
    for (open, end, expression) in &expressions {
        output.push_str(&template[literal_start..*open]);
        output.push_str(&expand_expression(expression, vars)?);
        literal_start = *end;
    }
    output.push_str(&template[literal_start..]);
    Ok(output)
}