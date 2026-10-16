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
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{00a0}')
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
    let mut indent = options.first_indent;
    let mut line_content: Vec<char> = Vec::new();

    for word in words {
        let mut pos = 0;

        while pos < word.len() {
            let capacity = options.width - indent;

            if line_content.is_empty() {
                let remaining = word.len() - pos;
                if remaining <= capacity {
                    line_content.extend_from_slice(&word[pos..]);
                    pos = word.len();
                } else {
                    line_content.extend_from_slice(&word[pos..pos + capacity]);
                    pos += capacity;
                    emit_line(&line_content, indent, output);
                    line_content.clear();
                    first_line = false;
                    indent = options.rest_indent;
                }
                continue;
            }

            let remaining = word.len() - pos;
            if remaining <= capacity {
                if line_content.len() + 1 + remaining <= capacity {
                    line_content.push(' ');
                    line_content.extend_from_slice(&word[pos..]);
                    pos = word.len();
                } else {
                    emit_line(&line_content, indent, output);
                    line_content.clear();
                    first_line = false;
                    indent = options.rest_indent;
                }
            } else {
                let available = capacity - line_content.len() - 1;
                if available >= 2 {
                    line_content.push(' ');
                    line_content.extend_from_slice(&word[pos..pos + available]);
                    pos += available;
                    emit_line(&line_content, indent, output);
                    line_content.clear();
                    first_line = false;
                    indent = options.rest_indent;
                } else {
                    emit_line(&line_content, indent, output);
                    line_content.clear();
                    first_line = false;
                    indent = options.rest_indent;
                }
            }
        }
    }

    if !line_content.is_empty() {
        emit_line(&line_content, indent, output);
    }

    let _ = first_line;
}

fn emit_line(content: &[char], indent: usize, output: &mut Vec<String>) {
    let mut line = String::with_capacity(indent + content.len());
    line.extend(std::iter::repeat(' ').take(indent));
    line.extend(content.iter());
    output.push(line);
}