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

    for (index, &byte) in input.as_bytes().iter().enumerate() {
        if !(matches!(byte, 32..=126 | 9 | 10 | 13)) {
            return Err(WrapError::InvalidByte { index, byte });
        }
    }

    let mut lines = Vec::new();
    let mut line = String::new();

    for word in input.split(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r')).filter(|w| !w.is_empty()) {
        let mut remaining = word;

        while !remaining.is_empty() {
            if line.len() == width {
                lines.push(std::mem::take(&mut line));
            }

            let suffix_len = remaining.len();

            if suffix_len <= width {
                let needed = suffix_len + usize::from(!line.is_empty());
                if line.len() + needed <= width {
                    if !line.is_empty() {
                        line.push(' ');
                    }
                    line.push_str(remaining);
                    remaining = "";
                } else {
                    lines.push(std::mem::take(&mut line));
                }
            } else if line.is_empty() {
                line.push_str(&remaining[..width]);
                remaining = &remaining[width..];
                lines.push(std::mem::take(&mut line));
            } else {
                let available = width - line.len() - 1;
                if available >= 2 {
                    line.push(' ');
                    line.push_str(&remaining[..available]);
                    remaining = &remaining[available..];
                    lines.push(std::mem::take(&mut line));
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