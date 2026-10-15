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
    let mut start = 0;

    while start < chars.len() {
        if chars[start] == '\n' {
            let mut end = start;
            while end < chars.len() && chars[end] == '\n' {
                end += 1;
            }
            if end - start >= 2 {
                for _ in 1..(end - start) {
                    output.push(String::new());
                }
                start = end;
                continue;
            }
        }

        let mut end = start;
        while end < chars.len() {
            if chars[end] == '\n' {
                let mut newline_end = end;
                while newline_end < chars.len() && chars[newline_end] == '\n' {
                    newline_end += 1;
                }
                if newline_end - end >= 2 {
                    break;
                }
            }
            end += 1;
        }

        let segment = &chars[start..end];
        let mut words: Vec<Vec<char>> = Vec::new();
        let mut current = Vec::new();
        for &ch in segment {
            let separator = ch == ' ' || ch == '\t' || ch == '\n' || ch == '\r' || ch == '\u{00A0}';
            if separator {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            } else {
                current.push(ch);
            }
        }
        if !current.is_empty() {
            words.push(current);
        }

        let mut line = String::new();
        let mut line_content_len = 0usize;
        let mut first_line = true;

        let emit_line = |line: &mut String,
                         line_content_len: &mut usize,
                         first_line: &mut bool,
                         output: &mut Vec<String>,
                         indent: usize| {
            if *line_content_len > 0 {
                let mut rendered = " ".repeat(indent);
                rendered.push_str(line);
                output.push(rendered);
                line.clear();
                *line_content_len = 0;
                *first_line = false;
            }
        };

        for word in words {
            let mut offset = 0;
            while offset < word.len() {
                let indent = if first_line {
                    options.first_indent
                } else {
                    options.rest_indent
                };
                let capacity = options.width - indent;
                let remaining = word.len() - offset;

                if remaining <= capacity {
                    let needed = remaining + usize::from(line_content_len > 0);
                    if line_content_len + needed <= capacity {
                        if line_content_len > 0 {
                            line.push(' ');
                            line_content_len += 1;
                        }
                        line.extend(word[offset..].iter());
                        line_content_len += remaining;
                        offset = word.len();
                    } else {
                        emit_line(
                            &mut line,
                            &mut line_content_len,
                            &mut first_line,
                            &mut output,
                            indent,
                        );
                    }
                } else if line_content_len == 0 {
                    line.extend(word[offset..offset + capacity].iter());
                    line_content_len = capacity;
                    offset += capacity;
                    emit_line(
                        &mut line,
                        &mut line_content_len,
                        &mut first_line,
                        &mut output,
                        indent,
                    );
                } else {
                    let available = capacity.saturating_sub(line_content_len + 1);
                    if available >= 2 {
                        line.push(' ');
                        line.extend(word[offset..offset + available].iter());
                        line_content_len += 1 + available;
                        offset += available;
                    }
                    emit_line(
                        &mut line,
                        &mut line_content_len,
                        &mut first_line,
                        &mut output,
                        indent,
                    );
                }
            }
        }

        if line_content_len > 0 {
            let indent = if first_line {
                options.first_indent
            } else {
                options.rest_indent
            };
            let mut rendered = " ".repeat(indent);
            rendered.push_str(&line);
            output.push(rendered);
        }

        if end == chars.len() {
            break;
        }

        let mut boundary_end = end;
        while boundary_end < chars.len() && chars[boundary_end] == '\n' {
            boundary_end += 1;
        }
        let boundary_count = boundary_end - end;
        if boundary_count >= 2 {
            for _ in 1..boundary_count {
                output.push(String::new());
            }
            start = boundary_end;
        } else {
            start = end + 1;
        }
    }

    Ok(output)
}

fn main() {}