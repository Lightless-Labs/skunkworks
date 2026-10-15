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

fn is_separator(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{00a0}')
}

fn normalize_newlines(input: &str) -> Vec<char> {
    let mut output = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            output.push('\n');
        } else {
            output.push(ch);
        }
    }

    output
}

fn parse_segment(segment: &[char]) -> Vec<(Vec<char>, Vec<usize>)> {
    let mut words = Vec::new();
    let mut word = Vec::new();
    let mut opportunities = Vec::new();

    for &ch in segment {
        if is_separator(ch) {
            if !word.is_empty() {
                words.push((std::mem::take(&mut word), std::mem::take(&mut opportunities)));
            }
        } else if ch == '~' {
            opportunities.push(word.len());
        } else {
            word.push(ch);
        }
    }

    if !word.is_empty() {
        words.push((word, opportunities));
    }

    words
}

#[derive(Default)]
struct Line {
    content: String,
    content_len: usize,
    pieces: usize,
    gaps: usize,
}

impl Line {
    fn is_empty(&self) -> bool {
        self.content_len == 0
    }

    fn add_piece(&mut self, piece: &str, len: usize, needs_gap: bool) {
        if needs_gap {
            self.content.push(' ');
            self.content_len += 1;
            self.gaps += 1;
        }
        self.content.push_str(piece);
        self.content_len += len;
        self.pieces += 1;
    }
}

fn layout_paragraph(words: &[(Vec<char>, Vec<usize>)], options: &WrapOptions) -> Vec<String> {
    if words.is_empty() {
        return Vec::new();
    }

    let mut output = Vec::new();
    let mut line = Line::default();
    let mut line_index = 0usize;

    for (word, opportunities) in words {
        let mut offset = 0usize;
        let mut continued = false;

        while offset < word.len() {
            let indent = if line_index == 0 {
                options.first_indent
            } else {
                options.rest_indent
            };
            let capacity = options.width - indent;
            let gap = if !line.is_empty() && !continued { 1 } else { 0 };
            let room = capacity.saturating_sub(line.content_len + gap);
            let suffix_len = word.len() - offset;

            if suffix_len <= room {
                let piece: String = word[offset..].iter().collect();
                line.add_piece(&piece, suffix_len, gap != 0);
                offset = word.len();
                break;
            }

            let mut usable = None;
            for &marker in opportunities {
                if marker <= offset || marker >= word.len() {
                    continue;
                }
                let prefix_len = marker - offset;
                if prefix_len + 1 <= room {
                    usable = Some((marker, prefix_len));
                }
            }

            if let Some((marker, prefix_len)) = usable {
                let unused = room - prefix_len - 1;
                let accept = if line.is_empty() {
                    true
                } else {
                    let baseline = capacity - line.content_len;
                    unused.saturating_mul(unused).saturating_add(options.hyphen_cost)
                        < baseline.saturating_mul(baseline)
                };

                if accept {
                    let mut piece: String = word[offset..marker].iter().collect();
                    piece.push('-');
                    line.add_piece(&piece, prefix_len + 1, gap != 0);
                    output.push(format!("{}{}", " ".repeat(indent), line.content));
                    line = Line::default();
                    line_index += 1;
                    offset = marker;
                    continued = true;
                    continue;
                }
            }

            if !line.is_empty() {
                output.push(format!("{}{}", " ".repeat(indent), line.content));
                line = Line::default();
                line_index += 1;
                continued = false;
                continue;
            }

            if suffix_len > capacity {
                let end = offset + capacity;
                let piece: String = word[offset..end].iter().collect();
                line.add_piece(&piece, capacity, false);
                output.push(format!("{}{}", " ".repeat(indent), line.content));
                line = Line::default();
                line_index += 1;
                offset = end;
                continued = true;
            } else {
                let piece: String = word[offset..].iter().collect();
                line.add_piece(&piece, suffix_len, false);
                offset = word.len();
            }
        }
    }

    if !line.is_empty() {
        output.push(format!("{}{}", " ".repeat(if line_index == 0 {
            options.first_indent
        } else {
            options.rest_indent
        }), line.content));
    }

    if options.justify && output.len() >= 2 {
        for paragraph_line_index in 0..output.len() - 1 {
            let indent = if paragraph_line_index == 0 {
                options.first_indent
            } else {
                options.rest_indent
            };
            let text = output[paragraph_line_index].clone();
            let content: String = text.chars().skip(indent).collect();

            let gaps = content.chars().filter(|&ch| ch == ' ').count();
            if gaps == 0 {
                continue;
            }

            let pre_width = indent + content.chars().count();
            let extra = options.width.saturating_sub(pre_width);
            let each = extra / gaps;
            let remainder = extra % gaps;
            let start_gap = paragraph_line_index % gaps;

            let mut rebuilt = String::new();
            rebuilt.push_str(&" ".repeat(indent));
            let mut gap_index = 0usize;
            for ch in content.chars() {
                if ch == ' ' {
                    rebuilt.push(' ');
                    let offset = (gap_index + gaps - start_gap) % gaps;
                    let add = each + usize::from(offset < remainder);
                    rebuilt.push_str(&" ".repeat(add));
                    gap_index += 1;
                } else {
                    rebuilt.push(ch);
                }
            }
            output[paragraph_line_index] = rebuilt;
        }
    }

    output
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

    let indexed: Vec<(usize, char)> = input.char_indices().collect();

    for &(index, ch) in &indexed {
        if ch.is_control() && !matches!(ch, '\t' | '\r' | '\n') {
            return Err(WrapError::InvalidControl { index });
        }
    }

    for (position, &(index, ch)) in indexed.iter().enumerate() {
        if ch != '~' {
            continue;
        }

        let left_valid = position > 0
            && indexed[position - 1].1 != '~'
            && !is_separator(indexed[position - 1].1);
        let right_valid = position + 1 < indexed.len()
            && indexed[position + 1].1 != '~'
            && !is_separator(indexed[position + 1].1);

        if !left_valid || !right_valid {
            return Err(WrapError::InvalidMarker { index });
        }
    }

    let normalized = normalize_newlines(input);
    let mut output = Vec::new();
    let mut start = 0usize;
    let mut index = 0usize;

    while index < normalized.len() {
        if normalized[index] != '\n' {
            index += 1;
            continue;
        }

        let run_start = index;
        while index < normalized.len() && normalized[index] == '\n' {
            index += 1;
        }

        let run_len = index - run_start;
        if run_len >= 2 {
            let words = parse_segment(&normalized[start..run_start]);
            output.extend(layout_paragraph(&words, options));
            output.extend(std::iter::repeat(String::new()).take(run_len - 1));
            start = index;
        }
    }

    let words = parse_segment(&normalized[start..]);
    output.extend(layout_paragraph(&words, options));

    Ok(output)
}