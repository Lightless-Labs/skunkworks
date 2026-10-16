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

    let mut output = Vec::new();
    let mut segment = String::new();
    let mut lf_run = 0usize;

    for ch in normalized.chars() {
        if ch == '\n' {
            lf_run += 1;
        } else {
            if lf_run >= 2 {
                wrap_segment(&segment, options, &mut output);
                output.extend((1..lf_run).map(|_| String::new()));
                segment.clear();
            } else if lf_run == 1 {
                segment.push('\n');
            }
            lf_run = 0;
            segment.push(ch);
        }
    }

    if lf_run >= 2 {
        wrap_segment(&segment, options, &mut output);
        output.extend((1..lf_run).map(|_| String::new()));
    } else {
        if lf_run == 1 {
            segment.push('\n');
        }
        wrap_segment(&segment, options, &mut output);
    }

    Ok(output)
}

fn is_separator(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{00A0}')
}

fn wrap_segment(segment: &str, options: &WrapOptions, output: &mut Vec<String>) {
    let words: Vec<Vec<char>> = segment
        .split(is_separator)
        .filter(|word| !word.is_empty())
        .map(|word| word.chars().collect())
        .collect();

    if words.is_empty() {
        return;
    }

    let mut first_line = true;
    let mut line_content = String::new();
    let mut content_len = 0usize;

    let emit_line = |output: &mut Vec<String>,
                     first_line: &mut bool,
                     line_content: &mut String,
                     content_len: &mut usize| {
        let indent = if *first_line {
            options.first_indent
        } else {
            options.rest_indent
        };
        let mut line = " ".repeat(indent);
        line.push_str(line_content);
        output.push(line);
        *first_line = false;
        line_content.clear();
        *content_len = 0;
    };

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
                    for &ch in &word[offset..] {
                        line_content.push(ch);
                    }
                    content_len += suffix_len;
                    offset = word.len();
                } else {
                    emit_line(
                        output,
                        &mut first_line,
                        &mut line_content,
                        &mut content_len,
                    );
                }
            } else if content_len == 0 {
                for &ch in &word[offset..offset + capacity] {
                    line_content.push(ch);
                }
                content_len = capacity;
                offset += capacity;
                emit_line(
                    output,
                    &mut first_line,
                    &mut line_content,
                    &mut content_len,
                );
            } else {
                let available = capacity.saturating_sub(content_len + 1);
                if available >= 2 {
                    line_content.push(' ');
                    content_len += 1;
                    for &ch in &word[offset..offset + available] {
                        line_content.push(ch);
                    }
                    content_len += available;
                    offset += available;
                    emit_line(
                        output,
                        &mut first_line,
                        &mut line_content,
                        &mut content_len,
                    );
                } else {
                    emit_line(
                        output,
                        &mut first_line,
                        &mut line_content,
                        &mut content_len,
                    );
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
        let mut line = " ".repeat(indent);
        line.push_str(&line_content);
        output.push(line);
    }
}