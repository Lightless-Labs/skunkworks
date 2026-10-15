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
        if !(matches!(byte, b' '..=b'~' | b'\t' | b'\n' | b'\r')) {
            return Err(WrapError::InvalidByte { index, byte });
        }
    }

    let mut lines = Vec::new();
    let mut line = String::new();
    let mut pos = 0;

    while pos < bytes.len() {
        while pos < bytes.len() && matches!(bytes[pos], b' ' | b'\t' | b'\n' | b'\r') {
            pos += 1;
        }
        if pos == bytes.len() {
            break;
        }

        let start = pos;
        while pos < bytes.len() && !matches!(bytes[pos], b' ' | b'\t' | b'\n' | b'\r') {
            pos += 1;
        }
        let mut word_pos = start;

        while word_pos < pos {
            if line.len() == width {
                lines.push(std::mem::take(&mut line));
            }

            let suffix_len = pos - word_pos;
            let line_len = line.len();

            if suffix_len <= width {
                let needed = suffix_len + usize::from(!line.is_empty());
                if line_len + needed <= width {
                    if !line.is_empty() {
                        line.push(' ');
                    }
                    line.push_str(&input[word_pos..pos]);
                    word_pos = pos;
                } else {
                    lines.push(std::mem::take(&mut line));
                }
            } else if line.is_empty() {
                let end = word_pos + width;
                lines.push(input[word_pos..end].to_string());
                word_pos = end;
            } else {
                let available = width - line_len - 1;
                if available >= 2 {
                    line.push(' ');
                    let end = word_pos + available;
                    line.push_str(&input[word_pos..end]);
                    lines.push(std::mem::take(&mut line));
                    word_pos = end;
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