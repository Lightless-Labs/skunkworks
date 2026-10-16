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
            let mut end = i + 1;
            while end < chars.len() && chars[end] == '\n' {
                end += 1;
            }

            if end - i >= 2 {
                append_segment(&chars[segment_start..i], options, &mut output);
                output.extend((1..(end - i)).map(|_| String::new()));
                segment_start = end;
            }
            i = end;
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

fn append_segment(chars: &[char], options: &WrapOptions, output: &mut Vec<String>) {
    let words: Vec<Vec<char>> = chars
        .split(|ch: &char| is_separator(*ch))
        .filter(|part| !part.is_empty())
        .map(|part| part.to_vec())
        .collect();

    if words.is_empty() {
        return;
    }

    let mut first_line = true;
    let mut line_content = String::new();
    let mut content_length = 0usize;

    let emit_line = |output: &mut Vec<String>,
                     first_line: &mut bool,
                     line_content: &mut String,
                     content_length: &mut usize| {
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
        *content_length = 0;
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
            let suffix_length = word.len() - offset;

            if suffix_length <= capacity {
                let needed = suffix_length + usize::from(content_length > 0);
                if content_length + needed <= capacity {
                    if content_length > 0 {
                        line_content.push(' ');
                        content_length += 1;
                    }
                    for &ch in &word[offset..] {
                        line_content.push(ch);
                        content_length += 1;
                    }
                    offset = word.len();
                } else {
                    emit_line(
                        output,
                        &mut first_line,
                        &mut line_content,
                        &mut content_length,
                    );
                }
            } else if content_length == 0 {
                for &ch in &word[offset..offset + capacity] {
                    line_content.push(ch);
                }
                content_length = capacity;
                offset += capacity;
                emit_line(
                    output,
                    &mut first_line,
                    &mut line_content,
                    &mut content_length,
                );
            } else {
                let available = capacity - content_length - 1;
                if available >= 2 {
                    line_content.push(' ');
                    content_length += 1;
                    for &ch in &word[offset..offset + available] {
                        line_content.push(ch);
                        content_length += 1;
                    }
                    offset += available;
                    emit_line(
                        output,
                        &mut first_line,
                        &mut line_content,
                        &mut content_length,
                    );
                } else {
                    emit_line(
                        output,
                        &mut first_line,
                        &mut line_content,
                        &mut content_length,
                    );
                }
            }
        }
    }

    if content_length > 0 {
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