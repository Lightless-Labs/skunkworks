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
struct Piece {
    text: String,
}

struct Line {
    indent: usize,
    pieces: Vec<Piece>,
    content: String,
}

impl Line {
    fn new(indent: usize) -> Self {
        Self {
            indent,
            pieces: Vec::new(),
            content: String::new(),
        }
    }

    fn content_len(&self) -> usize {
        self.content.chars().count()
    }

    fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    fn append_piece(&mut self, text: String) {
        if !self.pieces.is_empty() {
            self.content.push(' ');
        }
        self.content.push_str(&text);
        self.pieces.push(Piece { text });
    }

    fn render(&self, width: usize, justify: bool, final_line: bool, line_index: usize) -> String {
        let mut out = " ".repeat(self.indent);
        if justify && !final_line && self.pieces.len() >= 2 {
            let gaps = self.pieces.len() - 1;
            let used = self.indent + self.content_len();
            let extra = width.saturating_sub(used);
            let base = extra / gaps;
            let rem = extra % gaps;
            let start = line_index % gaps;

            for (i, piece) in self.pieces.iter().enumerate() {
                if i > 0 {
                    let gap_index = i - 1;
                    let bonus = if (gap_index + gaps - start) % gaps < rem {
                        1
                    } else {
                        0
                    };
                    out.push_str(&" ".repeat(1 + base + bonus));
                }
                out.push_str(&piece.text);
            }
        } else {
            out.push_str(&self.content);
        }
        out
    }
}

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{00A0}')
}

fn validate_markers(input: &str) -> Result<(), WrapError> {
    let chars: Vec<(usize, char)> = input.char_indices().collect();
    for (i, &(byte_index, c)) in chars.iter().enumerate() {
        if c == '~' {
            let valid_neighbor = |j: isize| -> bool {
                j >= 0
                    && (j as usize) < chars.len()
                    && chars[j as usize].1 != '~'
                    && !is_separator(chars[j as usize].1)
            };
            if !valid_neighbor(i as isize - 1) || !valid_neighbor(i as isize + 1) {
                return Err(WrapError::InvalidMarker { index: byte_index });
            }
        }
    }
    Ok(())
}

fn parse_words(segment: &str) -> Vec<Vec<char>> {
    let mut words = Vec::new();
    let mut current = Vec::new();

    for c in segment.chars() {
        if is_separator(c) {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
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

    for (index, c) in input.char_indices() {
        if c.is_control() && !matches!(c, '\t' | '\r' | '\n') {
            return Err(WrapError::InvalidControl { index });
        }
    }
    validate_markers(input)?;

    let mut normalized = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            normalized.push('\n');
        } else {
            normalized.push(c);
        }
    }

    let all: Vec<char> = normalized.chars().collect();
    let mut segments: Vec<Vec<char>> = Vec::new();
    let mut boundaries: Vec<usize> = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < all.len() {
        if all[i] == '\n' {
            let mut j = i;
            while j < all.len() && all[j] == '\n' {
                j += 1;
            }
            if j - i >= 2 {
                segments.push(all[start..i].to_vec());
                boundaries.push(j - i - 1);
                start = j;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    segments.push(all[start..].to_vec());

    let mut output = Vec::new();

    for (segment_index, segment) in segments.iter().enumerate() {
        let segment_string: String = segment.iter().collect();
        let words = parse_words(&segment_string);

        if !words.is_empty() {
            let mut lines: Vec<Line> = Vec::new();
            let mut line = Line::new(options.first_indent);
            let mut is_first_line = true;

            for word in words {
                let mut offset = 0;
                while offset < word.len() {
                    let capacity = options.width - line.indent;
                    let current_len = line.content_len();
                    let room = capacity.saturating_sub(current_len + usize::from(!line.is_empty()));
                    let suffix_len = word.len() - offset;

                    if suffix_len <= room {
                        let text: String = word[offset..]
                            .iter()
                            .filter(|&&c| c != '~')
                            .collect();
                        if !text.is_empty() {
                            line.append_piece(text);
                        }
                        offset = word.len();
                        continue;
                    }

                    let mut chosen: Option<(usize, usize)> = None;
                    let mut prefix_len = 0;
                    for k in offset..word.len() {
                        if word[k] == '~' && k > offset && k < word.len() - 1 {
                            let visible_prefix = prefix_len;
                            if visible_prefix + 1 <= room {
                                chosen = Some((k, visible_prefix));
                            }
                        }
                        if word[k] != '~' {
                            prefix_len += 1;
                        }
                    }

                    if let Some((marker_index, visible_prefix)) = chosen {
                        let used = visible_prefix + 1;
                        let unused = room.saturating_sub(used);
                        let accept = if line.is_empty() {
                            true
                        } else {
                            let baseline = capacity.saturating_sub(current_len);
                            unused * unused + options.hyphen_cost < baseline * baseline
                        };

                        if accept {
                            let mut text: String = word[offset..marker_index]
                                .iter()
                                .filter(|&&c| c != '~')
                                .collect();
                            text.push('-');
                            line.append_piece(text);
                            lines.push(line);
                            is_first_line = false;
                            line = Line::new(options.rest_indent);
                            offset = marker_index + 1;
                            continue;
                        }
                    }

                    if !line.is_empty() {
                        lines.push(line);
                        is_first_line = false;
                        line = Line::new(options.rest_indent);
                        continue;
                    }

                    let capacity = options.width - line.indent;
                    let suffix_visible: usize = word[offset..]
                        .iter()
                        .filter(|&&c| c != '~')
                        .count();

                    if suffix_visible <= capacity {
                        let text: String = word[offset..]
                            .iter()
                            .filter(|&&c| c != '~')
                            .collect();
                        if !text.is_empty() {
                            line.append_piece(text);
                        }
                        offset = word.len();
                    } else {
                        let mut consumed = 0;
                        let mut raw = String::new();
                        let mut next_offset = offset;
                        while next_offset < word.len() && consumed < capacity {
                            if word[next_offset] != '~' {
                                raw.push(word[next_offset]);
                                consumed += 1;
                            }
                            next_offset += 1;
                        }
                        line.append_piece(raw);
                        lines.push(line);
                        is_first_line = false;
                        line = Line::new(options.rest_indent);
                        offset = next_offset;
                    }
                }
            }

            if !line.is_empty() {
                lines.push(line);
            }

            let final_index = lines.len().saturating_sub(1);
            for (line_index, line) in lines.iter().enumerate() {
                output.push(line.render(
                    options.width,
                    options.justify,
                    line_index == final_index,
                    line_index,
                ));
            }
            let _ = is_first_line;
        }

        if segment_index < boundaries.len() {
            for _ in 0..boundaries[segment_index] {
                output.push(String::new());
            }
        }
    }

    Ok(output)
}