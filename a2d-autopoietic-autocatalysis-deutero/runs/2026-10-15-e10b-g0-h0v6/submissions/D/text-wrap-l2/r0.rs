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
    let mut index = 0;

    while index < chars.len() {
        if chars[index] == '\n' {
            let run_start = index;
            while index < chars.len() && chars[index] == '\n' {
                index += 1;
            }
            if index - run_start >= 2 {
                layout_segment(&chars[segment_start..run_start], options, &mut output);
                for _ in 1..(index - run_start) {
                    output.push(String::new());
                }
                segment_start = index;
            }
        } else {
            index += 1;
        }
    }

    layout_segment(&chars[segment_start..], options, &mut output);
    Ok(output)
}

fn is_separator(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{00a0}')
}

fn layout_segment(chars: &[char], options: &WrapOptions, output: &mut Vec<String>) {
    let words: Vec<Vec<char>> = chars
        .split(|ch| is_separator(*ch))
        .filter(|part| !part.is_empty())
        .map(|part| part.to_vec())
        .collect();

    let mut line = String::new();
    let mut content_len = 0usize;
    let mut line_number = 0usize;

    for word in words {
        let mut offset = 0usize;
        while offset < word.len() {
            let indent = if line_number == 0 {
                options.first_indent
            } else {
                options.rest_indent
            };
            let capacity = options.width - indent;
            let suffix_len = word.len() - offset;

            if suffix_len <= capacity {
                if content_len == 0 {
                    line.extend(std::iter::repeat_n(' ', indent));
                    line.extend(word[offset..].iter());
                    content_len = suffix_len;
                    offset = word.len();
                } else if content_len + 1 + suffix_len <= capacity {
                    line.push(' ');
                    line.extend(word[offset..].iter());
                    content_len += 1 + suffix_len;
                    offset = word.len();
                } else {
                    output.push(std::mem::take(&mut line));
                    content_len = 0;
                    line_number += 1;
                }
            } else if content_len == 0 {
                line.extend(std::iter::repeat_n(' ', indent));
                line.extend(word[offset..offset + capacity].iter());
                output.push(std::mem::take(&mut line));
                offset += capacity;
                line_number += 1;
            } else {
                let available = capacity - content_len - 1;
                if available >= 2 {
                    line.push(' ');
                    line.extend(word[offset..offset + available].iter());
                    output.push(std::mem::take(&mut line));
                    content_len = 0;
                    offset += available;
                    line_number += 1;
                } else {
                    output.push(std::mem::take(&mut line));
                    content_len = 0;
                    line_number += 1;
                }
            }
        }
    }

    if content_len != 0 {
        output.push(line);
    }
}