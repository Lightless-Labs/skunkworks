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

fn is_separator(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{00A0}')
}

fn emit_paragraph(
    segment: &str,
    options: &WrapOptions,
    output: &mut Vec<String>,
) {
    let words: Vec<Vec<char>> = segment
        .split(is_separator)
        .filter(|word| !word.is_empty())
        .map(|word| word.chars().collect())
        .collect();

    let mut first_line = true;
    let mut content = String::new();
    let mut content_length = 0usize;

    let emit_line = |first_line: &mut bool,
                     content: &mut String,
                     content_length: &mut usize,
                     output: &mut Vec<String>| {
        let indent = if *first_line {
            options.first_indent
        } else {
            options.rest_indent
        };
        let mut line = " ".repeat(indent);
        line.push_str(content);
        output.push(line);
        *first_line = false;
        content.clear();
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
            let remaining = word.len() - offset;

            if remaining <= capacity {
                let needed = remaining + usize::from(content_length > 0);
                if content_length + needed <= capacity {
                    if content_length > 0 {
                        content.push(' ');
                        content_length += 1;
                    }
                    for &ch in &word[offset..] {
                        content.push(ch);
                    }
                    content_length += remaining;
                    offset = word.len();
                } else {
                    emit_line(
                        &mut first_line,
                        &mut content,
                        &mut content_length,
                        output,
                    );
                }
            } else if content_length == 0 {
                for &ch in &word[offset..offset + capacity] {
                    content.push(ch);
                }
                content_length = capacity;
                offset += capacity;
                emit_line(
                    &mut first_line,
                    &mut content,
                    &mut content_length,
                    output,
                );
            } else {
                let available = capacity - content_length - 1;
                if available >= 2 {
                    content.push(' ');
                    for &ch in &word[offset..offset + available] {
                        content.push(ch);
                    }
                    offset += available;
                    content_length += available + 1;
                    emit_line(
                        &mut first_line,
                        &mut content,
                        &mut content_length,
                        output,
                    );
                } else {
                    emit_line(
                        &mut first_line,
                        &mut content,
                        &mut content_length,
                        output,
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
        line.push_str(&content);
        output.push(line);
    }
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
        if ch.is_control() && !matches!(ch, '\t' | '\r' | '\n') {
            return Err(WrapError::InvalidControl { index });
        }
    }

    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    if normalized.is_empty() {
        return Ok(Vec::new());
    }

    let chars: Vec<char> = normalized.chars().collect();
    let mut output = Vec::new();
    let mut segment_start = 0usize;
    let mut index = 0usize;

    while index < chars.len() {
        if chars[index] == '\n' {
            let run_start = index;
            while index < chars.len() && chars[index] == '\n' {
                index += 1;
            }

            if index - run_start >= 2 {
                let segment: String = chars[segment_start..run_start].iter().collect();
                emit_paragraph(&segment, options, &mut output);

                for _ in 1..(index - run_start) {
                    output.push(String::new());
                }
                segment_start = index;
            }
        } else {
            index += 1;
        }
    }

    let segment: String = chars[segment_start..].iter().collect();
    emit_paragraph(&segment, options, &mut output);

    Ok(output)
}

fn main() {}