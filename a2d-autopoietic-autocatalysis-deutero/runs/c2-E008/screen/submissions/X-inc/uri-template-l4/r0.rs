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
    Query,
    Ampersand,
}

impl Operator {
    fn separator(self) -> char {
        match self {
            Self::Dot => '.',
            Self::Slash => '/',
            Self::Semicolon => ';',
            Self::Query | Self::Ampersand => '&',
            _ => ',',
        }
    }

    fn prefix(self) -> Option<char> {
        match self {
            Self::Hash => Some('#'),
            Self::Dot => Some('.'),
            Self::Slash => Some('/'),
            Self::Semicolon => Some(';'),
            Self::Query => Some('?'),
            Self::Ampersand => Some('&'),
            _ => None,
        }
    }

    fn reserved(self) -> bool {
        matches!(self, Self::Plus | Self::Hash)
    }

    fn named_fields(self) -> bool {
        matches!(self, Self::Semicolon | Self::Query | Self::Ampersand)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Modifier {
    Explode,
    Prefix(usize),
}

#[derive(Debug, Clone)]
struct Specifier {
    name: String,
    modifier: Option<Modifier>,
    pos: usize,
}

#[derive(Debug, Clone)]
struct Expression {
    operator: Operator,
    specifiers: Vec<Specifier>,
}

#[derive(Debug, Clone)]
enum Part {
    Literal(String),
    Expression(Expression),
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn validate_specifier(
    source: &str,
    start: usize,
    end: usize,
    seen: &mut Vec<String>,
) -> Result<Specifier, Error> {
    if start == end {
        return Err(Error::EmptyVariable { pos: start });
    }

    let bytes = source.as_bytes();
    let mut modifier_start = end;
    for i in start..end {
        if bytes[i] == b'*' || bytes[i] == b':' {
            modifier_start = i;
            break;
        }
    }

    if modifier_start == start {
        return Err(Error::InvalidName { pos: start });
    }

    if !is_name_start(bytes[start]) {
        return Err(Error::InvalidName { pos: start });
    }
    for i in start + 1..modifier_start {
        if !is_name_continue(bytes[i]) {
            return Err(Error::InvalidName { pos: i });
        }
    }

    let modifier = if modifier_start == end {
        None
    } else {
        let suffix = &source[modifier_start..end];
        if suffix == "*" {
            Some(Modifier::Explode)
        } else if let Some(digits) = suffix.strip_prefix(':') {
            let valid = (1..=4).contains(&digits.len())
                && digits.as_bytes()[0].is_ascii_digit()
                && digits.as_bytes()[0] != b'0'
                && digits.bytes().all(|b| b.is_ascii_digit());
            if !valid {
                return Err(Error::InvalidModifier { pos: modifier_start });
            }
            let mut value = 0usize;
            for digit in digits.bytes() {
                value = value * 10 + (digit - b'0') as usize;
            }
            if !(1..=9999).contains(&value) {
                return Err(Error::InvalidModifier { pos: modifier_start });
            }
            Some(Modifier::Prefix(value))
        } else {
            return Err(Error::InvalidModifier { pos: modifier_start });
        }
    };

    let name = source[start..modifier_start].to_owned();
    if seen.iter().any(|existing| existing == &name) {
        return Err(Error::DuplicateVariable { pos: start });
    }
    seen.push(name.clone());

    Ok(Specifier {
        name,
        modifier,
        pos: start,
    })
}

fn parse_expression(source: &str, open: usize, close: usize) -> Result<Expression, Error> {
    let bytes = source.as_bytes();
    let mut content_start = open + 1;
    let mut operator = Operator::Simple;

    if content_start < close {
        operator = match bytes[content_start] {
            b'+' => Operator::Plus,
            b'#' => Operator::Hash,
            b'.' => Operator::Dot,
            b'/' => Operator::Slash,
            b';' => Operator::Semicolon,
            b'?' => Operator::Query,
            b'&' => Operator::Ampersand,
            _ => Operator::Simple,
        };
        if operator != Operator::Simple {
            content_start += 1;
        }
    }

    if content_start == close {
        return Err(Error::EmptyExpression { pos: close });
    }

    let mut specifiers = Vec::new();
    let mut seen = Vec::new();
    let mut start = content_start;

    for i in content_start..=close {
        if i == close || bytes[i] == b',' {
            specifiers.push(validate_specifier(source, start, i, &mut seen)?);
            start = i + 1;
        }
    }

    Ok(Expression {
        operator,
        specifiers,
    })
}

fn parse_template(template: &str) -> Result<Vec<Part>, Error> {
    let bytes = template.as_bytes();
    let mut parts = Vec::new();
    let mut literal_start = 0;
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                if literal_start < i {
                    parts.push(Part::Literal(template[literal_start..i].to_owned()));
                }

                let open = i;
                let mut close = open + 1;
                while close < bytes.len() && bytes[close] != b'}' {
                    close += 1;
                }
                if close == bytes.len() {
                    return Err(Error::Unclosed { pos: open });
                }

                if let Some(inner) = (open + 1..close).find(|&index| bytes[index] == b'{') {
                    return Err(Error::NestedOpen { pos: inner });
                }

                parts.push(Part::Expression(parse_expression(template, open, close)?));
                i = close + 1;
                literal_start = i;
            }
            _ => i += 1,
        }
    }

    if literal_start < template.len() {
        parts.push(Part::Literal(template[literal_start..].to_owned()));
    }

    Ok(parts)
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn is_reserved(byte: u8) -> bool {
    matches!(
        byte,
        b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&'
            | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn encode(input: &str, reserved: bool) -> String {
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

        if is_unreserved(byte) || (reserved && is_reserved(byte)) {
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

fn binding<'a>(name: &str, vars: &'a [(&str, Value)]) -> Option<&'a Value> {
    vars.iter()
        .rev()
        .find(|(binding_name, _)| *binding_name == name)
        .map(|(_, value)| value)
}

fn take_prefix(value: &str, count: usize) -> String {
    value.chars().take(count).collect()
}

fn named_field(name: &str, value: String, operator: Operator) -> String {
    if operator == Operator::Semicolon && value.is_empty() {
        name.to_owned()
    } else {
        format!("{name}={value}")
    }
}

fn expand_expression(expression: &Expression, vars: &[(&str, Value)]) -> Result<String, Error> {
    let operator = expression.operator;
    let mut fields = Vec::new();

    for specifier in &expression.specifiers {
        let Some(value) = binding(&specifier.name, vars) else {
            continue;
        };

        if matches!(specifier.modifier, Some(Modifier::Prefix(_)))
            && !matches!(value, Value::Scalar(_))
        {
            return Err(Error::TypeMismatch { pos: specifier.pos });
        }

        if let Value::Map(entries) = value {
            let mut keys = Vec::new();
            for (key, _) in entries {
                if keys.iter().any(|existing: &&String| *existing == key) {
                    return Err(Error::DuplicateKey { pos: specifier.pos });
                }
                keys.push(key);
            }
        }

        match value {
            Value::Scalar(raw) => {
                let scalar = match specifier.modifier {
                    Some(Modifier::Prefix(count)) => take_prefix(raw, count),
                    _ => raw.clone(),
                };
                let encoded = encode(&scalar, operator.reserved());
                if operator.named_fields() {
                    fields.push(named_field(&specifier.name, encoded, operator));
                } else {
                    fields.push(encoded);
                }
            }
            Value::List(items) => {
                if items.is_empty() {
                    continue;
                }
                let exploded = matches!(specifier.modifier, Some(Modifier::Explode));
                if exploded {
                    for item in items {
                        let encoded = encode(item, operator.reserved());
                        if operator.named_fields() {
                            fields.push(named_field(&specifier.name, encoded, operator));
                        } else {
                            fields.push(encoded);
                        }
                    }
                } else {
                    let joined = items
                        .iter()
                        .map(|item| encode(item, operator.reserved()))
                        .collect::<Vec<_>>()
                        .join(",");
                    if operator.named_fields() {
                        fields.push(named_field(&specifier.name, joined, operator));
                    } else {
                        fields.push(joined);
                    }
                }
            }
            Value::Map(entries) => {
                if entries.is_empty() {
                    continue;
                }
                let exploded = matches!(specifier.modifier, Some(Modifier::Explode));
                if exploded {
                    for (key, value) in entries {
                        let encoded_key = encode(key, operator.reserved());
                        let encoded_value = encode(value, operator.reserved());
                        if operator == Operator::Semicolon && encoded_value.is_empty() {
                            fields.push(encoded_key);
                        } else {
                            fields.push(format!("{encoded_key}={encoded_value}"));
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
                        fields.push(named_field(&specifier.name, joined, operator));
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
        output.push(prefix);
    }
    output.push_str(&fields.join(&operator.separator().to_string()));
    Ok(output)
}

pub fn expand(template: &str, vars: &[(&str, Value)]) -> Result<String, Error> {
    let parts = parse_template(template)?;
    let mut output = String::new();

    for part in parts {
        match part {
            Part::Literal(literal) => output.push_str(&literal),
            Part::Expression(expression) => {
                output.push_str(&expand_expression(&expression, vars)?);
            }
        }
    }

    Ok(output)
}