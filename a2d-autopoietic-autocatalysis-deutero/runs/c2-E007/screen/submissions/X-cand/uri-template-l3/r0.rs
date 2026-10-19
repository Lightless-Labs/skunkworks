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
struct Specifier<'a> {
    name: &'a str,
    start: usize,
}

impl Operator {
    fn parse(byte: u8) -> Option<Self> {
        match byte {
            b'+' => Some(Self::Reserved),
            b'#' => Some(Self::Fragment),
            b'.' => Some(Self::Dot),
            b'/' => Some(Self::Slash),
            b';' => Some(Self::Semicolon),
            b'?' => Some(Self::Query),
            b'&' => Some(Self::Continuation),
            _ => None,
        }
    }

    fn separator(self) -> char {
        match self {
            Self::Simple | Self::Reserved | Self::Fragment => ',',
            Self::Dot => '.',
            Self::Slash => '/',
            Self::Semicolon => ';',
            Self::Query | Self::Continuation => '&',
        }
    }

    fn prefix(self) -> Option<char> {
        match self {
            Self::Simple | Self::Reserved => None,
            Self::Fragment => Some('#'),
            Self::Dot => Some('.'),
            Self::Slash => Some('/'),
            Self::Semicolon => Some(';'),
            Self::Query => Some('?'),
            Self::Continuation => Some('&'),
        }
    }

    fn named_fields(self) -> bool {
        matches!(self, Self::Semicolon | Self::Query | Self::Continuation)
    }

    fn omit_equals_for_empty(self) -> bool {
        matches!(self, Self::Semicolon)
    }

    fn allows_reserved(self) -> bool {
        matches!(self, Self::Reserved | Self::Fragment)
    }
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn validate_expression<'a>(
    content: &'a str,
    content_pos: usize,
    close_pos: usize,
) -> Result<(Operator, Vec<Specifier<'a>>), Error> {
    let bytes = content.as_bytes();
    let (operator, offset) = if let Some(&first) = bytes.first() {
        if let Some(op) = Operator::parse(first) {
            (op, 1)
        } else {
            (Operator::Simple, 0)
        }
    } else {
        return Err(Error::EmptyExpression { pos: close_pos });
    };

    if offset == bytes.len() {
        return Err(Error::EmptyExpression { pos: close_pos });
    }

    let mut specifiers = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    let mut start = offset;

    for end in (offset..=bytes.len()).filter(|&i| i == bytes.len() || bytes[i] == b',') {
        if start == end {
            return Err(Error::EmptyVariable {
                pos: content_pos + start,
            });
        }

        let name_bytes = &bytes[start..end];
        if !is_name_start(name_bytes[0]) {
            return Err(Error::InvalidName {
                pos: content_pos + start,
            });
        }
        for (i, &byte) in name_bytes.iter().enumerate().skip(1) {
            if !is_name_continue(byte) {
                return Err(Error::InvalidName {
                    pos: content_pos + start + i,
                });
            }
        }

        let name = &content[start..end];
        if seen.contains(&name) {
            return Err(Error::DuplicateVariable {
                pos: content_pos + start,
            });
        }
        seen.push(name);
        specifiers.push(Specifier {
            name,
            start: content_pos + start,
        });
        start = end + 1;
    }

    Ok((operator, specifiers))
}

fn lookup<'a>(name: &str, vars: &'a [(&str, &str)]) -> Option<&'a str> {
    vars.iter()
        .rev()
        .find(|(binding, _)| *binding == name)
        .map(|(_, value)| *value)
}

fn is_reserved(byte: u8) -> bool {
    matches!(
        byte,
        b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&' | b'\'' | b'('
            | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

fn encode(value: &str, allow_reserved: bool) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = value.as_bytes();
    let mut out = String::new();
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'%'
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

        let unreserved =
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~');
        if unreserved || (allow_reserved && is_reserved(byte)) {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
        i += 1;
    }

    out
}

fn expand_expression(
    operator: Operator,
    specifiers: &[Specifier<'_>],
    vars: &[(&str, &str)],
) -> String {
    let mut fields = Vec::new();

    for specifier in specifiers {
        let _ = specifier.start;
        if let Some(value) = lookup(specifier.name, vars) {
            let encoded = encode(value, operator.allows_reserved());
            if operator.named_fields() {
                if operator.omit_equals_for_empty() && encoded.is_empty() {
                    fields.push(specifier.name.to_owned());
                } else {
                    fields.push(format!("{}={}", specifier.name, encoded));
                }
            } else {
                fields.push(encoded);
            }
        }
    }

    if fields.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    if let Some(prefix) = operator.prefix() {
        out.push(prefix);
    }
    for (i, field) in fields.iter().enumerate() {
        if i != 0 {
            out.push(operator.separator());
        }
        out.push_str(field);
    }
    out
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let bytes = template.as_bytes();
    let mut validated = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                let open = i;
                let close = match bytes[i + 1..].iter().position(|&byte| byte == b'}') {
                    Some(offset) => i + 1 + offset,
                    None => return Err(Error::Unclosed { pos: open }),
                };
                if let Some(offset) = bytes[i + 1..close].iter().position(|&byte| byte == b'{') {
                    return Err(Error::NestedOpen {
                        pos: i + 1 + offset,
                    });
                }
                let content_start = open + 1;
                let content = &template[content_start..close];
                let (operator, specifiers) =
                    validate_expression(content, content_start, close)?;
                validated.push((open, close + 1, operator, specifiers));
                i = close + 1;
            }
            _ => i += 1,
        }
    }

    let mut out = String::new();
    let mut cursor = 0;
    for (start, end, operator, specifiers) in validated {
        out.push_str(&template[cursor..start]);
        out.push_str(&expand_expression(operator, &specifiers, vars));
        cursor = end;
    }
    out.push_str(&template[cursor..]);
    Ok(out)
}