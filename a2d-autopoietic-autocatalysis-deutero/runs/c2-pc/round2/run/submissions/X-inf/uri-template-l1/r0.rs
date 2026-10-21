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

fn is_operator(byte: u8) -> bool {
    matches!(byte, b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&')
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn validate_expression(content: &str, base: usize, close: usize) -> Result<(), Error> {
    let bytes = content.as_bytes();
    if bytes.first().copied().is_some_and(is_operator) {
        return Err(Error::UnsupportedOperator { pos: base });
    }
    if bytes.is_empty() {
        return Err(Error::EmptyExpression { pos: close });
    }

    let mut seen: Vec<&str> = Vec::new();
    let mut start = 0;
    loop {
        let end = content[start..]
            .find(',')
            .map_or(content.len(), |relative| start + relative);

        if end == start {
            return Err(Error::EmptyVariable { pos: base + start });
        }

        let name = &content[start..end];
        let name_bytes = name.as_bytes();
        if !is_name_start(name_bytes[0]) {
            return Err(Error::InvalidName { pos: base + start });
        }
        for (index, &byte) in name_bytes.iter().enumerate().skip(1) {
            if !is_name_continue(byte) {
                return Err(Error::InvalidName {
                    pos: base + start + index,
                });
            }
        }

        if seen.iter().any(|prior| *prior == name) {
            return Err(Error::DuplicateVariable { pos: base + start });
        }
        seen.push(name);

        if end == content.len() {
            break;
        }
        start = end + 1;
    }
    Ok(())
}

fn append_encoded(out: &mut String, value: &str) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = value.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%' && index + 2 < bytes.len()
            && bytes[index + 1].is_ascii_hexdigit()
            && bytes[index + 2].is_ascii_hexdigit()
        {
            out.push('%');
            out.push((bytes[index + 1] as char).to_ascii_uppercase());
            out.push((bytes[index + 2] as char).to_ascii_uppercase());
            index += 3;
        } else if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
            index += 1;
        } else {
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 15) as usize] as char);
            index += 1;
        }
    }
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let bytes = template.as_bytes();
    let mut spans: Vec<(usize, usize, usize)> = Vec::new();
    let mut scan = 0;

    while scan < bytes.len() {
        match bytes[scan] {
            b'}' => return Err(Error::UnexpectedClose { pos: scan }),
            b'{' => {
                let opening = scan;
                let mut close = scan + 1;
                while close < bytes.len() && bytes[close] != b'}' {
                    close += 1;
                }
                if close == bytes.len() {
                    return Err(Error::Unclosed { pos: opening });
                }
                if let Some(relative) = bytes[opening + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen { pos: opening + 1 + relative });
                }

                let content_start = opening + 1;
                let content = &template[content_start..close];
                validate_expression(content, content_start, close)?;
                spans.push((opening, content_start, close));
                scan = close + 1;
            }
            _ => scan += 1,
        }
    }

    let mut output = String::new();
    let mut literal_start = 0;
    for (opening, content_start, close) in spans {
        output.push_str(&template[literal_start..opening]);
        let content = &template[content_start..close];
        let mut first = true;
        for name in content.split(',') {
            if let Some((_, value)) = vars.iter().rev().find(|(key, _)| *key == name) {
                if !first {
                    output.push(',');
                }
                append_encoded(&mut output, value);
                first = false;
            }
        }
        literal_start = close + 1;
    }
    output.push_str(&template[literal_start..]);
    Ok(output)
}