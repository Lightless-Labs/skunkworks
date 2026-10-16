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
        let valid_side = |j: usize| {
            j < chars.len() && chars[j].1 != '~' && !is_separator(chars[j].1)
        };
        if i == 0 || i + 1 >= chars.len() || !valid_side(i - 1) || !valid_side(i + 1) {
            return Err(WrapError::InvalidMarker { index: chars[i].0 });
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
    let mut segment_start = 0;
    let mut i = 0;
    while i < normalized.len() {
        if normalized[i] == '\n' {
            let run_start = i;
            while i < normalized.len() && normalized[i] == '\n' {
                i += 1;
            }
            if i - run_start >= 2 {
                layout_segment(&normalized[segment_start..run_start], options, &mut output);
                for _ in 1..(i - run_start) {
                    output.push(String::new());
                }
                segment_start = i;
            }
        } else {
            i += 1;
        }
    }
    layout_segment(&normalized[segment_start..], options, &mut output);

    Ok(output)
}

fn is_separator(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{00a0}')
}

#[derive(Clone)]
struct Word {
    chars: Vec<char>,
    breaks: Vec<usize>,
}

fn layout_segment(segment: &[char], options: &WrapOptions, output: &mut Vec<String>) {
    let mut words = Vec::new();
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

    let mut lines: Vec<Line> = Vec::new();
    let mut line = Line {
        indent: options.first_indent,
        pieces: Vec::new(),
        gaps: Vec::new(),
    };

    for word in words {
        let mut offset = 0;
        while offset < word.chars.len() {
            let capacity = options.width - line.indent;
            let content_len = line.content_len();
            let room = capacity.saturating_sub(content_len + usize::from(!line.pieces.is_empty()));

            let suffix_len = word.chars.len() - offset;
            if suffix_len <= room {
                if !line.pieces.is_empty() {
                    line.gaps.push(1);
                }
                line.pieces.push(word.chars[offset..].iter().collect());
                offset = word.chars.len();
                continue;
            }

            let mut usable = None;
            for &position in &word.breaks {
                if position > offset && position < word.chars.len() {
                    let prefix_len = position - offset;
                    if prefix_len + 1 <= room {
                        usable = Some((position, prefix_len));
                    }
                }
            }

            if let Some((position, prefix_len)) = usable {
                let unused = room - prefix_len - 1;
                let accept = line.pieces.is_empty()
                    || unused * unused + options.hyphen_cost
                        < capacity.saturating_sub(content_len).pow(2);
                if accept {
                    if !line.pieces.is_empty() {
                        line.gaps.push(1);
                    }
                    let mut piece: String = word.chars[offset..position].iter().collect();
                    piece.push('-');
                    line.pieces.push(piece);
                    lines.push(line);
                    line = Line {
                        indent: options.rest_indent,
                        pieces: Vec::new(),
                        gaps: Vec::new(),
                    };
                    offset = position;
                    continue;
                }
            }

            if !line.pieces.is_empty() {
                lines.push(line);
                line = Line {
                    indent: options.rest_indent,
                    pieces: Vec::new(),
                    gaps: Vec::new(),
                };
                continue;
            }

            if suffix_len > capacity {
                let end = offset + capacity;
                line.pieces.push(word.chars[offset..end].iter().collect());
                lines.push(line);
                line = Line {
                    indent: options.rest_indent,
                    pieces: Vec::new(),
                    gaps: Vec::new(),
                };
                offset = end;
            } else {
                line.pieces.push(word.chars[offset..].iter().collect());
                offset = word.chars.len();
            }
        }
    }

    if !line.pieces.is_empty() {
        lines.push(line);
    }

    let line_count = lines.len();
    for (line_index, line) in lines.into_iter().enumerate() {
        let content_len = line.content_len();
        let gap_count = line.gaps.len();
        let justify = options.justify && line_index + 1 < line_count && gap_count > 0;
        let extra = if justify {
            options.width - line.indent - content_len
        } else {
            0
        };
        let base = if gap_count > 0 { extra / gap_count } else { 0 };
        let remainder = if gap_count > 0 { extra % gap_count } else { 0 };
        let start_gap = if gap_count > 0 {
            line_index % gap_count
        } else {
            0
        };

        let mut rendered = " ".repeat(line.indent);
        for (piece_index, piece) in line.pieces.iter().enumerate() {
            if piece_index > 0 {
                let mut spaces = line.gaps[piece_index - 1];
                if justify {
                    spaces += base;
                    let gap = piece_index - 1;
                    if (gap + gap_count - start_gap) % gap_count < remainder {
                        spaces += 1;
                    }
                }
                rendered.push_str(&" ".repeat(spaces));
            }
            rendered.push_str(piece);
        }
        output.push(rendered);
    }
}

fn make_word(raw: Vec<char>) -> Word {
    let mut chars = Vec::with_capacity(raw.len());
    let mut breaks = Vec::new();
    for ch in raw {
        if ch == '~' {
            breaks.push(chars.len());
        } else {
            chars.push(ch);
        }
    }
    Word { chars, breaks }
}

struct Line {
    indent: usize,
    pieces: Vec<String>,
    gaps: Vec<usize>,
}

impl Line {
    fn content_len(&self) -> usize {
        self.pieces.iter().map(|piece| piece.chars().count()).sum::<usize>()
            + self.gaps.iter().sum::<usize>()
    }
}