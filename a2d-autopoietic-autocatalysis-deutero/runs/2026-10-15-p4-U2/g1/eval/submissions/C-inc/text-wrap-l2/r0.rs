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

    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
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
                append_segment(&chars[segment_start..run_start], options, &mut output);
                for _ in 1..(i - run_start) {
                    output.push(String::new());
                }
                segment_start = i;
            }
        } else {
            i += 1;
        }
    }

    append_segment(&chars[segment_start..], options, &mut output);
    Ok(output)
}

fn is_separator(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{00A0}')
}

fn append_segment(segment: &[char], options: &WrapOptions, output: &mut Vec<String>) {
    let words: Vec<Vec<char>> = segment
        .split(|ch| is_separator(*ch))
        .filter(|word| !word.is_empty())
        .map(|word| word.to_vec())
        .collect();

    if words.is_empty() {
        return;
    }

    let mut first_line = true;
    let mut line_content = String::new();
    let mut content_len = 0usize;

    for word in words {
        let mut offset = 0usize;

        while offset < word.len() {
            let indent = if first_line {
                options.first_indent
            } else {
                options.rest_indent
            };
            let capacity = options.width - indent;
            let suffix_len = word.len() - offset;

            if suffix_len <= capacity {
                let separator_len = usize::from(content_len > 0);
                if content_len + separator_len + suffix_len <= capacity {
                    if separator_len > 0 {
                        line_content.push(' ');
                        content_len += 1;
                    }
                    for ch in &word[offset..] {
                        line_content.push(*ch);
                    }
                    content_len += suffix_len;
                    offset = word.len();
                } else {
                    emit_line(
                        &mut line_content,
                        &mut content_len,
                        indent,
                        output,
                    );
                    first_line = false;
                }
            } else if content_len == 0 {
                for ch in &word[offset..offset + capacity] {
                    line_content.push(*ch);
                }
                content_len = capacity;
                offset += capacity;
                emit_line(
                    &mut line_content,
                    &mut content_len,
                    indent,
                    output,
                );
                first_line = false;
            } else {
                let available = capacity - content_len - 1;
                if available >= 2 {
                    line_content.push(' ');
                    for ch in &word[offset..offset + available] {
                        line_content.push(*ch);
                    }
                    offset += available;
                    content_len += 1 + available;
                    emit_line(
                        &mut line_content,
                        &mut content_len,
                        indent,
                        output,
                    );
                    first_line = false;
                } else {
                    emit_line(
                        &mut line_content,
                        &mut content_len,
                        indent,
                        output,
                    );
                    first_line = false;
                }
            }
        }
    }

    if content_len > 0 {
        let indent = if first_line {
            options.first_indent
        } else {
            options.rest_indent
        };
        emit_line(&mut line_content, &mut content_len, indent, output);
    }
}

fn emit_line(
    content: &mut String,
    content_len: &mut usize,
    indent: usize,
    output: &mut Vec<String>,
) {
    let mut line = " ".repeat(indent);
    line.push_str(content);
    output.push(line);
    content.clear();
    *content_len = 0;
}

fn main() {}