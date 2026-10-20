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

#[derive(Clone)]
struct Expression {
    start: usize,
    end: usize,
    operator: Option<u8>,
    names: Vec<(usize, usize)>,
}

fn name_first(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn name_rest(byte: u8) -> bool {
    name_first(byte) || byte.is_ascii_digit()
}

fn parse(template: &str) -> Result<Vec<Expression>, Error> {
    let b = template.as_bytes();
    let mut result = Vec::new();
    let mut pos = 0;

    while pos < b.len() {
        if b[pos] == b'}' {
            return Err(Error::UnexpectedClose { pos });
        }
        if b[pos] != b'{' {
            pos += 1;
            continue;
        }

        let start = pos;
        let end = match b[start + 1..].iter().position(|&c| c == b'}') {
            Some(n) => start + 1 + n,
            None => return Err(Error::Unclosed { pos: start }),
        };
        if let Some(n) = b[start + 1..end].iter().position(|&c| c == b'{') {
            return Err(Error::NestedOpen { pos: start + 1 + n });
        }

        let mut content = start + 1;
        let mut operator = None;
        if content < end && matches!(b[content], b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&') {
            operator = Some(b[content]);
            content += 1;
        }
        if content == end {
            return Err(Error::EmptyExpression { pos: end });
        }

        let mut names = Vec::new();
        let mut seen: Vec<&str> = Vec::new();
        let mut item = content;
        loop {
            let item_end = b[item..end]
                .iter()
                .position(|&c| c == b',')
                .map(|n| item + n)
                .unwrap_or(end);
            if item == item_end {
                return Err(Error::EmptyVariable { pos: item });
            }

            let part = &b[item..item_end];
            if !name_first(part[0]) {
                return Err(Error::InvalidName { pos: item });
            }
            if let Some(n) = part.iter().enumerate().skip(1).find_map(|(n, &c)| {
                if name_rest(c) { None } else { Some(n) }
            }) {
                return Err(Error::InvalidName { pos: item + n });
            }

            let name = &template[item..item_end];
            if seen.contains(&name) {
                return Err(Error::DuplicateVariable { pos: item });
            }
            seen.push(name);
            names.push((item, item_end));

            if item_end == end {
                break;
            }
            item = item_end + 1;
        }

        result.push(Expression {
            start,
            end,
            operator,
            names,
        });
        pos = end + 1;
    }

    Ok(result)
}

fn hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

fn encode_value(value: &str, allow_reserved: bool) -> String {
    let b = value.as_bytes();
    let mut result = String::new();
    let mut i = 0;
    const H: &[u8; 16] = b"0123456789ABCDEF";

    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(x), Some(y)) = (hex(b[i + 1]), hex(b[i + 2])) {
                result.push('%');
                result.push(H[x as usize] as char);
                result.push(H[y as usize] as char);
                i += 3;
                continue;
            }
        }

        let c = b[i];
        let reserved = matches!(
            c,
            b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&'
                | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
        );
        if c.is_ascii_alphanumeric() || matches!(c, b'-' | b'.' | b'_' | b'~') || (allow_reserved && reserved) {
            result.push(c as char);
        } else {
            result.push('%');
            result.push(H[(c >> 4) as usize] as char);
            result.push(H[(c & 15) as usize] as char);
        }
        i += 1;
    }
    result
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let expressions = parse(template)?;
    let b = template.as_bytes();
    let mut output = String::new();
    let mut cursor = 0;

    for expression in expressions {
        output.push_str(&template[cursor..expression.start]);
        let mut fields = Vec::new();

        for (start, end) in expression.names {
            let name = &template[start..end];
            if let Some((_, value)) = vars.iter().rev().find(|(key, _)| *key == name) {
                let encoded = encode_value(value, matches!(expression.operator, Some(b'+') | Some(b'#')));
                let field = match expression.operator {
                    Some(b';') if encoded.is_empty() => name.to_owned(),
                    Some(b';' | b'?' | b'&') => format!("{name}={encoded}"),
                    _ => encoded,
                };
                fields.push(field);
            }
        }

        if !fields.is_empty() {
            let (prefix, separator) = match expression.operator {
                Some(b'#') => ("#", ','),
                Some(b'.') => (".", '.'),
                Some(b'/') => ("/", '/'),
                Some(b';') => (";", ';'),
                Some(b'?') => ("?", '&'),
                Some(b'&') => ("&", '&'),
                _ => ("", ','),
            };
            output.push_str(prefix);
            for (i, field) in fields.iter().enumerate() {
                if i > 0 {
                    output.push(separator);
                }
                output.push_str(field);
            }
        }

        cursor = expression.end + 1;
        let _ = b;
    }

    output.push_str(&template[cursor..]);
    Ok(output)
}
