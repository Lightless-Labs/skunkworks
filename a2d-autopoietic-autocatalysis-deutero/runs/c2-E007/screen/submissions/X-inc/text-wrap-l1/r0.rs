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

    let bytes = input.as_bytes();

    for (index, &byte) in bytes.iter().enumerate() {
        let allowed = (32..=126).contains(&byte) || matches!(byte, b'\t' | b'\n' | b'\r');
        if !allowed {
            return Err(WrapError::InvalidByte { index, byte });
        }
    }

    let mut lines = Vec::new();
    let mut line = String::new();
    let mut position = 0;

    while position < bytes.len() {
        while position < bytes.len() && matches!(bytes[position], b' ' | b'\t' | b'\n' | b'\r') {
            position += 1;
        }
        if position == bytes.len() {
            break;
        }

        let start = position;
        while position < bytes.len() && !matches!(bytes[position], b' ' | b'\t' | b'\n' | b'\r') {
            position += 1;
        }

        let mut word_start = start;
        while word_start < position {
            if line.len() == width {
                lines.push(std::mem::take(&mut line));
            }

            let suffix_len = position - word_start;
            if suffix_len <= width {
                let needed = suffix_len + usize::from(!line.is_empty());
                if line.len() + needed <= width {
                    if !line.is_empty() {
                        line.push(' ');
                    }
                    line.push_str(std::str::from_utf8(&bytes[word_start..position]).unwrap());
                    word_start = position;
                } else {
                    lines.push(std::mem::take(&mut line));
                }
                continue;
            }

            if line.is_empty() {
                line.push_str(std::str::from_utf8(&bytes[word_start..word_start + width]).unwrap());
                lines.push(std::mem::take(&mut line));
                word_start += width;
            } else {
                let available = width - line.len() - 1;
                if available >= 2 {
                    line.push(' ');
                    line.push_str(
                        std::str::from_utf8(&bytes[word_start..word_start + available]).unwrap(),
                    );
                    lines.push(std::mem::take(&mut line));
                    word_start += available;
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