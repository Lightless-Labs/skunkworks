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
    if options.width == 0 {
        return Err(WrapError::WidthZero);
    }
    if options.width > 256 {
        return Err(WrapError::WidthTooLarge {
            width: options.width,
        });
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

    for (index, ch) in input.char_indices() {
        if ch.is_control() && ch != '\t' && ch != '\r' && ch != '\n' {
            return Err(WrapError::InvalidControl { index });
        }
    }

    let mut normalized = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            normalized.push('\n');
        } else {
            normalized.push(ch);
        }
    }

    let chars: Vec<char> = normalized.chars().collect();
    let mut output = Vec::new();
    let mut segment_start = 0;
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '\n' {
            let run_start = i;
            while i < chars.len() && chars[i] == '\n' {
                i += 1;
            }
            let run_len = i - run_start;
            if run_len >= 2 {
                wrap_segment(&chars[segment_start..run_start], options, &mut output);
                for _ in 1..run_len {
                    output.push(String::new());
                }
                segment_start = i;
            }
        } else {
            i += 1;
        }
    }

    wrap_segment(&chars[segment_start..], options, &mut output);
    Ok(output)
}

fn is_separator(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{00A0}')
}

fn wrap_segment(chars: &[char], options: &WrapOptions, output: &mut Vec<String>) {
    let mut words: Vec<Vec<char>> = Vec::new();
    let mut word = Vec::new();

    for &ch in chars {
        if is_separator(ch) {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
        } else {
            word.push(ch);
        }
    }
    if !word.is_empty() {
        words.push(word);
    }

    if words.is_empty() {
        return;
    }

    let mut line_number = 0usize;
    let mut line_indent = options.first_indent;
    let mut line_content: Vec<char> = Vec::new();

    for word in words {
        let mut offset = 0usize;

        while offset < word.len() {
            let capacity = options.width - line_indent;
            let suffix_len = word.len() - offset;

            if suffix_len <= capacity {
                let needed = suffix_len + usize::from(!line_content.is_empty());
                if line_content.len() + needed <= capacity {
                    if !line_content.is_empty() {
                        line_content.push(' ');
                    }
                    line_content.extend_from_slice(&word[offset..]);
                    offset = word.len();
                } else {
                    emit_line(
                        line_indent,
                        &line_content,
                        output,
                    );
                    line_content.clear();
                    line_number += 1;
                    line_indent = options.rest_indent;
                }
            } else if line_content.is_empty() {
                line_content.extend_from_slice(&word[offset..offset + capacity]);
                offset += capacity;
                emit_line(line_indent, &line_content, output);
                line_content.clear();
                line_number += 1;
                line_indent = options.rest_indent;
            } else {
                let available = capacity
                    .saturating_sub(line_content.len())
                    .saturating_sub(1);
                if available >= 2 {
                    line_content.push(' ');
                    line_content.extend_from_slice(&word[offset..offset + available]);
                    offset += available;
                    emit_line(line_indent, &line_content, output);
                    line_content.clear();
                    line_number += 1;
                    line_indent = options.rest_indent;
                } else {
                    emit_line(line_indent, &line_content, output);
                    line_content.clear();
                    line_number += 1;
                    line_indent = options.rest_indent;
                }
            }
        }

        let _ = line_number;
    }

    if !line_content.is_empty() {
        emit_line(line_indent, &line_content, output);
    }
}

fn emit_line(indent: usize, content: &[char], output: &mut Vec<String>) {
    let mut line = String::with_capacity(indent + content.len());
    line.extend(std::iter::repeat_n(' ', indent));
    line.extend(content.iter());
    output.push(line);
}