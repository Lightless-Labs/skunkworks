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
    Reserved,
    Fragment,
    Dot,
    Slash,
    Semicolon,
    Query,
    Continue,
}

impl Operator {
    fn from_char(c: char) -> Option<Self> {
        Some(match c {
            '+' => Self::Reserved,
            '#' => Self::Fragment,
            '.' => Self::Dot,
            '/' => Self::Slash,
            ';' => Self::Semicolon,
            '?' => Self::Query,
            '&' => Self::Continue,
            _ => return None,
        })
    }

    fn reserved(self) -> bool {
        matches!(self, Self::Reserved | Self::Fragment)
    }

    fn named(self) -> bool {
        matches!(self, Self::Semicolon | Self::Query | Self::Continue)
    }

    fn separator(self) -> char {
        match self {
            Self::Simple | Self::Reserved | Self::Fragment => ',',
            Self::Dot => '.',
            Self::Slash => '/',
            Self::Semicolon => ';',
            Self::Query | Self::Continue => '&',
        }
    }

    fn prefix(self) -> Option<char> {
        match self {
            Self::Fragment => Some('#'),
            Self::Dot => Some('.'),
            Self::Slash => Some('/'),
            Self::Semicolon => Some(';'),
            Self::Query => Some('?'),
            Self::Continue => Some('&'),
            Self::Simple | Self::Reserved => None,
        }
    }
}

#[derive(Clone)]
struct Specifier<'a> {
    name: &'a str,
    modifier: Modifier,
    pos: usize,
}

#[derive(Clone, Copy)]
enum Modifier {
    None,
    Explode,
    Prefix(usize),
}

fn valid_name_byte(b: u8, first: bool) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || (!first && b.is_ascii_digit())
}

fn parse_expression<'a>(
    template: &'a str,
    start: usize,
    end: usize,
) -> Result<(Operator, Vec<Specifier<'a>>), Error> {
    let bytes = template.as_bytes();
    let mut content_start = start + 1;
    let operator = if content_start < end {
        match bytes[content_start] {
            b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&' => {
                let op = Operator::from_char(bytes[content_start] as char).unwrap();
                content_start += 1;
                op
            }
            b'=' | b',' | b'!' | b'@' | b'|' => {
                return Err(Error::UnsupportedOperator { pos: content_start });
            }
            _ => Operator::Simple,
        }
    } else {
        Operator::Simple
    };

    if content_start == end {
        return Err(Error::EmptyExpression { pos: end });
    }

    let mut specs = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    let mut part_start = content_start;

    loop {
        let part_end = bytes[part_start..end]
            .iter()
            .position(|&b| b == b',')
            .map(|n| part_start + n)
            .unwrap_or(end);

        if part_start == part_end {
            return Err(Error::EmptyVariable { pos: part_start });
        }

        let part = &template[part_start..part_end];
        let modifier_at = part
            .as_bytes()
            .iter()
            .position(|&b| b == b'*' || b == b':');
        let name_end = modifier_at.map(|n| part_start + n).unwrap_or(part_end);
        let name = &template[part_start..name_end];

        if name.is_empty() {
            return Err(Error::InvalidName { pos: part_start });
        }

        for (i, &b) in bytes[part_start..name_end].iter().enumerate() {
            if !valid_name_byte(b, i == 0) {
                return Err(Error::InvalidName {
                    pos: part_start + i,
                });
            }
        }

        let modifier = if let Some(offset) = modifier_at {
            let mpos = part_start + offset;
            let suffix = &template[mpos..part_end];
            if suffix == "*" {
                Modifier::Explode
            } else if suffix.starts_with(':') {
                let digits = &suffix[1..];
                let valid = !digits.is_empty()
                    && digits.len() <= 4
                    && digits.as_bytes()[0].is_ascii_digit()
                    && digits.as_bytes()[0] != b'0'
                    && digits.bytes().all(|b| b.is_ascii_digit());
                if !valid {
                    return Err(Error::InvalidModifier { pos: mpos });
                }
                let value = digits.parse::<usize>().unwrap_or(0);
                if !(1..=9999).contains(&value) {
                    return Err(Error::InvalidModifier { pos: mpos });
                }
                Modifier::Prefix(value)
            } else {
                return Err(Error::InvalidModifier { pos: mpos });
            }
        } else {
            Modifier::None
        };

        if seen.iter().any(|&prior| prior == name) {
            return Err(Error::DuplicateVariable { pos: part_start });
        }
        seen.push(name);
        specs.push(Specifier {
            name,
            modifier,
            pos: part_start,
        });

        if part_end == end {
            break;
        }
        part_start = part_end + 1;
    }

    Ok((operator, specs))
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
        let reserved_byte = reserved
            && matches!(
                b,
                b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&'
                    | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
            );
        if unreserved || reserved_byte {
            out.push(b as char);
            i += 1;
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
            i += 1;
        }
    }
    out
}

fn prefix_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn selected_value<'a>(name: &str, vars: &'a [(&str, Value)]) -> Option<&'a Value> {
    vars.iter()
        .rev()
        .find(|(binding, _)| *binding == name)
        .map(|(_, value)| value)
}

fn named_field(name: &str, value: String, op: Operator) -> String {
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

        if matches!(spec.modifier, Modifier::Prefix(_))
            && !matches!(value, Value::Scalar(_))
        {
            return Err(Error::TypeMismatch { pos: spec.pos });
        }

        match value {
            Value::Scalar(s) => {
                let text = match spec.modifier {
                    Modifier::Prefix(n) => prefix_chars(s, n),
                    Modifier::None | Modifier::Explode => s.clone(),
                };
                let encoded = encode(&text, op.reserved());
                fields.push(if op.named() {
                    named_field(spec.name, encoded, op)
                } else {
                    encoded
                });
            }
            Value::List(items) => {
                if items.is_empty() {
                    continue;
                }
                match spec.modifier {
                    Modifier::Explode => {
                        for item in items {
                            let encoded = encode(item, op.reserved());
                            fields.push(if op.named() {
                                named_field(spec.name, encoded, op)
                            } else {
                                encoded
                            });
                        }
                    }
                    Modifier::None => {
                        let joined = items
                            .iter()
                            .map(|item| encode(item, op.reserved()))
                            .collect::<Vec<_>>()
                            .join(",");
                        fields.push(if op.named() {
                            named_field(spec.name, joined, op)
                        } else {
                            joined
                        });
                    }
                    Modifier::Prefix(_) => unreachable!(),
                }
            }
            Value::Map(entries) => {
                let mut keys: Vec<&str> = Vec::new();
                for (key, _) in entries {
                    if keys.contains(&key.as_str()) {
                        return Err(Error::DuplicateKey { pos: spec.pos });
                    }
                    keys.push(key);
                }
                if entries.is_empty() {
                    continue;
                }

                match spec.modifier {
                    Modifier::Explode => {
                        for (key, value) in entries {
                            let encoded_key = encode(key, op.reserved());
                            let encoded_value = encode(value, op.reserved());
                            fields.push(if op == Operator::Semicolon && encoded_value.is_empty() {
                                encoded_key
                            } else {
                                format!("{encoded_key}={encoded_value}")
                            });
                        }
                    }
                    Modifier::None => {
                        let mut components = Vec::with_capacity(entries.len() * 2);
                        for (key, value) in entries {
                            components.push(encode(key, op.reserved()));
                            components.push(encode(value, op.reserved()));
                        }
                        let joined = components.join(",");
                        fields.push(if op.named() {
                            named_field(spec.name, joined, op)
                        } else {
                            joined
                        });
                    }
                    Modifier::Prefix(_) => unreachable!(),
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
    let mut result = String::new();
    let mut cursor = 0;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'}' => return Err(Error::UnexpectedClose { pos: cursor }),
            b'{' => {
                let start = cursor;
                let Some(relative_end) = bytes[cursor + 1..].iter().position(|&b| b == b'}') else {
                    return Err(Error::Unclosed { pos: start });
                };
                let end = cursor + 1 + relative_end;

                if let Some(relative_open) = bytes[start + 1..end].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: start + 1 + relative_open,
                    });
                }

                let (op, specs) = parse_expression(template, start, end)?;
                cursor = end + 1;
                let expanded = expand_expression(op, &specs, vars)?;
                result.push_str(&expanded);
            }
            _ => {
                let next = bytes[cursor..]
                    .iter()
                    .position(|&b| b == b'{' || b == b'}')
                    .map(|n| cursor + n)
                    .unwrap_or(bytes.len());
                result.push_str(&template[cursor..next]);
                cursor = next;
            }
        }
    }

    Ok(result)
}