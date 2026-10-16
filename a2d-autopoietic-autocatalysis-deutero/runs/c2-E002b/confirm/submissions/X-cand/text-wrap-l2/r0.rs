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
    let mut segment_start = 0;
    let mut iter = normalized.char_indices().peekable();

    while let Some((index, ch)) = iter.next() {
        if ch != '\n' {
            continue;
        }

        let mut count = 1;
        while iter.peek().is_some_and(|(_, next)| *next == '\n') {
            iter.next();
            count += 1;
        }

        if count >= 2 {
            wrap_segment(&normalized[segment_start..index], options, &mut output);
            output.extend((1..count).map(|_| String::new()));
            segment_start = iter
                .peek()
                .map(|(next_index, _)| *next_index)
                .unwrap_or(normalized.len());
        }
    }

    wrap_segment(&normalized[segment_start..], options, &mut output);
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

    let mut line = String::new();
    let mut content_length = 0usize;
    let mut line_number = 0usize;

    for word in words {
        let mut consumed = 0usize;

        while consumed < word.len() {
            let indent = if line_number == 0 {
                options.first_indent
            } else {
                options.rest_indent
            };
            let capacity = options.width - indent;
            let suffix_length = word.len() - consumed;

            if suffix_length <= capacity {
                if line.is_empty() {
                    line.extend(word[consumed..].iter());
                    content_length += suffix_length;
                    consumed = word.len();
                } else if content_length + 1 + suffix_length <= capacity {
                    line.push(' ');
                    line.extend(word[consumed..].iter());
                    content_length += 1 + suffix_length;
                    consumed = word.len();
                } else {
                    emit_line(
                        &mut line,
                        &mut content_length,
                        &mut line_number,
                        indent,
                        output,
                    );
                }
            } else if line.is_empty() {
                line.extend(word[consumed..consumed + capacity].iter());
                consumed += capacity;
                content_length = capacity;
                emit_line(
                    &mut line,
                    &mut content_length,
                    &mut line_number,
                    indent,
                    output,
                );
            } else {
                let available = capacity.saturating_sub(content_length + 1);
                if available >= 2 {
                    line.push(' ');
                    line.extend(word[consumed..consumed + available].iter());
                    consumed += available;
                    content_length += 1 + available;
                }
                emit_line(
                    &mut line,
                    &mut content_length,
                    &mut line_number,
                    indent,
                    output,
                );
            }
        }
    }

    if !line.is_empty() {
        let indent = if line_number == 0 {
            options.first_indent
        } else {
            options.rest_indent
        };
        let mut output_line = " ".repeat(indent);
        output_line.push_str(&line);
        output.push(output_line);
    }
}

fn emit_line(
    line: &mut String,
    content_length: &mut usize,
    line_number: &mut usize,
    indent: usize,
    output: &mut Vec<String>,
) {
    let mut output_line = " ".repeat(indent);
    output_line.push_str(line);
    output.push(output_line);
    line.clear();
    *content_length = 0;
    *line_number += 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_unicode_words() {
        assert_eq!(
            wrap(
                "猫犬 鳥魚",
                &WrapOptions {
                    width: 5,
                    first_indent: 0,
                    rest_indent: 0
                }
            ),
            Ok(vec!["猫犬 鳥魚".to_string()])
        );
    }

    #[test]
    fn public_hanging() {
        assert_eq!(
            wrap(
                "a bb ccc dd",
                &WrapOptions {
                    width: 7,
                    first_indent: 1,
                    rest_indent: 2
                }
            ),
            Ok(vec![
                " a bb".to_string(),
                "  ccc".to_string(),
                "  dd".to_string()
            ])
        );
    }

    #[test]
    fn public_paragraph() {
        assert_eq!(
            wrap(
                "a\n\nb",
                &WrapOptions {
                    width: 4,
                    first_indent: 0,
                    rest_indent: 1
                }
            ),
            Ok(vec!["a".to_string(), "".to_string(), "b".to_string()])
        );
    }

    #[test]
    fn public_bad_indent() {
        assert_eq!(
            wrap(
                "x",
                &WrapOptions {
                    width: 3,
                    first_indent: 3,
                    rest_indent: 0
                }
            ),
            Err(WrapError::FirstIndentTooWide { indent: 3 })
        );
    }
}