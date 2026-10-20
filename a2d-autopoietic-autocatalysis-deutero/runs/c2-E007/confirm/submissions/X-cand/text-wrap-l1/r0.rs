#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WrapError {
    WidthZero,
    WidthTooLarge { width: usize },
    InvalidByte { index: usize, byte: u8 },
}

pub fn wrap(input: &str, width: usize) -> Result<Vec<String>, WrapError> {
    if width == 0 {
        return Err(WrapError::WidthZero);
    }
    if width > 256 {
        return Err(WrapError::WidthTooLarge { width });
    }

    for (index, byte) in input.bytes().enumerate() {
        if !(matches!(byte, b' '..=b'~') || matches!(byte, b'\t' | b'\n' | b'\r')) {
            return Err(WrapError::InvalidByte { index, byte });
        }
    }

    let mut lines = Vec::new();
    let mut line = String::new();

    for word in input.split(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r')).filter(|s| !s.is_empty()) {
        let mut rest = word;

        while !rest.is_empty() {
            if line.len() == width {
                lines.push(std::mem::take(&mut line));
            }

            if rest.len() <= width {
                let needed = rest.len() + usize::from(!line.is_empty());
                if line.len() + needed <= width {
                    if !line.is_empty() {
                        line.push(' ');
                    }
                    line.push_str(rest);
                    rest = "";
                } else {
                    lines.push(std::mem::take(&mut line));
                }
            } else if line.is_empty() {
                lines.push(rest[..width].to_string());
                rest = &rest[width..];
            } else {
                let available = width - line.len() - 1;
                if available >= 2 {
                    line.push(' ');
                    line.push_str(&rest[..available]);
                    lines.push(std::mem::take(&mut line));
                    rest = &rest[available..];
                } else {
                    lines.push(std::mem::take(&mut line));
                }
            }
        }
    }

    if !line.is_empty() {
        lines.push(line);
    }

    Ok(lines)
}