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

    let source = input.as_bytes();
    for (index, &byte) in source.iter().enumerate() {
        let allowed = (32..=126).contains(&byte) || byte == 9 || byte == 10 || byte == 13;
        if !allowed {
            return Err(WrapError::InvalidByte { index, byte });
        }
    }

    let mut output = Vec::new();
    let mut line = Vec::<u8>::new();
    let mut cursor = 0;

    while cursor < source.len() {
        while cursor < source.len()
            && matches!(source[cursor], b' ' | b'\t' | b'\n' | b'\r')
        {
            cursor += 1;
        }
        if cursor == source.len() {
            break;
        }

        let start = cursor;
        while cursor < source.len()
            && !matches!(source[cursor], b' ' | b'\t' | b'\n' | b'\r')
        {
            cursor += 1;
        }
        let word = &source[start..cursor];
        let mut pos = 0;

        while pos < word.len() {
            if line.len() == width {
                output.push(String::from_utf8(std::mem::take(&mut line)).unwrap());
            }

            let remaining = word.len() - pos;
            if remaining <= width {
                let separator = usize::from(!line.is_empty());
                if line.len() + separator + remaining <= width {
                    if separator == 1 {
                        line.push(b' ');
                    }
                    line.extend_from_slice(&word[pos..]);
                    pos = word.len();
                } else {
                    output.push(String::from_utf8(std::mem::take(&mut line)).unwrap());
                }
            } else if line.is_empty() {
                line.extend_from_slice(&word[pos..pos + width]);
                pos += width;
                output.push(String::from_utf8(std::mem::take(&mut line)).unwrap());
            } else {
                let available = width - line.len() - 1;
                if available >= 2 {
                    line.push(b' ');
                    line.extend_from_slice(&word[pos..pos + available]);
                    pos += available;
                    output.push(String::from_utf8(std::mem::take(&mut line)).unwrap());
                } else {
                    output.push(String::from_utf8(std::mem::take(&mut line)).unwrap());
                }
            }
        }
    }

    if !line.is_empty() {
        output.push(String::from_utf8(line).unwrap());
    }

    Ok(output)
}
