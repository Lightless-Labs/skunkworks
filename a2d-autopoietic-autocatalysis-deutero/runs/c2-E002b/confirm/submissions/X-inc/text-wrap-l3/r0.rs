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

struct Line {
    indent: usize,
    pieces: Vec<String>,
}

impl Line {
    fn content_len(&self) -> usize {
        self.pieces
            .iter()
            .map(|piece| piece.chars().count())
            .sum::<usize>()
            + self.pieces.len().saturating_sub(1)
    }

    fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }

    fn append_piece(&mut self, text: String) {
        self.pieces.push(text);
    }
}

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{00a0}')
}

fn new_line(indent: usize) -> Line {
    Line {
        indent,
        pieces: Vec::new(),
    }
}

fn render_line(line: &Line, width: usize, justify: bool, line_index: usize) -> String {
    let gaps = line.pieces.len().saturating_sub(1);
    let content_len = line.content_len();
    let extra = if justify && gaps > 0 {
        width - line.indent - content_len
    } else {
        0
    };
    let each = if gaps > 0 { extra / gaps } else { 0 };
    let remainder = if gaps > 0 { extra % gaps } else { 0 };
    let start_gap = if gaps > 0 { line_index % gaps } else { 0 };

    let mut rendered = " ".repeat(line.indent);
    for (piece_index, piece) in line.pieces.iter().enumerate() {
        if piece_index > 0 {
            let gap_index = piece_index - 1;
            let bonus = usize::from(
                remainder > 0 && (gap_index + gaps - start_gap) % gaps < remainder,
            );
            rendered.push_str(&" ".repeat(1 + each + bonus));
        }
        rendered.push_str(piece);
    }
    rendered
}

fn parse_words(segment: &[char]) -> Vec<Word> {
    let mut words = Vec::new();
    let mut chars = Vec::new();
    let mut breaks = Vec::new();

    for &c in segment {
        if is_separator(c) {
            if !chars.is_empty() {
                words.push(Word {
                    chars: std::mem::take(&mut chars),
                    breaks: std::mem::take(&mut breaks),
                });
            }
        } else if c == '~' {
            breaks.push(chars.len());
        } else {
            chars.push(c);
        }
    }

    if !chars.is_empty() {
        words.push(Word { chars, breaks });
    }
    words
}

fn layout_segment(segment: &[char], options: &WrapOptions) -> Vec<String> {
    let words = parse_words(segment);
    if words.is_empty() {
        return Vec::new();
    }

    let mut lines = Vec::new();
    let mut line = new_line(options.first_indent);
    let mut line_index = 0usize;

    for word in &words {
        let mut offset = 0;

        while offset < word.chars.len() {
            let capacity = options.width - line.indent;
            let current_len = line.content_len();
            let remaining = word.chars.len() - offset;
            let gap = usize::from(!line.is_empty());
            let available = capacity.saturating_sub(current_len);

            if remaining + gap <= available {
                let text: String = word.chars[offset..].iter().collect();
                line.append_piece(text);
                offset = word.chars.len();
                continue;
            }

            let opportunity = word
                .breaks
                .iter()
                .copied()
                .filter(|&break_at| break_at > offset && break_at < word.chars.len())
                .filter(|&break_at| break_at - offset + 1 <= available)
                .last();

            if let Some(end) = opportunity {
                let used = end - offset + 1;
                let unused = available.saturating_sub(used);
                let accept = if line.is_empty() {
                    true
                } else {
                    let baseline = capacity.saturating_sub(current_len);
                    unused * unused + options.hyphen_cost < baseline * baseline
                };

                if accept {
                    let mut text: String = word.chars[offset..end].iter().collect();
                    text.push('-');
                    line.append_piece(text);
                    lines.push(line);
                    line_index += 1;
                    line = new_line(options.rest_indent);
                    offset = end;
                    continue;
                }
            }

            if !line.is_empty() {
                lines.push(line);
                line_index += 1;
                line = new_line(options.rest_indent);
                continue;
            }

            let capacity = options.width - line.indent;
            if remaining > capacity {
                let text: String = word.chars[offset..offset + capacity].iter().collect();
                line.append_piece(text);
                lines.push(line);
                line_index += 1;
                line = new_line(options.rest_indent);
                offset += capacity;
            } else {
                let text: String = word.chars[offset..].iter().collect();
                line.append_piece(text);
                offset = word.chars.len();
            }
        }
    }

    if !line.is_empty() {
        lines.push(line);
    }

    let final_line = lines.len().saturating_sub(1);
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            render_line(
                line,
                options.width,
                options.justify && index < final_line && line.pieces.len() >= 2,
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

    let chars: Vec<(usize, char)> = input.char_indices().collect();

    for &(index, c) in &chars {
        if c.is_control() && !matches!(c, '\t' | '\r' | '\n') {
            return Err(WrapError::InvalidControl { index });
        }
    }

    for i in 0..chars.len() {
        if chars[i].1 != '~' {
            continue;
        }

        let side_is_word_char = |j: usize| {
            j < chars.len() && chars[j].1 != '~' && !is_separator(chars[j].1)
        };

        if i == 0 || !side_is_word_char(i - 1) || !side_is_word_char(i + 1) {
            return Err(WrapError::InvalidMarker { index: chars[i].0 });
        }
    }

    let mut normalized = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i].1;
        if c == '\r' {
            normalized.push('\n');
            if i + 1 < chars.len() && chars[i + 1].1 == '\n' {
                i += 1;
            }
        } else {
            normalized.push(c);
        }
        i += 1;
    }

    let mut output = Vec::new();
    let mut segment = Vec::new();
    i = 0;

    while i < normalized.len() {
        if normalized[i] == '\n' {
            let start = i;
            while i < normalized.len() && normalized[i] == '\n' {
                i += 1;
            }
            let count = i - start;
            if count >= 2 {
                output.extend(layout_segment(&segment, options));
                segment.clear();
                output.extend(std::iter::repeat_n(String::new(), count - 1));
            } else {
                segment.push('\n');
            }
        } else {
            segment.push(normalized[i]);
            i += 1;
        }
    }

    output.extend(layout_segment(&segment, options));
    Ok(output)
}