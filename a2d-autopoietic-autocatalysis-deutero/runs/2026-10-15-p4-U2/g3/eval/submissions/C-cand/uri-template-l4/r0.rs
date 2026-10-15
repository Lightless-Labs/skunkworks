use std::collections::HashSet;

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
    Question,
    Ampersand,
}

impl Operator {
    fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            b'+' => Some(Self::Reserved),
            b'#' => Some(Self::Fragment),
            b'.' => Some(Self::Dot),
            b'/' => Some(Self::Slash),
            b';' => Some(Self::Semicolon),
            b'?' => Some(Self::Question),
            b'&' => Some(Self::Ampersand),
            _ => None,
        }
    }

    fn is_reserved(self) -> bool {
        matches!(self, Self::Reserved | Self::Fragment)
    }

    fn separator(self) -> char {
        match self {
            Self::Simple | Self::Reserved | Self::Fragment => ',',
            Self::Dot => '.',
            Self::Slash => '/',
            Self::Semicolon => ';',
            Self::Question | Self::Ampersand => '&',
        }
    }

    fn prefix(self) -> Option<char> {
        match self {
            Self::Simple | Self::Reserved => None,
            Self::Fragment => Some('#'),
            Self::Dot => Some('.'),
            Self::Slash => Some('/'),
            Self::Semicolon => Some(';'),
            Self::Question => Some('?'),
            Self::Ampersand => Some('&'),
        }
    }

    fn named_fields(self) -> bool {
        matches!(self, Self::Semicolon | Self::Question | Self::Ampersand)
    }
}

#[derive(Debug, Clone)]
enum Modifier {
    None,
    Explode,
    Prefix(usize),
}

#[derive(Debug, Clone)]
struct Specifier {
    name: String,
    pos: usize,
    modifier: Modifier,
}

#[derive(Debug, Clone)]
struct Expression {
    operator: Operator,
    specifiers: Vec<Specifier>,
}

#[derive(Debug, Clone)]
enum Segment {
    Literal(String),
    Expression(Expression),
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn parse_specifier(content: &str, start: usize, end: usize) -> Result<Specifier, Error> {
    if start == end {
        return Err(Error::EmptyVariable { pos: start });
    }

    let bytes = content.as_bytes();
    let modifier_index = (start..end).find(|&i| bytes[i] == b'*' || bytes[i] == b':');
    let name_end = modifier_index.unwrap_or(end);

    if name_end == start || !is_name_start(bytes[start]) {
        return Err(Error::InvalidName { pos: start });
    }

    for i in start + 1..name_end {
        if !is_name_continue(bytes[i]) {
            return Err(Error::InvalidName { pos: i });
        }
    }

    let modifier = if let Some(index) = modifier_index {
        let suffix = &content[index..end];
        if suffix == "*" {
            Modifier::Explode
        } else if let Some(digits) = suffix.strip_prefix(':') {
            let valid = (1..=4).contains(&digits.len())
                && digits.as_bytes()[0].is_ascii_digit()
                && digits.as_bytes()[0] != b'0'
                && digits.bytes().all(|b| b.is_ascii_digit());

            if !valid {
                return Err(Error::InvalidModifier { pos: index });
            }

            let value = digits.parse::<usize>().unwrap_or(0);
            if !(1..=9999).contains(&value) {
                return Err(Error::InvalidModifier { pos: index });
            }
            Modifier::Prefix(value)
        } else {
            return Err(Error::InvalidModifier { pos: index });
        }
    } else {
        Modifier::None
    };

    Ok(Specifier {
        name: content[start..name_end].to_string(),
        pos: start,
        modifier,
    })
}

fn parse_expression(
    content: &str,
    content_pos: usize,
    close_pos: usize,
) -> Result<Expression, Error> {
    let bytes = content.as_bytes();
    let (operator, body_start) = if let Some(&first) = bytes.first() {
        if let Some(op) = Operator::from_byte(first) {
            (op, 1)
        } else if matches!(first, b'=' | b'!' | b'@' | b'|' | b'$' | b'%' | b'^') {
            return Err(Error::UnsupportedOperator { pos: content_pos });
        } else {
            (Operator::Simple, 0)
        }
    } else {
        return Err(Error::EmptyExpression { pos: close_pos });
    };

    if body_start == bytes.len() {
        return Err(Error::EmptyExpression { pos: close_pos });
    }

    let mut specifiers = Vec::new();
    let mut seen = HashSet::new();
    let mut start = body_start;

    loop {
        let end = content[start..]
            .find(',')
            .map(|relative| start + relative)
            .unwrap_or(content.len());

        let specifier = parse_specifier(content, start, end)?;
        if seen.contains(&specifier.name) {
            return Err(Error::DuplicateVariable {
                pos: content_pos + start,
            });
        }
        seen.insert(specifier.name.clone());
        specifiers.push(specifier);

        if end == content.len() {
            break;
        }
        start = end + 1;
    }

    Ok(Expression {
        operator,
        specifiers,
    })
}

fn parse_template(template: &str) -> Result<Vec<Segment>, Error> {
    let bytes = template.as_bytes();
    let mut segments = Vec::new();
    let mut cursor = 0;
    let mut literal_start = 0;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'}' => return Err(Error::UnexpectedClose { pos: cursor }),
            b'{' => {
                if literal_start < cursor {
                    segments.push(Segment::Literal(
                        template[literal_start..cursor].to_string(),
                    ));
                }

                let open = cursor;
                let close = bytes[open + 1..]
                    .iter()
                    .position(|&byte| byte == b'}')
                    .map(|relative| open + 1 + relative)
                    .ok_or(Error::Unclosed { pos: open })?;

                if let Some(relative) = bytes[open + 1..close]
                    .iter()
                    .position(|&byte| byte == b'{')
                {
                    return Err(Error::NestedOpen {
                        pos: open + 1 + relative,
                    });
                }

                let content_pos = open + 1;
                let expression =
                    parse_expression(&template[content_pos..close], content_pos, close)?;
                segments.push(Segment::Expression(expression));
                cursor = close + 1;
                literal_start = cursor;
            }
            _ => cursor += 1,
        }
    }

    if literal_start < template.len() {
        segments.push(Segment::Literal(template[literal_start..].to_string()));
    }

    Ok(segments)
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

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'A' + value - 10) as char,
    }
}

fn encode(value: &str, reserved: bool) -> String {
    let bytes = value.as_bytes();
    let mut output = String::new();
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'%' && i + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                output.push('%');
                output.push(hex_digit(high));
                output.push(hex_digit(low));
                i += 3;
                continue;
            }
        }

        if is_unreserved(byte) || (reserved && is_reserved(byte)) {
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

fn select_value<'a>(name: &str, vars: &'a [(&str, Value)]) -> Option<&'a Value> {
    vars.iter()
        .rev()
        .find(|(binding_name, _)| *binding_name == name)
        .map(|(_, value)| value)
}

fn apply_prefix(value: &str, count: usize) -> String {
    value.chars().take(count).collect()
}

fn scalar_field(name: &str, value: &str, operator: Operator) -> String {
    let encoded = encode(value, operator.is_reserved());
    if operator.named_fields() {
        if operator == Operator::Semicolon && encoded.is_empty() {
            name.to_string()
        } else {
            format!("{name}={encoded}")
        }
    } else {
        encoded
    }
}

fn expand_expression(expression: &Expression, vars: &[(&str, Value)]) -> Result<String, Error> {
    let operator = expression.operator;
    let mut fields = Vec::new();

    for specifier in &expression.specifiers {
        let Some(value) = select_value(&specifier.name, vars) else {
            continue;
        };

        if matches!(specifier.modifier, Modifier::Prefix(_))
            && !matches!(value, Value::Scalar(_))
        {
            return Err(Error::TypeMismatch { pos: specifier.pos });
        }

        match value {
            Value::Scalar(text) => {
                let selected = match specifier.modifier {
                    Modifier::Prefix(count) => apply_prefix(text, count),
                    _ => text.clone(),
                };
                fields.push(scalar_field(&specifier.name, &selected, operator));
            }
            Value::List(items) => {
                if items.is_empty() {
                    continue;
                }

                if matches!(specifier.modifier, Modifier::Explode) {
                    for item in items {
                        fields.push(scalar_field(&specifier.name, item, operator));
                    }
                } else {
                    let joined = items
                        .iter()
                        .map(|item| encode(item, operator.is_reserved()))
                        .collect::<Vec<_>>()
                        .join(",");
                    if operator.named_fields() {
                        fields.push(format!("{}={joined}", specifier.name));
                    } else {
                        fields.push(joined);
                    }
                }
            }
            Value::Map(entries) => {
                let mut keys = HashSet::new();
                for (key, _) in entries {
                    if !keys.insert(key) {
                        return Err(Error::DuplicateKey { pos: specifier.pos });
                    }
                }

                if entries.is_empty() {
                    continue;
                }

                if matches!(specifier.modifier, Modifier::Explode) {
                    for (key, value) in entries {
                        let encoded_key = encode(key, operator.is_reserved());
                        let encoded_value = encode(value, operator.is_reserved());
                        let field = if operator == Operator::Semicolon && encoded_value.is_empty() {
                            encoded_key
                        } else {
                            format!("{encoded_key}={encoded_value}")
                        };
                        fields.push(field);
                    }
                } else {
                    let mut components = Vec::with_capacity(entries.len() * 2);
                    for (key, value) in entries {
                        components.push(encode(key, operator.is_reserved()));
                        components.push(encode(value, operator.is_reserved()));
                    }
                    let joined = components.join(",");
                    if operator.named_fields() {
                        fields.push(format!("{}={joined}", specifier.name));
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
    let segments = parse_template(template)?;
    let mut output = String::new();

    for segment in segments {
        match segment {
            Segment::Literal(text) => output.push_str(&text),
            Segment::Expression(expression) => {
                output.push_str(&expand_expression(&expression, vars)?);
            }
        }
    }

    Ok(output)
}

fn main() {}