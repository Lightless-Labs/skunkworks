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
struct Specifier<'a> {
    name: &'a str,
    start: usize,
}

#[derive(Debug)]
struct Expression<'a> {
    start: usize,
    end: usize,
    specifiers: Vec<Specifier<'a>>,
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let bytes = template.as_bytes();
    let mut expressions = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'}' => return Err(Error::UnexpectedClose { pos: i }),
            b'{' => {
                let open = i;
                let close = match bytes[i + 1..].iter().position(|&b| b == b'}') {
                    Some(offset) => i + 1 + offset,
                    None => return Err(Error::Unclosed { pos: open }),
                };

                if let Some(offset) = bytes[i + 1..close].iter().position(|&b| b == b'{') {
                    return Err(Error::NestedOpen {
                        pos: i + 1 + offset,
                    });
                }

                let mut content_start = i + 1;
                if content_start == close {
                    return Err(Error::EmptyExpression { pos: close });
                }

                match bytes[content_start] {
                    b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&' => {
                        return Err(Error::UnsupportedOperator { pos: content_start });
                    }
                    _ => {}
                }

                let mut specifiers = Vec::new();
                let mut spec_start = content_start;
                while spec_start <= close {
                    let spec_end = bytes[spec_start..close]
                        .iter()
                        .position(|&b| b == b',')
                        .map_or(close, |offset| spec_start + offset);

                    if spec_start == spec_end {
                        return Err(Error::EmptyVariable { pos: spec_start });
                    }

                    let name_bytes = &bytes[spec_start..spec_end];
                    let valid_first = matches!(
                        name_bytes[0],
                        b'A'..=b'Z' | b'a'..=b'z' | b'_'
                    );
                    if !valid_first {
                        return Err(Error::InvalidName { pos: spec_start });
                    }

                    let mut seen = Vec::<&str>::new();
                    let name = &template[spec_start..spec_end];
                    for (offset, &byte) in name_bytes.iter().enumerate().skip(1) {
                        if !matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_') {
                            return Err(Error::InvalidName {
                                pos: spec_start + offset,
                            });
                        }
                    }

                    if specifiers.iter().any(|specifier: &Specifier<'_>| specifier.name == name) {
                        return Err(Error::DuplicateVariable { pos: spec_start });
                    }
                    seen.clear();
                    seen.push(name);
                    specifiers.push(Specifier {
                        name,
                        start: spec_start,
                    });

                    if spec_end == close {
                        break;
                    }
                    spec_start = spec_end + 1;
                }

                expressions.push(Expression {
                    start: open,
                    end: close,
                    specifiers,
                });
                i = close + 1;
            }
            _ => i += 1,
        }
    }

    let mut output = String::new();
    let mut cursor = 0;
    for expression in expressions {
        output.push_str(&template[cursor..expression.start]);
        let mut first = true;
        for specifier in expression.specifiers {
            if let Some(value) = vars
                .iter()
                .rev()
                .find(|(name, _)| *name == specifier.name)
                .map(|(_, value)| *value)
            {
                if !first {
                    output.push(',');
                }
                first = false;
                encode_value(value, &mut output);
            }
        }
        cursor = expression.end + 1;
    }
    output.push_str(&template[cursor..]);
    Ok(output)
}

fn encode_value(value: &str, output: &mut String) {
    let bytes = value.as_bytes();
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

        if matches!(
            byte,
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~'
        ) {
            output.push(byte as char);
        } else {
            output.push('%');
            output.push(hex_digit(byte >> 4));
            output.push(hex_digit(byte & 0x0f));
        }
        i += 1;
    }
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