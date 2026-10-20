#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WrapError {
    WidthZero,
    WidthTooLarge { width: usize },
    FirstIndentTooWide { indent: usize },
    RestIndentTooWide { indent: usize },
    InvalidControl { index: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrapOptions {
    pub width: usize,
    pub first_indent: usize,
    pub rest_indent: usize,
}

pub fn wrap(input: &str, options: &WrapOptions) -> Result<Vec<String>, WrapError> {
    match options.width {
        0 => return Err(WrapError::WidthZero),
        width if width > 256 => return Err(WrapError::WidthTooLarge { width }),
        _ => {}
    }
    if options.first_indent >= options.width {
        return Err(WrapError::FirstIndentTooWide {
            indent: options.first_indent,
        });
    }
    if options.rest_indent >= options.width {
        return Err(WrapError::RestIndentTooWide {
            indent: options.rest_indent,
        });
    }

    for (byte_index, scalar) in input.char_indices() {
        if scalar.is_control() && !matches!(scalar, '\t' | '\r' | '\n') {
            return Err(WrapError::InvalidControl { index: byte_index });
        }
    }

    let mut normalized = Vec::new();
    let mut iter = input.chars().peekable();
    while let Some(ch) = iter.next() {
        if ch == '\r' {
            if iter.peek() == Some(&'\n') {
                iter.next();
            }
            normalized.push('\n');
        } else {
            normalized.push(ch);
        }
    }

    let mut result = Vec::new();
    let mut segment = Vec::new();
    let mut index = 0;
    while index < normalized.len() {
        if normalized[index] != '\n' {
            segment.push(normalized[index]);
            index += 1;
            continue;
        }

        let start = index;
        while index < normalized.len() && normalized[index] == '\n' {
            index += 1;
        }

        if index - start == 1 {
            segment.push('\n');
        } else {
            layout(&segment, options, &mut result);
            segment.clear();
            for _ in 1..(index - start) {
                result.push(String::new());
            }
        }
    }
    layout(&segment, options, &mut result);
    Ok(result)
}

fn separator(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{00A0}')
}

fn layout(segment: &[char], options: &WrapOptions, output: &mut Vec<String>) {
    let mut words: Vec<&[char]> = Vec::new();
    let mut start = None;
    for (i, &ch) in segment.iter().enumerate() {
        if separator(ch) {
            if let Some(begin) = start.take() {
                words.push(&segment[begin..i]);
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(begin) = start {
        words.push(&segment[begin..]);
    }
    if words.is_empty() {
        return;
    }

    let mut line = Vec::new();
    let mut line_number = 0usize;
    let mut wi = 0usize;
    let mut offset = 0usize;

    while wi < words.len() {
        let indent = if line_number == 0 {
            options.first_indent
        } else {
            options.rest_indent
        };
        let cap = options.width - indent;
        let tail_len = words[wi].len() - offset;

        if tail_len <= cap {
            let separator_len = usize::from(!line.is_empty());
            if line.len() + separator_len + tail_len <= cap {
                if separator_len != 0 {
                    line.push(' ');
                }
                line.extend_from_slice(&words[wi][offset..]);
                wi += 1;
                offset = 0;
            } else {
                push(&line, indent, output);
                line.clear();
                line_number += 1;
            }
        } else if line.is_empty() {
            line.extend_from_slice(&words[wi][offset..offset + cap]);
            offset += cap;
            push(&line, indent, output);
            line.clear();
            line_number += 1;
            if offset == words[wi].len() {
                wi += 1;
                offset = 0;
            }
        } else {
            let available = cap - line.len() - 1;
            if available < 2 {
                push(&line, indent, output);
                line.clear();
                line_number += 1;
            } else {
                line.push(' ');
                line.extend_from_slice(&words[wi][offset..offset + available]);
                offset += available;
                push(&line, indent, output);
                line.clear();
                line_number += 1;
                if offset == words[wi].len() {
                    wi += 1;
                    offset = 0;
                }
            }
        }
    }

    if !line.is_empty() {
        let indent = if line_number == 0 {
            options.first_indent
        } else {
            options.rest_indent
        };
        push(&line, indent, output);
    }
}

fn push(content: &[char], indent: usize, output: &mut Vec<String>) {
    let mut value = String::new();
    for _ in 0..indent {
        value.push(' ');
    }
    for &ch in content {
        value.push(ch);
    }
    output.push(value);
}
