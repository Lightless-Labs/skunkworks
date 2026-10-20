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
enum Op {
    Simple,
    Plus,
    Hash,
    Dot,
    Slash,
    Semi,
    Query,
    Amp,
}

#[derive(Clone, Copy)]
enum Mod {
    Plain,
    Explode,
    Prefix(usize),
}

struct Var<'a> {
    name: &'a str,
    modifier: Mod,
    pos: usize,
}

fn parse<'a>(body: &'a str, body_pos: usize, close: usize) -> Result<(Op, Vec<Var<'a>>), Error> {
    let mut idx = 0;
    let op = match body.as_bytes().first().copied() {
        Some(b'+') => { idx = 1; Op::Plus }
        Some(b'#') => { idx = 1; Op::Hash }
        Some(b'.') => { idx = 1; Op::Dot }
        Some(b'/') => { idx = 1; Op::Slash }
        Some(b';') => { idx = 1; Op::Semi }
        Some(b'?') => { idx = 1; Op::Query }
        Some(b'&') => { idx = 1; Op::Amp }
        _ => Op::Simple,
    };
    if idx == body.len() {
        return Err(Error::EmptyExpression { pos: close });
    }

    let rest = &body[idx..];
    let mut vars = Vec::new();
    let mut begin = 0;
    loop {
        let end = rest[begin..].find(',').map(|n| begin + n).unwrap_or(rest.len());
        let token = &rest[begin..end];
        let pos = body_pos + idx + begin;
        if token.is_empty() {
            return Err(Error::EmptyVariable { pos });
        }
        let m = token.bytes().position(|b| b == b'*' || b == b':');
        let name_end = m.unwrap_or(token.len());
        let name = &token[..name_end];
        if name.is_empty() {
            return Err(Error::InvalidName { pos });
        }
        let nb = name.as_bytes();
        if !(nb[0].is_ascii_alphabetic() || nb[0] == b'_') {
            return Err(Error::InvalidName { pos });
        }
        for (i, &b) in nb.iter().enumerate().skip(1) {
            if !(b.is_ascii_alphanumeric() || b == b'_') {
                return Err(Error::InvalidName { pos: pos + i });
            }
        }
        let modifier = if let Some(m) = m {
            let suffix = &token[m..];
            if suffix == "*" {
                Mod::Explode
            } else if suffix.starts_with(':') {
                let digits = &suffix[1..];
                if !(1..=4).contains(&digits.len())
                    || digits.as_bytes()[0] == b'0'
                    || !digits.bytes().all(|b| b.is_ascii_digit())
                {
                    return Err(Error::InvalidModifier { pos: pos + m });
                }
                let n = digits.parse::<usize>().unwrap_or(0);
                if n == 0 || n > 9999 {
                    return Err(Error::InvalidModifier { pos: pos + m });
                }
                Mod::Prefix(n)
            } else {
                return Err(Error::InvalidModifier { pos: pos + m });
            }
        } else {
            Mod::Plain
        };
        if vars.iter().any(|v: &Var<'_>| v.name == name) {
            return Err(Error::DuplicateVariable { pos });
        }
        vars.push(Var { name, modifier, pos });
        if end == rest.len() {
            break;
        }
        begin = end + 1;
    }
    Ok((op, vars))
}

fn encoded(s: &str, reserved: bool) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let b = s.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'%' && i + 2 < b.len() && b[i + 1].is_ascii_hexdigit() && b[i + 2].is_ascii_hexdigit() {
            out.push('%');
            out.push((b[i + 1] as char).to_ascii_uppercase());
            out.push((b[i + 2] as char).to_ascii_uppercase());
            i += 3;
        } else if c.is_ascii_alphanumeric() || matches!(c, b'-' | b'.' | b'_' | b'~')
            || (reserved && matches!(c, b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='))
        {
            out.push(c as char);
            i += 1;
        } else {
            out.push('%');
            out.push(HEX[(c >> 4) as usize] as char);
            out.push(HEX[(c & 15) as usize] as char);
            i += 1;
        }
    }
    out
}

fn parameter(op: Op, name: &str, value: String) -> String {
    if op == Op::Semi && value.is_empty() {
        name.to_owned()
    } else {
        format!("{name}={value}")
    }
}

fn render(op: Op, vars_in_expr: &[Var<'_>], bindings: &[(&str, Value)]) -> Result<String, Error> {
    let reserved = matches!(op, Op::Plus | Op::Hash);
    let mut fields = Vec::new();
    for var in vars_in_expr {
        let selected = bindings.iter().rev().find(|(n, _)| *n == var.name).map(|(_, v)| v);
        let Some(value) = selected else { continue };
        if matches!(var.modifier, Mod::Prefix(_)) && !matches!(value, Value::Scalar(_)) {
            return Err(Error::TypeMismatch { pos: var.pos });
        }
        match value {
            Value::Scalar(s) => {
                let s = match var.modifier {
                    Mod::Prefix(n) => s.chars().take(n).collect::<String>(),
                    _ => s.clone(),
                };
                let value = encoded(&s, reserved);
                fields.push(if matches!(op, Op::Semi | Op::Query | Op::Amp) {
                    parameter(op, var.name, value)
                } else { value });
            }
            Value::List(list) => {
                if list.is_empty() { continue; }
                if matches!(var.modifier, Mod::Explode) {
                    for item in list {
                        let item = encoded(item, reserved);
                        fields.push(if matches!(op, Op::Semi | Op::Query | Op::Amp) {
                            parameter(op, var.name, item)
                        } else { item });
                    }
                } else {
                    let value = list.iter().map(|s| encoded(s, reserved)).collect::<Vec<_>>().join(",");
                    fields.push(if matches!(op, Op::Semi | Op::Query | Op::Amp) {
                        parameter(op, var.name, value)
                    } else { value });
                }
            }
            Value::Map(map) => {
                if map.is_empty() { continue; }
                for i in 0..map.len() {
                    if map[..i].iter().any(|(key, _)| *key == map[i].0) {
                        return Err(Error::DuplicateKey { pos: var.pos });
                    }
                }
                if matches!(var.modifier, Mod::Explode) {
                    for (key, value) in map {
                        let key = encoded(key, reserved);
                        let value = encoded(value, reserved);
                        fields.push(if op == Op::Semi && value.is_empty() {
                            key
                        } else {
                            format!("{key}={value}")
                        });
                    }
                } else {
                    let mut flat = Vec::with_capacity(map.len() * 2);
                    for (key, value) in map {
                        flat.push(encoded(key, reserved));
                        flat.push(encoded(value, reserved));
                    }
                    let value = flat.join(",");
                    fields.push(if matches!(op, Op::Semi | Op::Query | Op::Amp) {
                        parameter(op, var.name, value)
                    } else { value });
                }
            }
        }
    }
    if fields.is_empty() { return Ok(String::new()); }
    let (prefix, sep) = match op {
        Op::Simple | Op::Plus => ("", ","),
        Op::Hash => ("#", ","),
        Op::Dot => (".", "."),
        Op::Slash => ("/", "/"),
        Op::Semi => (";", ";"),
        Op::Query => ("?", "&"),
        Op::Amp => ("&", "&"),
    };
    Ok(format!("{prefix}{}", fields.join(sep)))
}

pub fn expand(template: &str, vars: &[(&str, Value)]) -> Result<String, Error> {
    let mut parsed: Vec<(usize, usize, Op, Vec<Var<'_>>)> = Vec::new();
    let b = template.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                let open = i;
                let Some(rel) = template[open + 1..].find('}') else {
                    return Err(Error::Unclosed { pos: open });
                };
                let close = open + 1 + rel;
                if let Some(nested) = template[open + 1..close].find('{') {
                    return Err(Error::NestedOpen { pos: open + 1 + nested });
                }
                let (op, specs) = parse(&template[open + 1..close], open + 1, close)?;
                parsed.push((open, close + 1, op, specs));
                i = close + 1;
            }
            _ => i += 1,
        }
    }

    let mut out = String::new();
    let mut cursor = 0;
    for (start, end, op, specs) in parsed {
        out.push_str(&template[cursor..start]);
        out.push_str(&render(op, &specs, vars)?);
        cursor = end;
    }
    out.push_str(&template[cursor..]);
    Ok(out)
}
