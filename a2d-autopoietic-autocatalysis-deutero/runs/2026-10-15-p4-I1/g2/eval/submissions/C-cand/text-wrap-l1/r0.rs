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
        if !(matches!(byte, b' '..=b'~') || matches!(byte, b'\t' | b'\n' | b'\r')) {
            return Err(WrapError::InvalidByte { index, byte });
        }
    }

    let mut lines = Vec::new();
    let mut line = String::new();

    for word in input.split(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r')).filter(|word| !word.is_empty()) {
        let mut suffix = word;

        while !suffix.is_empty() {
            if line.len() == width {
                lines.push(std::mem::take(&mut line));
            }

            let suffix_len = suffix.len();

            if suffix_len <= width {
                let needed = suffix_len + usize::from(!line.is_empty());
                if line.len() + needed <= width {
                    if !line.is_empty() {
                        line.push(' ');
                    }
                    line.push_str(suffix);
                    break;
                }

                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                    continue;
                }
            } else if line.is_empty() {
                line.push_str(&suffix[..width]);
                lines.push(std::mem::take(&mut line));
                suffix = &suffix[width..];
                continue;
            } else {
                let available = width - line.len() - 1;
                if available >= 2 {
                    line.push(' ');
                    line.push_str(&suffix[..available]);
                    lines.push(std::mem::take(&mut line));
                    suffix = &suffix[available..];
                    continue;
                }

                lines.push(std::mem::take(&mut line));
                continue;
            }
        }
    }

    if !line.is_empty() {
        lines.push(line);
    }
    Ok(lines)
}

fn main() {}