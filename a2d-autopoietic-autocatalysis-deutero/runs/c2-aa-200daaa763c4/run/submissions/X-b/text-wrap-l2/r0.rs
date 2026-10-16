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

    let normalized: String = input
        .replace("\r\n", "\n")
        .replace('\r', "\n");
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
            if i - run_start >= 2 {
                wrap_segment(&chars[segment_start..run_start], options, &mut output);
                for _ in 1..(i - run_start) {
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

fn wrap_segment(segment: &[char], options: &WrapOptions, output: &mut Vec<String>) {
    let mut words: Vec<Vec<char>> = Vec::new();
    let mut word = Vec::new();

    for &ch in segment {
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

    let mut first_line = true;
    let mut content = String::new();
    let mut content_len = 0usize;
    let mut indent = options.first_indent;

    for word in words {
        let mut offset = 0;
        while offset < word.len() {
            let capacity = options.width - indent;
            let remaining = word.len() - offset;

            if remaining <= capacity {
                let separator_len = usize::from(content_len > 0);
                if content_len + separator_len + remaining <= capacity {
                    if separator_len > 0 {
                        content.push(' ');
                        content_len += 1;
                    }
                    for &ch in &word[offset..] {
                        content.push(ch);
                    }
                    content_len += remaining;
                    offset = word.len();
                } else {
                    emit_line(indent, &content, output);
                    first_line = false;
                    indent = if first_line {
                        options.first_indent
                    } else {
                        options.rest_indent
                    };
                    content.clear();
                    content_len = 0;
                }
            } else if content_len == 0 {
                for &ch in &word[offset..offset + capacity] {
                    content.push(ch);
                }
                content_len = capacity;
                offset += capacity;

                emit_line(indent, &content, output);
                first_line = false;
                indent = options.rest_indent;
                content.clear();
                content_len = 0;
            } else {
                let available = capacity.saturating_sub(content_len + 1);
                if available >= 2 {
                    content.push(' ');
                    for &ch in &word[offset..offset + available] {
                        content.push(ch);
                    }
                    offset += available;
                    emit_line(indent, &content, output);
                    first_line = false;
                    indent = options.rest_indent;
                    content.clear();
                    content_len = 0;
                } else {
                    emit_line(indent, &content, output);
                    first_line = false;
                    indent = options.rest_indent;
                    content.clear();
                    content_len = 0;
                }
            }
        }
    }

    if content_len > 0 {
        emit_line(indent, &content, output);
    }
}

fn emit_line(indent: usize, content: &str, output: &mut Vec<String>) {
    let mut line = String::with_capacity(indent + content.len());
    line.extend(std::iter::repeat_n(' ', indent));
    line.push_str(content);
    output.push(line);
}