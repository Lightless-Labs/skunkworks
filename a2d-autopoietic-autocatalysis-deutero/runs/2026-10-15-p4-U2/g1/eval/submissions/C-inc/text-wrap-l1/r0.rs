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
        if !((32..=126).contains(&byte) || matches!(byte, b'\t' | b'\n' | b'\r')) {
            return Err(WrapError::InvalidByte { index, byte });
        }
    }

    let mut lines = Vec::new();
    let mut line = String::new();

    for word in input.split(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r')).filter(|w| !w.is_empty()) {
        let mut offset = 0;

        while offset < word.len() {
            if line.len() == width {
                lines.push(std::mem::take(&mut line));
            }

            let remaining = &word[offset..];
            let remaining_len = remaining.len();

            if remaining_len <= width {
                let separator_len = usize::from(!line.is_empty());
                if line.len() + separator_len + remaining_len <= width {
                    if separator_len == 1 {
                        line.push(' ');
                    }
                    line.push_str(remaining);
                    offset = word.len();
                } else {
                    lines.push(std::mem::take(&mut line));
                }
            } else if line.is_empty() {
                line.push_str(&remaining[..width]);
                lines.push(std::mem::take(&mut line));
                offset += width;
            } else {
                let available = width - line.len() - 1;
                if available >= 2 {
                    line.push(' ');
                    line.push_str(&remaining[..available]);
                    lines.push(std::mem::take(&mut line));
                    offset += available;
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

fn main() {}