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
    Question,
    Ampersand,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Modifier {
    None,
    Explode,
    Prefix(usize),
}

struct Specifier<'a> {
    name: &'a str,
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
    b.is_ascii_alphanumeric() || b == b'_'
}

fn parse_expression<'a>(
    template: &'a str,
    content_start: usize,
    content_end: usize,
) -> Result<Expression<'a>, Error> {
    let bytes = template.as_bytes();
    let mut i = content_start;
    let operator = if i < content_end {
        match bytes[i] {
            b'+' => { i += 1; Operator::Reserved }
            b'#' => { i += 1; Operator::Fragment }
            b'.' => { i += 1; Operator::Dot }
            b'/' => { i += 1; Operator::Slash }
            b';' => { i += 1; Operator::Semicolon }
            b'?' => { i += 1; Operator::Question }
            b'&' => { i += 1; Operator::Ampersand }
            b'=' | b',' | b'!' | b'@' | b'|' => {
                return Err(Error::UnsupportedOperator { pos: i });
            }
            _ => Operator::Simple,
        }
    } else {
        Operator::Simple
    };

    if i == content_end {
        return Err(Error::EmptyExpression { pos: content_end });
    }

    let mut specs = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut start = i;

    loop {
        let mut end = start;
        while end < content_end && bytes[end] != b',' {
            end += 1;
        }
        if start == end {
            return Err(Error::EmptyVariable { pos: start });
        }

        let mut modifier_at = end;
        for p in start..end {
            if bytes[p] == b'*' || bytes[p] == b':' {
                modifier_at = p;
                break;
            }
        }

        if modifier_at == start || !is_name_start(bytes[start]) {
            return Err(Error::InvalidName { pos: start });
        }
        for p in start + 1..modifier_at {
            if !is_name_continue(bytes[p]) {
                return Err(Error::InvalidName { pos: p });
            }
        }

        let modifier = if modifier_at == end {
            Modifier::None
        } else if bytes[modifier_at] == b'*' && modifier_at + 1 == end {
            Modifier::Explode
        } else if bytes[modifier_at] == b':' {
            let digits = &bytes[modifier_at + 1..end];
            let valid = (1..=4).contains(&digits.len())
                && digits[0].is_ascii_digit()
                && digits[0] != b'0'
                && digits.iter().all(u8::is_ascii_digit);
            if !valid {
                return Err(Error::InvalidModifier { pos: modifier_at });
            }
            let mut n = 0usize;
            for &d in digits {
                n = n * 10 + (d - b'0') as usize;
            }
            if n > 9999 {
                return Err(Error::InvalidModifier { pos: modifier_at });
            }
            Modifier::Prefix(n)
        } else {
            return Err(Error::InvalidModifier { pos: modifier_at });
        };

        let name = &template[start..modifier_at];
        if !seen.insert(name) {
            return Err(Error::DuplicateVariable { pos: start });
        }
        specs.push(Specifier { name, start, modifier });

        if end == content_end {
            break;
        }
        start = end + 1;
        if start == content_end {
            return Err(Error::EmptyVariable { pos: start });
        }
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

fn encode(input: &str, reserved: bool) -> String {
    let bytes = input.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' && i + 2 < bytes.len() {
            if let (Some(h1), Some(h2)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                out.push('%');
                out.push(char::from(if h1 < 10 { b'0' + h1 } else { b'A' + h1 - 10 }));
                out.push(char::from(if h2 < 10 { b'0' + h2 } else { b'A' + h2 - 10 }));
                i += 3;
                continue;
            }
        }
        let unreserved = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~');
        let allowed_reserved = reserved
            && matches!(
                b,
                b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&'
                    | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
            );
        if unreserved || allowed_reserved {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push(char::from(b"0123456789ABCDEF"[(b >> 4) as usize]));
            out.push(char::from(b"0123456789ABCDEF"[(b & 15) as usize]));
        }
        i += 1;
    }
    out
}

fn prefix_chars(value: &str, n: usize) -> String {
    value.chars().take(n).collect()
}

fn selected_value<'a>(name: &str, vars: &'a [(&str, Value)]) -> Option<&'a Value> {
    vars.iter().rev().find(|(key, _)| *key == name).map(|(_, value)| value)
}

fn wrap_named(name: &str, value: String, op: Operator) -> String {
    match op {
        Operator::Semicolon => {
            if value.is_empty() { name.to_string() } else { format!("{name}={value}") }
        }
        Operator::Question | Operator::Ampersand => format!("{name}={value}"),
        _ => value,
    }
}

fn expand_expression(expr: &Expression<'_>, vars: &[(&str, Value)]) -> Result<String, Error> {
    let reserved = matches!(expr.operator, Operator::Reserved | Operator::Fragment);
    let (separator, prefix) = match expr.operator {
        Operator::Simple | Operator::Reserved => (",", ""),
        Operator::Fragment => (",", "#"),
        Operator::Dot => (".", "."),
        Operator::Slash => ("/", "/"),
        Operator::Semicolon => (";", ";"),
        Operator::Question => ("&", "?"),
        Operator::Ampersand => ("&", "&"),
    };

    let mut fields = Vec::new();

    for spec in &expr.specs {
        let Some(value) = selected_value(spec.name, vars) else {
            continue;
        };

        if matches!(spec.modifier, Modifier::Prefix(_)) {
            match value {
                Value::Scalar(_) => {}
                Value::List(_) | Value::Map(_) => {
                    return Err(Error::TypeMismatch { pos: spec.start });
                }
            }
        }

        match value {
            Value::Scalar(s) => {
                let text = match spec.modifier {
                    Modifier::Prefix(n) => prefix_chars(s, n),
                    _ => s.clone(),
                };
                let encoded = encode(&text, reserved);
                fields.push(wrap_named(spec.name, encoded, expr.operator));
            }
            Value::List(items) => {
                if items.is_empty() {
                    continue;
                }
                match spec.modifier {
                    Modifier::Explode => {
                        for item in items {
                            fields.push(wrap_named(spec.name, encode(item, reserved), expr.operator));
                        }
                    }
                    _ => {
                        let joined = items
                            .iter()
                            .map(|item| encode(item, reserved))
                            .collect::<Vec<_>>()
                            .join(",");
                        fields.push(wrap_named(spec.name, joined, expr.operator));
                    }
                }
            }
            Value::Map(entries) => {
                let mut keys = std::collections::HashSet::new();
                for (key, _) in entries {
                    if !keys.insert(key) {
                        return Err(Error::DuplicateKey { pos: spec.start });
                    }
                }
                if entries.is_empty() {
                    continue;
                }

                match spec.modifier {
                    Modifier::Explode => {
                        for (key, value) in entries {
                            let k = encode(key, reserved);
                            let v = encode(value, reserved);
                            if expr.operator == Operator::Semicolon && v.is_empty() {
                                fields.push(k);
                            } else {
                                fields.push(format!("{k}={v}"));
                            }
                        }
                    }
                    _ => {
                        let mut components = Vec::with_capacity(entries.len() * 2);
                        for (key, value) in entries {
                            components.push(encode(key, reserved));
                            components.push(encode(value, reserved));
                        }
                        let joined = components.join(",");
                        fields.push(wrap_named(spec.name, joined, expr.operator));
                    }
                }
            }
        }
    }

    if fields.is_empty() {
        Ok(String::new())
    } else {
        Ok(format!("{prefix}{}", fields.join(separator)))
    }
}

pub fn expand(template: &str, vars: &[(&str, Value)]) -> Result<String, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut literals = Vec::new();
    let mut cursor = 0;
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                let open = i;
                let mut close = i + 1;
                while close < bytes.len() && bytes[close] != b'}' {
                    close += 1;
                }
                if close == bytes.len() {
                    return Err(Error::Unclosed { pos: open });
                }
                if let Some(inner) = (open + 1..close).find(|&p| bytes[p] == b'{') {
                    return Err(Error::NestedOpen { pos: inner });
                }
                literals.push(&template[cursor..open]);
                let expr = parse_expression(template, open + 1, close)?;
                expressions.push(expr);
                cursor = close + 1;
                i = close + 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    // Syntax is validated for all expressions before any values are inspected.
    let mut output = String::new();
    let mut literal_index = 0;
    let mut scan = 0;
    for expr in &expressions {
        let open = template[scan..].find('{').map(|p| scan + p).unwrap_or(scan);
        output.push_str(&template[scan..open]);
        output.push_str(&expand_expression(expr, vars)?);
        let close = template[open + 1..].find('}').map(|p| open + 1 + p).unwrap_or(open + 1);
        scan = close + 1;
        literal_index += 1;
    }
    output.push_str(&template[scan..]);
    let _ = (&literals, literal_index, cursor);
    Ok(output)
}

fn main() {}