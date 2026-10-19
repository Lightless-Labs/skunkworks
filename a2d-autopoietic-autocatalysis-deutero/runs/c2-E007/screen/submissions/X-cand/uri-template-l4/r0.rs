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

#[derive(Clone)]
struct Spec {
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

struct Expr {
    start: usize,
    end: usize,
    operator: Option<char>,
    specs: Vec<Spec>,
}

fn name_char_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn name_char_rest(b: u8) -> bool {
    name_char_start(b) || b.is_ascii_digit()
}

fn parse_expression(template: &str, start: usize, end: usize) -> Result<Expr, Error> {
    let bytes = template.as_bytes();
    let mut content_start = start + 1;
    let mut operator = None;

    if content_start < end && b"+#./;?&".contains(&bytes[content_start]) {
        let op = bytes[content_start] as char;
        if op == '&' || op == '+' || op == '#' || op == '.' || op == '/' || op == ';' || op == '?' {
            operator = Some(op);
            content_start += 1;
        }
    }

    if content_start == end {
        return Err(Error::EmptyExpression { pos: end });
    }

    let mut specs = Vec::new();
    let mut seen = Vec::<String>::new();
    let mut piece_start = content_start;

    loop {
        let piece_end = bytes[piece_start..end]
            .iter()
            .position(|&b| b == b',')
            .map(|i| piece_start + i)
            .unwrap_or(end);

        if piece_start == piece_end {
            return Err(Error::EmptyVariable { pos: piece_start });
        }

        let piece = &bytes[piece_start..piece_end];
        let modifier_rel = piece.iter().position(|&b| b == b'*' || b == b':');
        let name_end = modifier_rel.map(|i| piece_start + i).unwrap_or(piece_end);

        if name_end == piece_start {
            return Err(Error::InvalidName { pos: piece_start });
        }

        if !name_char_start(bytes[piece_start]) {
            return Err(Error::InvalidName { pos: piece_start });
        }
        for idx in piece_start + 1..name_end {
            if !name_char_rest(bytes[idx]) {
                return Err(Error::InvalidName { pos: idx });
            }
        }

        let name = template[piece_start..name_end].to_string();
        let modifier = if let Some(rel) = modifier_rel {
            let mod_start = piece_start + rel;
            let suffix = &bytes[mod_start..piece_end];
            if suffix == b"*" {
                Modifier::Explode
            } else if suffix.len() >= 2
                && suffix[0] == b':'
                && suffix.len() <= 5
                && suffix[1].is_ascii_digit()
                && suffix[1] != b'0'
                && suffix[1..].iter().all(u8::is_ascii_digit)
            {
                let n = std::str::from_utf8(&suffix[1..])
                    .ok()
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(0);
                if (1..=9999).contains(&n) {
                    Modifier::Prefix(n)
                } else {
                    return Err(Error::InvalidModifier { pos: mod_start });
                }
            } else {
                return Err(Error::InvalidModifier { pos: mod_start });
            }
        } else {
            Modifier::None
        };

        if seen.iter().any(|existing| existing == &name) {
            return Err(Error::DuplicateVariable { pos: piece_start });
        }
        seen.push(name.clone());
        specs.push(Spec {
            name,
            start: piece_start,
            modifier,
        });

        if piece_end == end {
            break;
        }
        piece_start = piece_end + 1;
    }

    Ok(Expr {
        start,
        end,
        operator,
        specs,
    })
}

fn validate_template(template: &str) -> Result<Vec<Expr>, Error> {
    let bytes = template.as_bytes();
    let mut exprs = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                let start = i;
                let close = bytes[start + 1..]
                    .iter()
                    .position(|&b| b == b'}')
                    .map(|p| start + 1 + p);
                let Some(end) = close else {
                    return Err(Error::Unclosed { pos: start });
                };
                if let Some(inner) = bytes[start + 1..end].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: start + 1 + inner,
                    });
                }
                exprs.push(parse_expression(template, start, end)?);
                i = end + 1;
            }
            _ => i += 1,
        }
    }
    Ok(exprs)
}

fn safe_byte(b: u8, reserved: bool) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~')
        || (reserved
            && matches!(
                b,
                b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&'
                    | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
            ))
}

fn encode(s: &str, reserved: bool) -> String {
    let b = s.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() && b[i + 1].is_ascii_hexdigit() && b[i + 2].is_ascii_hexdigit() {
            out.push('%');
            out.push((b[i + 1] as char).to_ascii_uppercase());
            out.push((b[i + 2] as char).to_ascii_uppercase());
            i += 3;
        } else if safe_byte(b[i], reserved) {
            out.push(b[i] as char);
            i += 1;
        } else {
            out.push('%');
            out.push(HEX[(b[i] >> 4) as usize] as char);
            out.push(HEX[(b[i] & 0x0f) as usize] as char);
            i += 1;
        }
    }
    out
}

fn lookup<'a>(name: &str, vars: &'a [(&str, Value)]) -> Option<&'a Value> {
    vars.iter().rev().find(|(n, _)| *n == name).map(|(_, v)| v)
}

fn prefix_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn named_field(name: &str, value: String, op: char) -> String {
    if op == ';' && value.is_empty() {
        name.to_string()
    } else {
        format!("{name}={value}")
    }
}

fn expand_expr(expr: &Expr, vars: &[(&str, Value)]) -> Result<String, Error> {
    let op = expr.operator.unwrap_or('\0');
    let reserved = op == '+' || op == '#';
    let mut fields = Vec::<String>::new();

    for spec in &expr.specs {
        let Some(value) = lookup(&spec.name, vars) else {
            continue;
        };

        if matches!(spec.modifier, Modifier::Prefix(_)) && !matches!(value, Value::Scalar(_)) {
            return Err(Error::TypeMismatch { pos: spec.start });
        }

        match value {
            Value::Scalar(s) => {
                let selected = match spec.modifier {
                    Modifier::Prefix(n) => prefix_chars(s, n),
                    _ => s.clone(),
                };
                let encoded = encode(&selected, reserved);
                fields.push(if matches!(op, ';' | '?' | '&') {
                    named_field(&spec.name, encoded, op)
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
                            let encoded = encode(item, reserved);
                            fields.push(if matches!(op, ';' | '?' | '&') {
                                named_field(&spec.name, encoded, op)
                            } else {
                                encoded
                            });
                        }
                    }
                    _ => {
                        let joined = items
                            .iter()
                            .map(|item| encode(item, reserved))
                            .collect::<Vec<_>>()
                            .join(",");
                        fields.push(if matches!(op, ';' | '?' | '&') {
                            named_field(&spec.name, joined, op)
                        } else {
                            joined
                        });
                    }
                }
            }
            Value::Map(entries) => {
                if entries.is_empty() {
                    continue;
                }
                for i in 0..entries.len() {
                    if entries[..i].iter().any(|(key, _)| key == &entries[i].0) {
                        return Err(Error::DuplicateKey { pos: spec.start });
                    }
                }
                match spec.modifier {
                    Modifier::Explode => {
                        for (key, val) in entries {
                            let encoded_key = encode(key, reserved);
                            let encoded_val = encode(val, reserved);
                            fields.push(if op == ';' && encoded_val.is_empty() {
                                encoded_key
                            } else {
                                format!("{encoded_key}={encoded_val}")
                            });
                        }
                    }
                    _ => {
                        let joined = entries
                            .iter()
                            .flat_map(|(key, val)| [encode(key, reserved), encode(val, reserved)])
                            .collect::<Vec<_>>()
                            .join(",");
                        fields.push(if matches!(op, ';' | '?' | '&') {
                            named_field(&spec.name, joined, op)
                        } else {
                            joined
                        });
                    }
                }
            }
        }
    }

    if fields.is_empty() {
        return Ok(String::new());
    }

    let (prefix, separator) = match op {
        '+' => ("", ","),
        '#' => ("#", ","),
        '.' => (".", "."),
        '/' => ("/", "/"),
        ';' => (";", ";"),
        '?' => ("?", "&"),
        '&' => ("&", "&"),
        _ => ("", ","),
    };
    Ok(format!("{prefix}{}", fields.join(separator)))
}

pub fn expand(template: &str, vars: &[(&str, Value)]) -> Result<String, Error> {
    let exprs = validate_template(template)?;
    let mut out = String::new();
    let mut cursor = 0;

    for expr in &exprs {
        out.push_str(&template[cursor..expr.start]);
        out.push_str(&expand_expr(expr, vars)?);
        cursor = expr.end + 1;
    }
    out.push_str(&template[cursor..]);
    Ok(out)
}