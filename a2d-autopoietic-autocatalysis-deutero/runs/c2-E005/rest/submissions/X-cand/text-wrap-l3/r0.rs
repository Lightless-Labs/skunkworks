#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WrapError {
    WidthZero,
    WidthTooLarge { width: usize },
    FirstIndentTooWide { indent: usize },
    RestIndentTooWide { indent: usize },
    HyphenCostTooLarge { cost: usize },
    InvalidControl { index: usize },
    InvalidMarker { index: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrapOptions {
    pub width: usize,
    pub first_indent: usize,
    pub rest_indent: usize,
    pub hyphen_cost: usize,
    pub justify: bool,
}

#[derive(Clone, Copy)]
struct Scalar {
    ch: char,
    byte_index: usize,
}

fn is_separator(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{00A0}')
}

fn append_spaces(output: &mut String, count: usize) {
    output.extend(std::iter::repeat_n(' ', count));
}

fn finish_line(
    indent: usize,
    text: &str,
    pieces: &[String],
    is_final: bool,
    line_index: usize,
    width: usize,
    justify: bool,
    output: &mut Vec<String>,
) {
    let mut line = String::new();
    append_spaces(&mut line, indent);

    if justify && !is_final && pieces.len() >= 2 {
        let gaps = pieces.len() - 1;
        let base_width = pieces.iter().map(|piece| piece.chars().count()).sum::<usize>()
            + gaps;
        let extra = width - indent - base_width;
        let each = extra / gaps;
        let remainder = extra % gaps;
        let start_gap = line_index % gaps;

        for (index, piece) in pieces.iter().enumerate() {
            line.push_str(piece);
            if index < gaps {
                let cyclic_index = (index + gaps - start_gap) % gaps;
                let spaces = 1 + each + usize::from(cyclic_index < remainder);
                append_spaces(&mut line, spaces);
            }
        }
    } else {
        line.push_str(text);
    }

    output.push(line);
}

fn layout_paragraph(
    paragraph: &[char],
    options: &WrapOptions,
    output: &mut Vec<String>,
) {
    let mut words: Vec<Vec<char>> = Vec::new();
    let mut current_word = Vec::new();

    for &ch in paragraph {
        if is_separator(ch) {
            if !current_word.is_empty() {
                words.push(std::mem::take(&mut current_word));
            }
        } else if ch != '~' {
            current_word.push(ch);
        } else {
            current_word.push(ch);
        }
    }
    if !current_word.is_empty() {
        words.push(current_word);
    }

    if words.is_empty() {
        return;
    }

    let mut first_line = true;
    let mut line_text = String::new();
    let mut line_pieces: Vec<String> = Vec::new();
    let mut line_content_len = 0usize;
    let mut line_indent = options.first_indent;
    let mut line_index = 0usize;

    let emit = |text: &str,
                pieces: &[String],
                is_final: bool,
                indent: usize,
                line_index: usize,
                output: &mut Vec<String>| {
        finish_line(
            indent,
            text,
            pieces,
            is_final,
            line_index,
            options.width,
            options.justify,
            output,
        );
    };

    for word in &words {
        let mut offset = 0usize;

        while offset < word.len() {
            if line_content_len > 0 {
                let room = options.width - line_indent - line_content_len;
                let suffix_len = word[offset..].iter().filter(|&&ch| ch != '~').count();

                if suffix_len + 1 <= room {
                    line_text.push(' ');
                    line_text.extend(word[offset..].iter().copied().filter(|&ch| ch != '~'));
                    line_content_len += suffix_len + 1;
                    line_pieces.push(
                        word[offset..]
                            .iter()
                            .copied()
                            .filter(|&ch| ch != '~')
                            .collect(),
                    );
                    offset = word.len();
                    continue;
                }

                let content_room = room.saturating_sub(1);
                let mut chosen: Option<(usize, usize)> = None;
                let mut prefix_len = 0usize;
                for marker in offset..word.len() {
                    if word[marker] == '~' {
                        if marker > offset && marker + 1 < word.len() && prefix_len + 1 <= content_room {
                            chosen = Some((marker, prefix_len));
                        }
                    } else {
                        prefix_len += 1;
                    }
                }

                let capacity = options.width - line_indent;
                if let Some((marker, prefix_len)) = chosen {
                    let used = prefix_len + 1;
                    let unused = content_room - used;
                    let left_room = capacity - line_content_len;
                    if unused
                        .checked_mul(unused)
                        .and_then(|value| value.checked_add(options.hyphen_cost))
                        .is_some_and(|cost| cost < left_room.saturating_mul(left_room))
                    {
                        line_text.push(' ');
                        let fragment: String = word[offset..marker]
                            .iter()
                            .copied()
                            .filter(|&ch| ch != '~')
                            .collect();
                        line_text.push_str(&fragment);
                        line_text.push('-');
                        line_content_len += used + 1;
                        line_pieces.push(format!("{fragment}-"));
                        emit(
                            &line_text,
                            &line_pieces,
                            false,
                            line_indent,
                            line_index,
                            output,
                        );
                        line_index += 1;
                        first_line = false;
                        line_indent = options.rest_indent;
                        line_text.clear();
                        line_pieces.clear();
                        line_content_len = 0;
                        offset = marker + 1;
                        continue;
                    }
                }

                emit(
                    &line_text,
                    &line_pieces,
                    false,
                    line_indent,
                    line_index,
                    output,
                );
                line_index += 1;
                first_line = false;
                line_indent = options.rest_indent;
                line_text.clear();
                line_pieces.clear();
                line_content_len = 0;
                continue;
            }

            let capacity = options.width - line_indent;
            let suffix_len = word[offset..].iter().filter(|&&ch| ch != '~').count();
            if suffix_len <= capacity {
                let piece: String = word[offset..]
                    .iter()
                    .copied()
                    .filter(|&ch| ch != '~')
                    .collect();
                line_content_len += piece.chars().count();
                line_text.push_str(&piece);
                line_pieces.push(piece);
                offset = word.len();
                continue;
            }

            let mut chosen: Option<(usize, usize)> = None;
            let mut prefix_len = 0usize;
            for marker in offset..word.len() {
                if word[marker] == '~' {
                    if marker > offset && marker + 1 < word.len() && prefix_len + 1 <= capacity {
                        chosen = Some((marker, prefix_len));
                    }
                } else {
                    prefix_len += 1;
                }
            }

            if let Some((marker, prefix_len)) = chosen {
                let fragment: String = word[offset..marker]
                    .iter()
                    .copied()
                    .filter(|&ch| ch != '~')
                    .collect();
                let piece = format!("{fragment}-");
                line_content_len += prefix_len + 1;
                line_text.push_str(&piece);
                line_pieces.push(piece);
                emit(
                    &line_text,
                    &line_pieces,
                    false,
                    line_indent,
                    line_index,
                    output,
                );
                line_index += 1;
                first_line = false;
                line_indent = options.rest_indent;
                line_text.clear();
                line_pieces.clear();
                line_content_len = 0;
                offset = marker + 1;
                continue;
            }

            let mut consumed = 0usize;
            let mut raw = String::new();
            let mut next_offset = offset;
            while next_offset < word.len() && consumed < capacity {
                if word[next_offset] != '~' {
                    raw.push(word[next_offset]);
                    consumed += 1;
                }
                next_offset += 1;
            }
            line_text.push_str(&raw);
            line_content_len += consumed;
            line_pieces.push(raw);
            emit(
                &line_text,
                &line_pieces,
                false,
                line_indent,
                line_index,
                output,
            );
            line_index += 1;
            first_line = false;
            line_indent = options.rest_indent;
            line_text.clear();
            line_pieces.clear();
            line_content_len = 0;
            offset = next_offset;
        }
    }

    if line_content_len > 0 {
        emit(
            &line_text,
            &line_pieces,
            true,
            line_indent,
            line_index,
            output,
        );
    }

    let _ = first_line;
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
    if options.hyphen_cost > 1_000_000 {
        return Err(WrapError::HyphenCostTooLarge {
            cost: options.hyphen_cost,
        });
    }

    let scalars: Vec<Scalar> = input
        .char_indices()
        .map(|(byte_index, ch)| Scalar { ch, byte_index })
        .collect();

    for scalar in &scalars {
        if scalar.ch.is_control() && !matches!(scalar.ch, '\t' | '\r' | '\n') {
            return Err(WrapError::InvalidControl {
                index: scalar.byte_index,
            });
        }
    }

    for index in 0..scalars.len() {
        if scalars[index].ch == '~' {
            let valid_neighbor = |neighbor: Option<&Scalar>| {
                neighbor.is_some_and(|scalar| scalar.ch != '~' && !is_separator(scalar.ch))
            };
            if !valid_neighbor(index.checked_sub(1).and_then(|i| scalars.get(i)))
                || !valid_neighbor(scalars.get(index + 1))
            {
                return Err(WrapError::InvalidMarker {
                    index: scalars[index].byte_index,
                });
            }
        }
    }

    let mut normalized = Vec::new();
    let mut index = 0usize;
    while index < scalars.len() {
        if scalars[index].ch == '\r' {
            if index + 1 < scalars.len() && scalars[index + 1].ch == '\n' {
                normalized.push('\n');
                index += 2;
            } else {
                normalized.push('\n');
                index += 1;
            }
        } else {
            normalized.push(scalars[index].ch);
            index += 1;
        }
    }

    let mut output = Vec::new();
    let mut segment = Vec::new();
    let mut cursor = 0usize;

    while cursor < normalized.len() {
        if normalized[cursor] == '\n' {
            let start = cursor;
            while cursor < normalized.len() && normalized[cursor] == '\n' {
                cursor += 1;
            }
            let run_len = cursor - start;
            if run_len >= 2 {
                layout_paragraph(&segment, options, &mut output);
                segment.clear();
                output.extend(std::iter::repeat_n(String::new(), run_len - 1));
            } else {
                segment.push('\n');
            }
        } else {
            segment.push(normalized[cursor]);
            cursor += 1;
        }
    }
    layout_paragraph(&segment, options, &mut output);

    Ok(output)
}