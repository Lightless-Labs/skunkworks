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

    for (index, ch) in input.char_indices() {
        if ch.is_control() && ch != '\t' && ch != '\r' && ch != '\n' {
            return Err(WrapError::InvalidControl { index });
        }
    }

    let chars: Vec<(usize, char)> = input.char_indices().collect();
    for i in 0..chars.len() {
        if chars[i].1 != '~' {
            continue;
        }
        let valid_side = |ch: char| ch != '~' && !is_separator(ch);
        if i == 0
            || i + 1 == chars.len()
            || !valid_side(chars[i - 1].1)
            || !valid_side(chars[i + 1].1)
        {
            return Err(WrapError::InvalidMarker {
                index: chars[i].0,
            });
        }
    }

    let mut normalized = Vec::new();
    let mut iter = input.chars().peekable();
    while let Some(ch) = iter.next() {
        if ch == '\r' {
            if iter.peek() == Some(&'\n') {
                iter.next();
            }
            normalized.push('\n');
        } else {
            normalized.push(ch);
        }
    }

    let mut output = Vec::new();
    let mut segment = Vec::new();
    let mut i = 0;
    while i < normalized.len() {
        if normalized[i] == '\n' {
            let mut j = i + 1;
            while j < normalized.len() && normalized[j] == '\n' {
                j += 1;
            }
            if j - i >= 2 {
                layout_segment(&segment, options, &mut output);
                segment.clear();
                for _ in 1..(j - i) {
                    output.push(String::new());
                }
                i = j;
                continue;
            }
        }
        segment.push(normalized[i]);
        i += 1;
    }
    layout_segment(&segment, options, &mut output);
    Ok(output)
}

fn is_separator(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{00a0}')
}

#[derive(Clone)]
struct Word {
    chars: Vec<char>,
    markers: Vec<usize>,
}

#[derive(Clone)]
struct Piece {
    text: String,
    words: usize,
}

fn layout_segment(segment: &[char], options: &WrapOptions, output: &mut Vec<String>) {
    let mut words = Vec::<Word>::new();
    let mut current = Vec::new();

    for &ch in segment {
        if is_separator(ch) {
            if !current.is_empty() {
                words.push(make_word(std::mem::take(&mut current)));
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        words.push(make_word(current));
    }
    if words.is_empty() {
        return;
    }

    let mut pieces_per_line: Vec<Vec<Piece>> = Vec::new();
    let mut pieces = Vec::<Piece>::new();
    let mut content_len = 0usize;
    let mut line_index = 0usize;

    for word in &words {
        let mut offset = 0usize;
        while offset < word.chars.len() {
            let indent = if line_index == 0 {
                options.first_indent
            } else {
                options.rest_indent
            };
            let capacity = options.width - indent;
            let gap = usize::from(content_len > 0);
            let room = capacity.saturating_sub(content_len + gap);
            let suffix_len = word.chars.len() - offset;

            if suffix_len <= room {
                if gap > 0 {
                    pieces.push(Piece {
                        text: " ".to_string(),
                        words: 0,
                    });
                    content_len += 1;
                }
                let text: String = word.chars[offset..]
                    .iter()
                    .filter(|&&ch| ch != '~')
                    .collect();
                content_len += text.chars().count();
                pieces.push(Piece { text, words: 1 });
                offset = word.chars.len();
                continue;
            }

            let mut usable = None;
            for &marker in &word.markers {
                if marker <= offset || marker >= word.chars.len() {
                    continue;
                }
                let prefix_len = marker - offset;
                if prefix_len + 1 <= room {
                    usable = Some((marker, prefix_len));
                }
            }

            if let Some((marker, prefix_len)) = usable {
                let remaining = room - prefix_len - 1;
                let accept = if content_len == 0 {
                    true
                } else {
                    remaining * remaining + options.hyphen_cost
                        < (capacity - content_len) * (capacity - content_len)
                };

                if accept {
                    if gap > 0 {
                        pieces.push(Piece {
                            text: " ".to_string(),
                            words: 0,
                        });
                        content_len += 1;
                    }
                    let mut text: String = word.chars[offset..marker]
                        .iter()
                        .filter(|&&ch| ch != '~')
                        .collect();
                    text.push('-');
                    content_len += prefix_len + 1;
                    pieces.push(Piece { text, words: 1 });
                    pieces_per_line.push(std::mem::take(&mut pieces));
                    line_index += 1;
                    content_len = 0;
                    offset = marker + 1;
                    continue;
                }
            }

            if content_len > 0 {
                pieces_per_line.push(std::mem::take(&mut pieces));
                line_index += 1;
                content_len = 0;
                continue;
            }

            if suffix_len > capacity {
                let end = offset + capacity;
                let text: String = word.chars[offset..end]
                    .iter()
                    .filter(|&&ch| ch != '~')
                    .collect();
                pieces.push(Piece { text, words: 1 });
                content_len = capacity;
                pieces_per_line.push(std::mem::take(&mut pieces));
                line_index += 1;
                offset = end;
            } else {
                let text: String = word.chars[offset..]
                    .iter()
                    .filter(|&&ch| ch != '~')
                    .collect();
                content_len += text.chars().count();
                pieces.push(Piece { text, words: 1 });
                offset = word.chars.len();
            }
        }
    }

    if content_len > 0 {
        pieces_per_line.push(pieces);
    }

    let last = pieces_per_line.len().saturating_sub(1);
    for (idx, line_pieces) in pieces_per_line.into_iter().enumerate() {
        let indent = if idx == 0 {
            options.first_indent
        } else {
            options.rest_indent
        };
        let text_width: usize = line_pieces.iter().map(|p| p.text.chars().count()).sum();
        let gaps = line_pieces.iter().filter(|p| p.words == 0).count();
        let word_count = line_pieces.iter().filter(|p| p.words == 1).count();
        let should_justify = options.justify && idx != last && word_count >= 2 && gaps > 0;
        let extra = if should_justify {
            options.width - indent - text_width
        } else {
            0
        };
        let base = if gaps > 0 { extra / gaps } else { 0 };
        let remainder = if gaps > 0 { extra % gaps } else { 0 };

        let mut rendered = " ".repeat(indent);
        let mut gap_index = 0usize;
        for piece in line_pieces {
            if piece.words == 0 {
                let add = if should_justify {
                    base + usize::from(
                        (gap_index + gaps - (idx % gaps)) % gaps < remainder
                    )
                } else {
                    0
                };
                rendered.push_str(&" ".repeat(1 + add));
                gap_index += 1;
            } else {
                rendered.push_str(&piece.text);
            }
        }
        output.push(rendered);
    }
}

fn make_word(chars: Vec<char>) -> Word {
    let markers = chars
        .iter()
        .enumerate()
        .filter_map(|(i, &ch)| (ch == '~').then_some(i))
        .collect();
    Word { chars, markers }
}

fn main() {}