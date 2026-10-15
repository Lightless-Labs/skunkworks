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
    let is_separator = |ch: char| matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{00A0}');

    for i in 0..chars.len() {
        if chars[i].1 == '~'
            && (i == 0
                || i + 1 == chars.len()
                || chars[i - 1].1 == '~'
                || chars[i + 1].1 == '~'
                || is_separator(chars[i - 1].1)
                || is_separator(chars[i + 1].1))
        {
            return Err(WrapError::InvalidMarker { index: chars[i].0 });
        }
    }

    let mut normalized = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i].1;
        if ch == '\r' {
            normalized.push('\n');
            if i + 1 < chars.len() && chars[i + 1].1 == '\n' {
                i += 1;
            }
        } else {
            normalized.push(ch);
        }
        i += 1;
    }

    let mut output = Vec::new();
    let mut segment_start = 0;
    let mut pos = 0;
    while pos < normalized.len() {
        if normalized[pos] == '\n' {
            let run_start = pos;
            while pos < normalized.len() && normalized[pos] == '\n' {
                pos += 1;
            }
            if pos - run_start >= 2 {
                layout_segment(&normalized[segment_start..run_start], options, &mut output);
                for _ in 1..(pos - run_start) {
                    output.push(String::new());
                }
                segment_start = pos;
            }
        } else {
            pos += 1;
        }
    }
    layout_segment(&normalized[segment_start..], options, &mut output);

    Ok(output)
}

#[derive(Debug, Clone)]
struct Line {
    pieces: Vec<String>,
    indent: usize,
    content_len: usize,
}

fn layout_segment(segment: &[char], options: &WrapOptions, output: &mut Vec<String>) {
    let mut words: Vec<Vec<char>> = Vec::new();
    let mut current = Vec::new();

    for &ch in segment {
        if matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{00A0}') {
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
    if words.is_empty() {
        return;
    }

    let mut lines = Vec::<Line>::new();
    let mut line_index = 0usize;
    let mut line = Line {
        pieces: Vec::new(),
        indent: options.first_indent,
        content_len: 0,
    };

    for word in words {
        let mut offset = 0usize;
        while offset < word.len() {
            let capacity = options.width - line.indent;
            let remaining = word.len() - offset;
            let gap = usize::from(!line.pieces.is_empty());
            let room = capacity.saturating_sub(line.content_len + gap);

            if remaining <= room {
                if gap != 0 {
                    line.content_len += 1;
                }
                let text: String = word[offset..]
                    .iter()
                    .copied()
                    .filter(|&ch| ch != '~')
                    .collect();
                line.content_len += text.chars().count();
                line.pieces.push(text);
                offset = word.len();
                continue;
            }

            let mut opportunities = Vec::new();
            for marker in (offset + 1)..word.len() {
                if word[marker] == '~' {
                    let prefix_len = marker - offset;
                    if prefix_len + 1 <= room {
                        opportunities.push(marker);
                    }
                }
            }

            let selected = opportunities.last().copied();
            if let Some(marker) = selected {
                let prefix_len = marker - offset;
                let unused = room - prefix_len - 1;
                let use_opportunity = if line.pieces.is_empty() {
                    true
                } else {
                    let baseline = capacity.saturating_sub(line.content_len);
                    let lhs = (unused as u128) * (unused as u128)
                        + options.hyphen_cost as u128;
                    let rhs = (baseline as u128) * (baseline as u128);
                    lhs < rhs
                };

                if use_opportunity {
                    if gap != 0 {
                        line.content_len += 1;
                    }
                    let mut text: String = word[offset..marker]
                        .iter()
                        .copied()
                        .filter(|&ch| ch != '~')
                        .collect();
                    text.push('-');
                    line.content_len += prefix_len + 1;
                    line.pieces.push(text);
                    lines.push(line);
                    line_index += 1;
                    line = Line {
                        pieces: Vec::new(),
                        indent: options.rest_indent,
                        content_len: 0,
                    };
                    offset = marker + 1;
                    continue;
                }
            }

            if !line.pieces.is_empty() {
                lines.push(line);
                line_index += 1;
                line = Line {
                    pieces: Vec::new(),
                    indent: options.rest_indent,
                    content_len: 0,
                };
                continue;
            }

            if remaining > capacity {
                let end = offset + capacity;
                let text: String = word[offset..end]
                    .iter()
                    .copied()
                    .filter(|&ch| ch != '~')
                    .collect();
                line.content_len = text.chars().count();
                line.pieces.push(text);
                lines.push(line);
                line_index += 1;
                line = Line {
                    pieces: Vec::new(),
                    indent: options.rest_indent,
                    content_len: 0,
                };
                offset = end;
            } else {
                let text: String = word[offset..]
                    .iter()
                    .copied()
                    .filter(|&ch| ch != '~')
                    .collect();
                line.content_len = text.chars().count();
                line.pieces.push(text);
                offset = word.len();
            }
        }
    }

    if !line.pieces.is_empty() {
        lines.push(line);
    }

    let final_line = lines.len().saturating_sub(1);
    for (index, line) in lines.into_iter().enumerate() {
        let gaps = line.pieces.len().saturating_sub(1);
        let mut rendered = " ".repeat(line.indent);

        if options.justify && index != final_line && gaps >= 1 {
            let extra = options.width.saturating_sub(line.indent + line.content_len);
            let base = extra / gaps;
            let remainder = extra % gaps;
            let start_gap = index % gaps;

            for (piece_index, piece) in line.pieces.iter().enumerate() {
                rendered.push_str(piece);
                if piece_index < gaps {
                    let additional = base
                        + usize::from((piece_index + gaps - start_gap) % gaps < remainder);
                    rendered.push_str(&" ".repeat(1 + additional));
                }
            }
        } else {
            for (piece_index, piece) in line.pieces.iter().enumerate() {
                if piece_index > 0 {
                    rendered.push(' ');
                }
                rendered.push_str(piece);
            }
        }
        output.push(rendered);
    }

    let _ = line_index;
}