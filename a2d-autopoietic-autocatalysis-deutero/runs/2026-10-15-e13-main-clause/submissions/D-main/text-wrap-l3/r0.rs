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
    text: Vec<char>,
}

struct Line {
    indent: usize,
    pieces: Vec<Piece>,
    content_len: usize,
}

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{00a0}')
}

fn parse_segment(segment: &[char]) -> Vec<Vec<char>> {
    let mut words = Vec::new();
    let mut word = Vec::new();

    for &c in segment {
        if is_separator(c) {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
        } else {
            word.push(c);
        }
    }

    if !word.is_empty() {
        words.push(word);
    }
    words
}

fn visible_len(word: &[char], start: usize, end: usize) -> usize {
    word[start..end].iter().filter(|&&c| c != '~').count()
}

fn visible_suffix(word: &[char], offset: usize) -> Vec<char> {
    word[offset..]
        .iter()
        .copied()
        .filter(|&c| c != '~')
        .collect()
}

fn append_piece(line: &mut Line, chars: Vec<char>) {
    if !line.pieces.is_empty() {
        line.content_len += 1;
    }
    line.content_len += chars.len();
    line.pieces.push(Piece { text: chars });
}

fn line_to_string(
    line: &Line,
    justify: bool,
    is_final: bool,
    width: usize,
    line_index: usize,
) -> String {
    let gaps = line.pieces.len().saturating_sub(1);
    let extra = if justify && !is_final && gaps > 0 {
        width.saturating_sub(line.indent + line.content_len)
    } else {
        0
    };

    let each = if gaps > 0 { extra / gaps } else { 0 };
    let remainder = if gaps > 0 { extra % gaps } else { 0 };
    let start = if gaps > 0 { line_index % gaps } else { 0 };

    let mut output = " ".repeat(line.indent);
    for (i, piece) in line.pieces.iter().enumerate() {
        output.extend(piece.text.iter());
        if i < gaps {
            let add = each
                + usize::from(
                    remainder > 0 && (i + gaps - start) % gaps < remainder,
                );
            output.push_str(&" ".repeat(1 + add));
        }
    }
    output
}

fn wrap_paragraph(words: &[Vec<char>], options: &WrapOptions, output: &mut Vec<String>) {
    if words.is_empty() {
        return;
    }

    let mut lines = Vec::new();
    let mut line = Line {
        indent: options.first_indent,
        pieces: Vec::new(),
        content_len: 0,
    };

    for word in words {
        let mut offset = 0;

        while offset < word.len() {
            let capacity = options.width - line.indent;
            let separator = usize::from(!line.pieces.is_empty());
            let room = capacity.saturating_sub(line.content_len + separator);
            let suffix = visible_suffix(word, offset);

            if suffix.len() <= room {
                append_piece(&mut line, suffix);
                offset = word.len();
                continue;
            }

            let mut selected = None;
            for marker in offset.saturating_add(1)..word.len() {
                if word[marker] == '~' {
                    let prefix_len = visible_len(word, offset, marker);
                    if prefix_len + 1 <= room {
                        selected = Some((marker, prefix_len));
                    }
                }
            }

            if let Some((marker, prefix_len)) = selected {
                let unused = room - prefix_len - 1;
                let accepted = line.pieces.is_empty()
                    || unused * unused + options.hyphen_cost
                        < (capacity - line.content_len) * (capacity - line.content_len);

                if accepted {
                    let mut fragment: Vec<char> = word[offset..marker]
                        .iter()
                        .copied()
                        .filter(|&c| c != '~')
                        .collect();
                    fragment.push('-');
                    append_piece(&mut line, fragment);
                    lines.push(line);
                    line = Line {
                        indent: options.rest_indent,
                        pieces: Vec::new(),
                        content_len: 0,
                    };
                    offset = marker + 1;
                    continue;
                }
            }

            if !line.pieces.is_empty() {
                lines.push(line);
                line = Line {
                    indent: options.rest_indent,
                    pieces: Vec::new(),
                    content_len: 0,
                };
                continue;
            }

            let capacity = options.width - line.indent;
            if suffix.len() > capacity {
                let mut fragment = Vec::new();
                let mut consumed = 0;
                let mut i = offset;

                while i < word.len() && consumed < capacity {
                    if word[i] != '~' {
                        fragment.push(word[i]);
                        consumed += 1;
                    }
                    i += 1;
                }

                append_piece(&mut line, fragment);
                lines.push(line);
                line = Line {
                    indent: options.rest_indent,
                    pieces: Vec::new(),
                    content_len: 0,
                };
                offset = i;
            } else {
                append_piece(&mut line, suffix);
                offset = word.len();
            }
        }
    }

    if !line.pieces.is_empty() {
        lines.push(line);
    }

    let last = lines.len().saturating_sub(1);
    for (i, line) in lines.iter().enumerate() {
        output.push(line_to_string(
            line,
            options.justify,
            i == last,
            options.width,
            i,
        ));
    }
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

    let chars: Vec<(usize, char)> = input.char_indices().collect();
    for i in 0..chars.len() {
        if chars[i].1 == '~' {
            let valid_side = |j: usize| {
                j < chars.len() && chars[j].1 != '~' && !is_separator(chars[j].1)
            };
            if i == 0 || i + 1 == chars.len() || !valid_side(i - 1) || !valid_side(i + 1) {
                return Err(WrapError::InvalidMarker { index: chars[i].0 });
            }
        }
    }

    let mut normalized = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i].1;
        if c == '\r' {
            normalized.push('\n');
            if i + 1 < chars.len() && chars[i + 1].1 == '\n' {
                i += 2;
            } else {
                i += 1;
            }
        } else {
            normalized.push(c);
            i += 1;
        }
    }

    let mut output = Vec::new();
    let mut segment = Vec::new();
    let mut pos = 0;

    while pos < normalized.len() {
        if normalized[pos] == '\n' {
            let mut end = pos;
            while end < normalized.len() && normalized[end] == '\n' {
                end += 1;
            }

            if end - pos >= 2 {
                wrap_paragraph(&parse_segment(&segment), options, &mut output);
                segment.clear();
                for _ in 1..(end - pos) {
                    output.push(String::new());
                }
                pos = end;
                continue;
            }
        }

        segment.push(normalized[pos]);
        pos += 1;
    }

    wrap_paragraph(&parse_segment(&segment), options, &mut output);
    Ok(output)
}

fn main() {}