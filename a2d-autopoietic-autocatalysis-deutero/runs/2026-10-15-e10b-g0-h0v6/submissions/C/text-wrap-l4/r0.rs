#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WrapError {
    WidthZero,
    WidthTooLarge { width: usize },
    FirstIndentTooWide { indent: usize },
    RestIndentTooWide { indent: usize },
    HyphenCostTooLarge { cost: usize },
    InputTooLong { chars: usize },
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
    markers: Vec<usize>,
}

#[derive(Clone)]
struct Line {
    indent: usize,
    pieces: Vec<String>,
    display: String,
    consumed: usize,
    marker_ended: bool,
    raw: bool,
    piece_count: usize,
}

#[derive(Clone)]
struct Solution {
    cost: u64,
    marker_count: usize,
    lines: Vec<Line>,
}

fn better(a: &Solution, b: &Solution) -> bool {
    if a.cost != b.cost {
        return a.cost < b.cost;
    }
    if a.marker_count != b.marker_count {
        return a.marker_count < b.marker_count;
    }
    if a.lines.len() != b.lines.len() {
        return a.lines.len() < b.lines.len();
    }

    let av: Vec<usize> = a.lines.iter().map(|l| l.consumed).collect();
    let bv: Vec<usize> = b.lines.iter().map(|l| l.consumed).collect();
    if av != bv {
        return av > bv;
    }

    let astrings: Vec<&str> = a.lines.iter().map(|l| l.display.as_str()).collect();
    let bstrings: Vec<&str> = b.lines.iter().map(|l| l.display.as_str()).collect();
    astrings < bstrings
}

fn solve(
    words: &[Word],
    word_idx: usize,
    offset: usize,
    first: bool,
    options: &WrapOptions,
    memo: &mut Vec<Vec<Vec<Option<Solution>>>>,
) -> Option<Solution> {
    if word_idx == words.len() {
        return Some(Solution {
            cost: 0,
            marker_count: 0,
            lines: Vec::new(),
        });
    }
    let first_slot = usize::from(first);
    if let Some(result) = &memo[word_idx][offset][first_slot] {
        return Some(result.clone());
    }

    let indent = if first {
        options.first_indent
    } else {
        options.rest_indent
    };
    let cap = options.width - indent;
    let mut best: Option<Solution> = None;

    let mut consider = |end_word: usize,
                        end_offset: usize,
                        marker_ended: bool,
                        raw: bool,
                        line_pieces: Vec<String>,
                        consumed: usize| {
        let mut display = " ".repeat(indent);
        for (i, p) in line_pieces.iter().enumerate() {
            if i != 0 {
                display.push(' ');
            }
            display.push_str(p);
        }

        let next = if raw {
            solve(words, word_idx, end_offset, false, options, memo)
        } else if marker_ended {
            solve(words, end_word, end_offset, false, options, memo)
        } else {
            solve(words, end_word, 0, false, options, memo)
        };
        let Some(mut tail) = next else {
            return;
        };

        let gap = options.width - (indent + display.chars().count());
        let piece_count = line_pieces.len();
        let is_final = tail.lines.is_empty();
        let mut line_cost = (gap as u64) * (gap as u64);
        if is_final {
            line_cost /= 4;
        }
        if marker_ended {
            line_cost += options.hyphen_cost as u64;
            tail.marker_count += 1;
        }
        if raw {
            line_cost += (2 * options.width + 1) as u64;
        }
        if !is_final && piece_count == 1 {
            line_cost += 3;
        }
        tail.cost += line_cost;
        tail.lines.insert(
            0,
            Line {
                indent,
                pieces: line_pieces,
                display,
                consumed,
                marker_ended,
                raw,
                piece_count,
            },
        );
        if best.as_ref().map_or(true, |b| better(&tail, b)) {
            best = Some(tail);
        }
    };

    // Enumerate all lines ending after a completed word.
    let mut pieces = Vec::new();
    let mut used = 0usize;
    for wi in word_idx..words.len() {
        let start = if wi == word_idx { offset } else { 0 };
        let remaining = words[wi].chars.len() - start;
        let sep = usize::from(!pieces.is_empty());
        if used + sep + remaining > cap {
            break;
        }
        if sep != 0 {
            used += 1;
        }
        let text: String = words[wi].chars[start..].iter().collect();
        used += remaining;
        pieces.push(text);
        consider(wi + 1, 0, false, false, pieces.clone(), used);
    }

    // Marker-ended lines, possibly including completed prior words.
    let mut prefix_pieces = Vec::new();
    let mut prefix_used = 0usize;
    for wi in word_idx..words.len() {
        let start = if wi == word_idx { offset } else { 0 };
        let sep = usize::from(!prefix_pieces.is_empty());
        if prefix_used + sep >= cap {
            break;
        }
        if sep != 0 {
            prefix_used += 1;
        }
        let word = &words[wi];
        for &marker in &word.markers {
            if marker <= start {
                continue;
            }
            let plen = marker - start;
            if prefix_used + plen + 1 > cap {
                continue;
            }
            let mut line_pieces = prefix_pieces.clone();
            let mut text: String = word.chars[start..marker].iter().collect();
            text.push('-');
            line_pieces.push(text);
            consider(
                wi,
                marker,
                true,
                false,
                line_pieces,
                prefix_used + plen + 1,
            );
        }

        let remaining = word.chars.len() - start;
        if prefix_used + remaining > cap {
            break;
        }
        prefix_used += remaining;
        prefix_pieces.push(word.chars[start..].iter().collect());
    }

    // Raw split is allowed only when this suffix begins an empty line and no
    // remaining marker prefix plus hyphen can fit.
    if offset == 0 || word_idx < words.len() {
        let word = &words[word_idx];
        let remaining = word.chars.len() - offset;
        if remaining > cap {
            let any_marker_fits = word.markers.iter().any(|&m| {
                m > offset && m - offset + 1 <= cap
            });
            if !any_marker_fits {
                let end = offset + cap;
                let text: String = word.chars[offset..end].iter().collect();
                consider(word_idx, end, false, true, vec![text], cap);
            }
        }
    }

    memo[word_idx][offset][first_slot] = best.clone();
    best
}

fn layout_paragraph(words: &[Word], options: &WrapOptions) -> Vec<Line> {
    if words.is_empty() {
        return Vec::new();
    }
    let max_len = words.iter().map(|w| w.chars.len()).max().unwrap_or(0);
    let mut memo = vec![
        vec![vec![None; 2]; max_len + 1];
        words.len()
    ];
    solve(words, 0, 0, true, options, &mut memo)
        .map(|s| s.lines)
        .unwrap_or_default()
}

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{00A0}')
}

fn render_line(line: &Line, options: &WrapOptions, line_index: usize) -> String {
    let mut out = " ".repeat(line.indent);
    let gaps = line.pieces.len().saturating_sub(1);
    let justify = options.justify && line_index + 1 > 0
        && line.piece_count >= 2
        && !line.marker_ended
        && !line.raw;
    if gaps == 0 {
        out.push_str(&line.pieces[0]);
        return out;
    }

    let base_spaces = if justify {
        (options.width - line.display.chars().count()) / gaps
    } else {
        0
    };
    let extra = if justify {
        (options.width - line.display.chars().count()) % gaps
    } else {
        0
    };

    for i in 0..line.pieces.len() {
        if i != 0 {
            out.push(' ');
            if justify {
                for _ in 0..base_spaces {
                    out.push(' ');
                }
                let start = line_index % gaps;
                let distance = (i - 1 + gaps - start) % gaps;
                if distance < extra {
                    out.push(' ');
                }
            }
        }
        out.push_str(&line.pieces[i]);
    }
    out
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
    if chars.len() > 512 {
        return Err(WrapError::InputTooLong { chars: chars.len() });
    }

    for &(index, c) in &chars {
        if c.is_control() && !matches!(c, '\t' | '\r' | '\n') {
            return Err(WrapError::InvalidControl { index });
        }
    }

    for i in 0..chars.len() {
        if chars[i].1 == '~' {
            let left_ok = i > 0 && chars[i - 1].1 != '~' && !is_separator(chars[i - 1].1);
            let right_ok = i + 1 < chars.len()
                && chars[i + 1].1 != '~'
                && !is_separator(chars[i + 1].1);
            if !left_ok || !right_ok {
                return Err(WrapError::InvalidMarker {
                    index: chars[i].0,
                });
            }
        }
    }

    let mut normalized = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i].1;
        if c == '\r' && i + 1 < chars.len() && chars[i + 1].1 == '\n' {
            normalized.push('\n');
            i += 2;
        } else {
            normalized.push(if c == '\r' { '\n' } else { c });
            i += 1;
        }
    }

    let mut output = Vec::new();
    let mut segment = Vec::new();
    let mut j = 0usize;
    while j < normalized.len() {
        if normalized[j] == '\n' {
            let mut k = j;
            while k < normalized.len() && normalized[k] == '\n' {
                k += 1;
            }
            let run = k - j;
            if run >= 2 {
                let words = parse_words(&segment);
                let lines = layout_paragraph(&words, options);
                append_rendered(&mut output, lines, options);
                segment.clear();
                for _ in 1..run {
                    output.push(String::new());
                }
                j = k;
                continue;
            }
        }
        segment.push(normalized[j]);
        j += 1;
    }
    let words = parse_words(&segment);
    let lines = layout_paragraph(&words, options);
    append_rendered(&mut output, lines, options);

    Ok(output)
}

fn parse_words(segment: &[char]) -> Vec<Word> {
    let mut words = Vec::new();
    let mut chars = Vec::new();
    let mut markers = Vec::new();
    for &c in segment {
        if is_separator(c) {
            if !chars.is_empty() {
                words.push(Word {
                    chars: std::mem::take(&mut chars),
                    markers: std::mem::take(&mut markers),
                });
            }
        } else if c == '~' {
            markers.push(chars.len());
        } else {
            chars.push(c);
        }
    }
    if !chars.is_empty() {
        words.push(Word { chars, markers });
    }
    words
}

fn append_rendered(output: &mut Vec<String>, lines: Vec<Line>, options: &WrapOptions) {
    let last = lines.len().saturating_sub(1);
    for (i, line) in lines.iter().enumerate() {
        let justified = options.justify
            && i != last
            && line.piece_count >= 2
            && !line.marker_ended
            && !line.raw;
        if justified {
            output.push(render_line(line, options, i));
        } else {
            let mut text = " ".repeat(line.indent);
            for (piece_i, piece) in line.pieces.iter().enumerate() {
                if piece_i != 0 {
                    text.push(' ');
                }
                text.push_str(piece);
            }
            output.push(text);
        }
    }
}

fn main() {}