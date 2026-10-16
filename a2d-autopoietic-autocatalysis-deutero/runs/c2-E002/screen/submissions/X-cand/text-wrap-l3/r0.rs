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
struct Scalar {
    ch: char,
    byte_index: usize,
}

#[derive(Clone)]
struct Word {
    chars: Vec<char>,
    markers: Vec<usize>,
}

#[derive(Clone)]
struct Piece {
    text: String,
}

struct Line {
    indent: usize,
    pieces: Vec<Piece>,
    content_len: usize,
}

fn is_separator(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{00A0}')
}

fn parse_words(segment: &[char]) -> Vec<Word> {
    let mut words = Vec::new();
    let mut chars = Vec::new();
    let mut markers = Vec::new();

    for &ch in segment {
        if is_separator(ch) {
            if !chars.is_empty() {
                words.push(Word { chars, markers });
                chars = Vec::new();
                markers = Vec::new();
            }
        } else if ch == '~' {
            markers.push(chars.len());
        } else {
            chars.push(ch);
        }
    }

    if !chars.is_empty() {
        words.push(Word { chars, markers });
    }

    words
}

fn append_piece(line: &mut Line, text: String) {
    if !line.pieces.is_empty() {
        line.content_len += 1;
    }
    line.content_len += text.chars().count();
    line.pieces.push(Piece { text });
}

fn emit_line(line: Line, width: usize, justify: bool, line_index: usize, final_line: bool) -> String {
    let mut out = " ".repeat(line.indent);

    if justify && !final_line && line.pieces.len() >= 2 {
        let gaps = line.pieces.len() - 1;
        let extra = width.saturating_sub(line.indent + line.content_len);
        let each = extra / gaps;
        let remainder = extra % gaps;
        let start_gap = line_index % gaps;

        for (i, piece) in line.pieces.iter().enumerate() {
            if i > 0 {
                let gap = i - 1;
                let additional = each + usize::from((gap + gaps - start_gap) % gaps < remainder);
                out.push_str(&" ".repeat(1 + additional));
            }
            out.push_str(&piece.text);
        }
    } else {
        for (i, piece) in line.pieces.iter().enumerate() {
            if i > 0 {
                out.push(' ');
            }
            out.push_str(&piece.text);
        }
    }

    out
}

fn layout_paragraph(words: &[Word], options: &WrapOptions) -> Vec<String> {
    let mut result = Vec::new();
    let mut line_index = 0usize;
    let mut line = Line {
        indent: options.first_indent,
        pieces: Vec::new(),
        content_len: 0,
    };

    for word in words {
        let mut offset = 0usize;

        while offset < word.chars.len() {
            let capacity = options.width - line.indent;
            let separator = usize::from(!line.pieces.is_empty());
            let room = capacity.saturating_sub(line.content_len + separator);
            let suffix_len = word.chars.len() - offset;

            if suffix_len <= room {
                let text: String = word.chars[offset..].iter().collect();
                append_piece(&mut line, text);
                offset = word.chars.len();
                continue;
            }

            let mut usable: Option<(usize, usize)> = None;
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
                let unused = room - prefix_len - 1;
                let accepted = if line.pieces.is_empty() {
                    true
                } else {
                    let threshold = room.saturating_mul(room);
                    let costed = unused.saturating_mul(unused).saturating_add(options.hyphen_cost);
                    costed < threshold
                };

                if accepted {
                    let mut text: String = word.chars[offset..marker].iter().collect();
                    text.push('-');
                    append_piece(&mut line, text);
                    offset = marker;
                    let emitted = emit_line(line, options.width, options.justify, line_index, false);
                    result.push(emitted);
                    line_index += 1;
                    line = Line {
                        indent: options.rest_indent,
                        pieces: Vec::new(),
                        content_len: 0,
                    };
                    continue;
                }
            }

            if !line.pieces.is_empty() {
                let emitted = emit_line(line, options.width, options.justify, line_index, false);
                result.push(emitted);
                line_index += 1;
                line = Line {
                    indent: options.rest_indent,
                    pieces: Vec::new(),
                    content_len: 0,
                };
                continue;
            }

            let capacity = options.width - line.indent;
            if suffix_len > capacity {
                let end = offset + capacity;
                let text: String = word.chars[offset..end].iter().collect();
                append_piece(&mut line, text);
                offset = end;
                let emitted = emit_line(line, options.width, options.justify, line_index, false);
                result.push(emitted);
                line_index += 1;
                line = Line {
                    indent: options.rest_indent,
                    pieces: Vec::new(),
                    content_len: 0,
                };
            } else {
                let text: String = word.chars[offset..].iter().collect();
                append_piece(&mut line, text);
                offset = word.chars.len();
            }
        }
    }

    if !line.pieces.is_empty() {
        result.push(emit_line(line, options.width, options.justify, line_index, true));
    }

    result
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

    for i in 0..scalars.len() {
        if scalars[i].ch != '~' {
            continue;
        }
        let valid_side = |index: usize| {
            index < scalars.len()
                && scalars[index].ch != '~'
                && !is_separator(scalars[index].ch)
        };
        if i == 0 || i + 1 >= scalars.len() || !valid_side(i - 1) || !valid_side(i + 1) {
            return Err(WrapError::InvalidMarker {
                index: scalars[i].byte_index,
            });
        }
    }

    let mut normalized = Vec::new();
    let mut i = 0usize;
    while i < scalars.len() {
        let ch = scalars[i].ch;
        if ch == '\r' {
            if i + 1 < scalars.len() && scalars[i + 1].ch == '\n' {
                i += 1;
            }
            normalized.push('\n');
        } else {
            normalized.push(ch);
        }
        i += 1;
    }

    let mut output = Vec::new();
    let mut start = 0usize;
    let mut pos = 0usize;

    while pos < normalized.len() {
        if normalized[pos] == '\n' {
            let run_start = pos;
            while pos < normalized.len() && normalized[pos] == '\n' {
                pos += 1;
            }
            let run_len = pos - run_start;
            if run_len >= 2 {
                let words = parse_words(&normalized[start..run_start]);
                output.extend(layout_paragraph(&words, options));
                output.extend(std::iter::repeat_n(String::new(), run_len - 1));
                start = pos;
            }
        } else {
            pos += 1;
        }
    }

    let words = parse_words(&normalized[start..]);
    output.extend(layout_paragraph(&words, options));

    Ok(output)
}