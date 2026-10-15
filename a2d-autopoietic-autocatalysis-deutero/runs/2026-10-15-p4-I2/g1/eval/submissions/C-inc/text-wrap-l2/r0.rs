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

    let mut first_line = true;
    let mut content = String::new();
    let mut content_len = 0usize;

    for mut word in words {
        while !word.is_empty() {
            let indent = if first_line {
                options.first_indent
            } else {
                options.rest_indent
            };
            let capacity = options.width - indent;
            let word_len = word.len();

            if word_len <= capacity {
                if content_len == 0 {
                    content.extend(word.iter());
                    content_len = word_len;
                    word.clear();
                } else if content_len + 1 + word_len <= capacity {
                    content.push(' ');
                    content.extend(word.iter());
                    content_len += 1 + word_len;
                    word.clear();
                } else {
                    emit_line(
                        &mut content,
                        indent,
                        output,
                    );
                    content_len = 0;
                    first_line = false;
                }
            } else if content_len == 0 {
                content.extend(word.iter().take(capacity));
                word.drain(..capacity);
                content_len = capacity;
                emit_line(&mut content, indent, output);
                content_len = 0;
                first_line = false;
            } else {
                let available = capacity.saturating_sub(content_len + 1);
                if available >= 2 {
                    content.push(' ');
                    content.extend(word.iter().take(available));
                    word.drain(..available);
                    content_len += 1 + available;
                    emit_line(&mut content, indent, output);
                    content_len = 0;
                    first_line = false;
                } else {
                    emit_line(&mut content, indent, output);
                    content_len = 0;
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
        emit_line(&mut content, indent, output);
    }
}

fn emit_line(content: &mut String, indent: usize, output: &mut Vec<String>) {
    let mut line = String::with_capacity(indent + content.len());
    line.extend(std::iter::repeat(' ').take(indent));
    line.push_str(content);
    output.push(line);
    content.clear();
}

fn main() {}