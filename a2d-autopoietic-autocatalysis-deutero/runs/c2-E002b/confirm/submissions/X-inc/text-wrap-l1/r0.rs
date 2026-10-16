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
        if !(32..=126).contains(&byte) && !matches!(byte, b'\t' | b'\n' | b'\r') {
            return Err(WrapError::InvalidByte { index, byte });
        }
    }

    let mut lines = Vec::new();
    let mut line = String::new();

    for word in input.split(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r')) {
        if word.is_empty() {
            continue;
        }

        let bytes = word.as_bytes();
        let mut offset = 0;

        while offset < bytes.len() {
            if line.len() == width {
                lines.push(std::mem::take(&mut line));
            }

            let remaining = bytes.len() - offset;
            let separator = usize::from(!line.is_empty());

            if remaining <= width {
                if line.len() + separator + remaining <= width {
                    if separator != 0 {
                        line.push(' ');
                    }
                    line.push_str(&word[offset..]);
                    offset = bytes.len();
                } else {
                    lines.push(std::mem::take(&mut line));
                }
            } else if line.is_empty() {
                let end = offset + width;
                lines.push(word[offset..end].to_string());
                offset = end;
            } else {
                let available = width - line.len() - 1;
                if available >= 2 {
                    line.push(' ');
                    line.push_str(&word[offset..offset + available]);
                    offset += available;
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