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

#[derive(Clone)]
struct Word {
    chars: Vec<char>,
    breaks: Vec<usize>,
}

#[derive(Clone)]
struct Line {
    indent: usize,
    pieces: Vec<String>,
}

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{00a0}')
}

fn parse_segment(chars: &[char]) -> Vec<Word> {
    let mut words = Vec::new();
    let mut current = Vec::new();
    let mut breaks = Vec::new();

    for &ch in chars {
        if is_separator(ch) {
            if !current.is_empty() {
                words.push(Word {
                    chars: std::mem::take(&mut current),
                    breaks: std::mem::take(&mut breaks),
                });
            }
        } else if ch == '~' {
            breaks.push(current.len());
        } else {
            current.push(ch);
        }
    }

    if !current.is_empty() {
        words.push(Word {
            chars: current,
            breaks,
        });
    }
    words
}

fn text(chars: &[char]) -> String {
    chars.iter().collect()
}

fn line_content_len(line: &Line) -> usize {
    line.pieces.iter().map(|piece| piece.chars().count()).sum()
}

fn render_line(
    line: &Line,
    width: usize,
    justify: bool,
    is_final: bool,
    paragraph_line_index: usize,
) -> String {
    let piece_width = line_content_len(line);
    let gaps = line.pieces.len().saturating_sub(1);
    let pre_width = line.indent + piece_width + gaps;
    let extra = if justify && !is_final && line.pieces.len() >= 2 {
        width - pre_width
    } else {
        0
    };

    let each = if gaps > 0 { extra / gaps } else { 0 };
    let remainder = if gaps > 0 { extra % gaps } else { 0 };
    let start_gap = if gaps > 0 {
        paragraph_line_index % gaps
    } else {
        0
    };

    let mut rendered = " ".repeat(line.indent);
    for (i, piece) in line.pieces.iter().enumerate() {
        if i > 0 {
            let gap = i - 1;
            let gets_remainder =
                (gap + gaps - start_gap) % gaps < remainder;
            rendered.push_str(&" ".repeat(1 + each + usize::from(gets_remainder)));
        }
        rendered.push_str(piece);
    }
    rendered
}

fn layout_paragraph(words: &[Word], options: &WrapOptions) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = Line {
        indent: options.first_indent,
        pieces: Vec::new(),
    };
    let mut word_index = 0;
    let mut offset = 0;

    while word_index < words.len() {
        let word = &words[word_index];
        let capacity = options.width - line.indent;
        let current_len = line_content_len(&line);
        let suffix_len = word.chars.len() - offset;
        let room = capacity.saturating_sub(current_len + usize::from(!line.pieces.is_empty()));

        if suffix_len <= room {
            line.pieces.push(text(&word.chars[offset..]));
            word_index += 1;
            offset = 0;
            continue;
        }

        let available = capacity - current_len;
        let opportunity = word
            .breaks
            .iter()
            .copied()
            .filter(|&at| {
                at > offset && at < word.chars.len() && at - offset + 1 <= available
            })
            .last();

        let accepted = opportunity.filter(|&at| {
            if line.pieces.is_empty() {
                true
            } else {
                let used = at - offset + 1;
                let unused = available - used;
                let baseline_unused = capacity - current_len;
                unused * unused + options.hyphen_cost < baseline_unused * baseline_unused
            }
        });

        if let Some(at) = accepted {
            let mut piece = text(&word.chars[offset..at]);
            piece.push('-');
            line.pieces.push(piece);
            lines.push(line);
            line = Line {
                indent: options.rest_indent,
                pieces: Vec::new(),
            };
            offset = at;
            continue;
        }

        if !line.pieces.is_empty() {
            lines.push(line);
            line = Line {
                indent: options.rest_indent,
                pieces: Vec::new(),
            };
            continue;
        }

        if suffix_len > capacity {
            line.pieces.push(text(&word.chars[offset..offset + capacity]));
            lines.push(line);
            line = Line {
                indent: options.rest_indent,
                pieces: Vec::new(),
            };
            offset += capacity;
        } else {
            line.pieces.push(text(&word.chars[offset..]));
            word_index += 1;
            offset = 0;
        }
    }

    if !line.pieces.is_empty() {
        lines.push(line);
    }

    let final_index = lines.len().saturating_sub(1);
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            render_line(
                line,
                options.width,
                options.justify,
                index == final_index,
                index,
            )
        })
        .collect()
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

    let original: Vec<(usize, char)> = input.char_indices().collect();
    for &(index, ch) in &original {
        if ch.is_control() && !matches!(ch, '\t' | '\r' | '\n') {
            return Err(WrapError::InvalidControl { index });
        }
    }

    for i in 0..original.len() {
        if original[i].1 == '~' {
            let left_ok = i > 0
                && original[i - 1].1 != '~'
                && !is_separator(original[i - 1].1);
            let right_ok = i + 1 < original.len()
                && original[i + 1].1 != '~'
                && !is_separator(original[i + 1].1);
            if !left_ok || !right_ok {
                return Err(WrapError::InvalidMarker {
                    index: original[i].0,
                });
            }
        }
    }

    let mut normalized = Vec::new();
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

    let mut segments = Vec::new();
    let mut boundary_counts = Vec::new();
    let mut segment = Vec::new();
    let mut i = 0;

    while i < normalized.len() {
        if normalized[i] == '\n' {
            let mut end = i;
            while end < normalized.len() && normalized[end] == '\n' {
                end += 1;
            }
            if end - i >= 2 {
                segments.push(std::mem::take(&mut segment));
                boundary_counts.push(end - i - 1);
                i = end;
                continue;
            }
        }
        segment.push(normalized[i]);
        i += 1;
    }
    segments.push(segment);

    let mut output = Vec::new();
    for (segment_index, segment) in segments.iter().enumerate() {
        let words = parse_segment(segment);
        if !words.is_empty() {
            output.extend(layout_paragraph(&words, options));
        }
        if segment_index < boundary_counts.len() {
            output.extend((0..boundary_counts[segment_index]).map(|_| String::new()));
        }
    }

    Ok(output)
}

fn main() {}