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
        if !(32..=126).contains(&byte) && !matches!(byte, b'\t' | b'\n' | b'\r') {
            return Err(WrapError::InvalidByte { index, byte });
        }
    }

    let mut lines = Vec::new();
    let mut line = String::new();

    for word in input.split(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r')).filter(|w| !w.is_empty()) {
        let mut remainder = word;

        while !remainder.is_empty() {
            let suffix_len = remainder.len();

            if suffix_len <= width {
                let needed = suffix_len + usize::from(!line.is_empty());
                if line.len() + needed <= width {
                    if !line.is_empty() {
                        line.push(' ');
                    }
                    line.push_str(remainder);
                    remainder = "";
                } else {
                    if !line.is_empty() {
                        lines.push(std::mem::take(&mut line));
                    } else {
                        // A suffix no longer than width always fits on an empty line.
                        unreachable!();
                    }
                }
            } else if line.is_empty() {
                let (head, tail) = remainder.split_at(width);
                lines.push(head.to_string());
                remainder = tail;
            } else {
                let available = width - line.len() - 1;
                if available >= 2 {
                    line.push(' ');
                    line.push_str(&remainder[..available]);
                    lines.push(std::mem::take(&mut line));
                    remainder = &remainder[available..];
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