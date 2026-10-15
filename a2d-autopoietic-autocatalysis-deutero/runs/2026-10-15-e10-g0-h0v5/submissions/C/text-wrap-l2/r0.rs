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
    let mut result = Vec::new();
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
                wrap_segment(&chars[segment_start..run_start], options, &mut result);
                for _ in 1..run_len {
                    result.push(String::new());
                }
                segment_start = i;
            }
        } else {
            i += 1;
        }
    }
    wrap_segment(&chars[segment_start..], options, &mut result);

    Ok(result)
}

fn is_separator(ch: char) -> bool {
    ch == ' ' || ch == '\t' || ch == '\n' || ch == '\r' || ch == '\u{00A0}'
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

    let mut line = String::new();
    let mut content_len = 0usize;
    let mut line_number = 0usize;

    let indent_for_line = |line_number: usize| {
        if line_number == 0 {
            options.first_indent
        } else {
            options.rest_indent
        }
    };

    let emit_line = |line: &mut String,
                     content_len: &mut usize,
                     line_number: &mut usize,
                     output: &mut Vec<String>| {
        if *content_len > 0 {
            output.push(line.clone());
            line.clear();
            *content_len = 0;
            *line_number += 1;
        }
    };

    for word in words {
        let mut offset = 0usize;
        while offset < word.len() {
            let indent = indent_for_line(line_number);
            let capacity = options.width - indent;
            let remaining = word.len() - offset;

            if remaining <= capacity {
                let separator_len = if content_len > 0 { 1 } else { 0 };
                if content_len + separator_len + remaining <= capacity {
                    if content_len == 0 {
                        line.push_str(&" ".repeat(indent));
                    } else {
                        line.push(' ');
                        content_len += 1;
                    }
                    for &ch in &word[offset..] {
                        line.push(ch);
                    }
                    content_len += remaining;
                    offset = word.len();
                } else {
                    emit_line(&mut line, &mut content_len, &mut line_number, output);
                }
            } else if content_len == 0 {
                line.push_str(&" ".repeat(indent));
                for &ch in &word[offset..offset + capacity] {
                    line.push(ch);
                }
                content_len = capacity;
                offset += capacity;
                emit_line(&mut line, &mut content_len, &mut line_number, output);
            } else {
                let available = capacity.saturating_sub(content_len + 1);
                if available >= 2 {
                    line.push(' ');
                    content_len += 1;
                    for &ch in &word[offset..offset + available] {
                        line.push(ch);
                    }
                    content_len += available;
                    offset += available;
                    emit_line(&mut line, &mut content_len, &mut line_number, output);
                } else {
                    emit_line(&mut line, &mut content_len, &mut line_number, output);
                }
            }
        }
    }

    if content_len > 0 {
        output.push(line);
    }
}

fn main() {}