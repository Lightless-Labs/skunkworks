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

#[derive(Debug)]
struct Spec<'a> {
    operator: Option<u8>,
    names: Vec<&'a str>,
}

#[derive(Debug)]
enum Part<'a> {
    Literal(&'a str),
    Expression(Spec<'a>),
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

fn parse_expression<'a>(template: &'a str, open: usize, close: usize) -> Result<Spec<'a>, Error> {
    let bytes = template.as_bytes();
    if let Some(inner) = (open + 1..close).find(|&i| bytes[i] == b'{') {
        return Err(Error::NestedOpen { pos: inner });
    }

    let mut start = open + 1;
    let operator = if start < close && b"+#./;?&".contains(&bytes[start]) {
        let op_pos = start;
        let op = bytes[start];
        start += 1;
        if op != b'+' && op != b'#' {
            return Err(Error::UnsupportedOperator { pos: op_pos });
        }
        Some(op)
    } else {
        None
    };

    if start == close {
        return Err(Error::EmptyExpression { pos: close });
    }

    let mut names = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    let mut spec_start = start;

    loop {
        let spec_end = (spec_start..close)
            .find(|&i| bytes[i] == b',')
            .unwrap_or(close);

        if spec_start == spec_end {
            return Err(Error::EmptyVariable { pos: spec_start });
        }

        let mut i = spec_start;
        if !is_name_start(bytes[i]) {
            return Err(Error::InvalidName { pos: i });
        }
        i += 1;
        while i < spec_end && is_name_continue(bytes[i]) {
            i += 1;
        }
        if i < spec_end {
            return Err(Error::InvalidName { pos: i });
        }

        let name = &template[spec_start..spec_end];
        if seen.contains(&name) {
            return Err(Error::DuplicateVariable { pos: spec_start });
        }
        seen.push(name);
        names.push(name);

        if spec_end == close {
            break;
        }
        spec_start = spec_end + 1;
    }

    Ok(Spec { operator, names })
}

fn parse_template(template: &str) -> Result<Vec<Part<'_>>, Error> {
    let bytes = template.as_bytes();
    let mut parts = Vec::new();
    let mut literal_start = 0;
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                if literal_start < i {
                    parts.push(Part::Literal(&template[literal_start..i]));
                }
                let open = i;
                let Some(relative_close) = bytes[i + 1..].iter().position(|&b| b == b'}') else {
                    return Err(Error::Unclosed { pos: open });
                };
                let close = i + 1 + relative_close;
                let spec = parse_expression(template, open, close)?;
                parts.push(Part::Expression(spec));
                i = close + 1;
                literal_start = i;
            }
            _ => i += 1,
        }
    }

    if literal_start < bytes.len() {
        parts.push(Part::Literal(&template[literal_start..]));
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

fn upper_hex(byte: u8) -> u8 {
    match byte {
        b'a'..=b'f' => byte - b'a' + b'A',
        _ => byte,
    }
}

fn encode(value: &str, allow_reserved: bool, output: &mut String) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = value.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];

        if byte == b'%' && i + 2 < bytes.len()
            && hex_value(bytes[i + 1]).is_some()
            && hex_value(bytes[i + 2]).is_some()
        {
            output.push('%');
            output.push(upper_hex(bytes[i + 1]) as char);
            output.push(upper_hex(bytes[i + 2]) as char);
            i += 3;
            continue;
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
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let parts = parse_template(template)?;
    let mut output = String::new();

    for part in parts {
        match part {
            Part::Literal(text) => output.push_str(text),
            Part::Expression(spec) => {
                let mut fields = Vec::new();
                for name in spec.names {
                    if let Some((_, value)) = vars.iter().rev().find(|(key, _)| *key == name) {
                        fields.push(*value);
                    }
                }

                if fields.is_empty() {
                    continue;
                }

                if spec.operator == Some(b'#') {
                    output.push('#');
                }

                let allow_reserved = matches!(spec.operator, Some(b'+' | b'#'));
                for (index, value) in fields.into_iter().enumerate() {
                    if index > 0 {
                        output.push(',');
                    }
                    encode(value, allow_reserved, &mut output);
                }
            }
        }
    }

    Ok(output)
}
