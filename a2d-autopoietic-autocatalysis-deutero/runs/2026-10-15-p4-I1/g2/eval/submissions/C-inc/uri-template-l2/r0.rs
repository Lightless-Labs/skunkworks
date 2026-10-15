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
struct Variable {
    name: String,
    start: usize,
}

#[derive(Debug)]
struct Expression {
    start: usize,
    end: usize,
    operator: Option<char>,
    variables: Vec<Variable>,
}

#[derive(Debug)]
enum Part {
    Literal { start: usize, end: usize },
    Expression(Expression),
}

fn parse(template: &str) -> Result<Vec<Part>, Error> {
    let mut parts = Vec::new();
    let mut pos = 0;
    let mut literal_start = 0;

    while pos < template.len() {
        match template.as_bytes()[pos] {
            b'}' => return Err(Error::UnexpectedClose { pos }),
            b'{' => {
                if literal_start < pos {
                    parts.push(Part::Literal {
                        start: literal_start,
                        end: pos,
                    });
                }

                let open = pos;
                let Some(relative_close) = template.as_bytes()[open + 1..]
                    .iter()
                    .position(|&byte| byte == b'}')
                else {
                    return Err(Error::Unclosed { pos: open });
                };
                let close = open + 1 + relative_close;

                if let Some(relative_nested) = template.as_bytes()[open + 1..close]
                    .iter()
                    .position(|&byte| byte == b'{')
                {
                    return Err(Error::NestedOpen {
                        pos: open + 1 + relative_nested,
                    });
                }

                let content_start = open + 1;
                let mut spec_start = content_start;
                let mut operator = None;
                if content_start < close
                    && matches!(
                        template.as_bytes()[content_start],
                        b'+' | b'#' | b'.' | b'/' | b';' | b'?' | b'&'
                    )
                {
                    let op = template.as_bytes()[content_start] as char;
                    if op != '+' && op != '#' {
                        return Err(Error::UnsupportedOperator { pos: content_start });
                    }
                    operator = Some(op);
                    spec_start += 1;
                }

                if spec_start == close {
                    return Err(Error::EmptyExpression { pos: close });
                }

                let mut variables = Vec::new();
                let mut seen = std::collections::HashSet::new();
                let mut entry_start = spec_start;

                loop {
                    let entry_end = template.as_bytes()[entry_start..close]
                        .iter()
                        .position(|&byte| byte == b',')
                        .map_or(close, |offset| entry_start + offset);

                    if entry_start == entry_end {
                        return Err(Error::EmptyVariable { pos: entry_start });
                    }

                    let bytes = &template.as_bytes()[entry_start..entry_end];
                    let valid_first = |byte: u8| byte.is_ascii_alphabetic() || byte == b'_';
                    let valid_rest = |byte: u8| {
                        byte.is_ascii_alphanumeric() || byte == b'_'
                    };

                    if !valid_first(bytes[0]) {
                        return Err(Error::InvalidName { pos: entry_start });
                    }
                    if let Some(offset) = bytes[1..].iter().position(|&byte| !valid_rest(byte)) {
                        return Err(Error::InvalidName {
                            pos: entry_start + 1 + offset,
                        });
                    }

                    let name = &template[entry_start..entry_end];
                    if !seen.insert(name.to_owned()) {
                        return Err(Error::DuplicateVariable { pos: entry_start });
                    }
                    variables.push(Variable {
                        name: name.to_owned(),
                        start: entry_start,
                    });

                    if entry_end == close {
                        break;
                    }
                    entry_start = entry_end + 1;
                }

                parts.push(Part::Expression(Expression {
                    start: open,
                    end: close,
                    operator,
                    variables,
                }));
                pos = close + 1;
                literal_start = pos;
            }
            _ => {
                let ch = template[pos..].chars().next().expect("valid UTF-8 boundary");
                pos += ch.len_utf8();
            }
        }
    }

    if literal_start < template.len() {
        parts.push(Part::Literal {
            start: literal_start,
            end: template.len(),
        });
    }

    Ok(parts)
}

fn encode(value: &str, allow_reserved: bool) -> String {
    let bytes = value.as_bytes();
    let mut output = String::new();
    let mut pos = 0;

    while pos < bytes.len() {
        let byte = bytes[pos];

        if byte == b'%' && pos + 2 < bytes.len() {
            let hex = |b: u8| -> Option<u8> {
                match b {
                    b'0'..=b'9' => Some(b - b'0'),
                    b'a'..=b'f' => Some(b - b'a' + 10),
                    b'A'..=b'F' => Some(b - b'A' + 10),
                    _ => None,
                }
            };
            if let (Some(high), Some(low)) = (hex(bytes[pos + 1]), hex(bytes[pos + 2])) {
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                output.push('%');
                output.push(HEX[high as usize] as char);
                output.push(HEX[low as usize] as char);
                pos += 3;
                continue;
            }
        }

        let unreserved = byte.is_ascii_alphanumeric() || b"-._~".contains(&byte);
        let reserved = b":/?#[]@!$&'()*+,;=".contains(&byte);
        if unreserved || (allow_reserved && reserved) {
            output.push(byte as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
        pos += 1;
    }

    output
}

pub fn expand(template: &str, vars: &[(&str, &str)]) -> Result<String, Error> {
    let parts = parse(template)?;
    let mut output = String::new();

    for part in parts {
        match part {
            Part::Literal { start, end } => output.push_str(&template[start..end]),
            Part::Expression(expression) => {
                let allow_reserved = matches!(expression.operator, Some('+') | Some('#'));
                let mut fields = Vec::new();

                for variable in expression.variables {
                    let _ = variable.start;
                    if let Some((_, value)) = vars.iter().rev().find(|(name, _)| *name == variable.name) {
                        fields.push(encode(value, allow_reserved));
                    }
                }

                if !fields.is_empty() {
                    if expression.operator == Some('#') {
                        output.push('#');
                    }
                    output.push_str(&fields.join(","));
                }

                let _ = (expression.start, expression.end);
            }
        }
    }

    Ok(output)
}

fn main() {}