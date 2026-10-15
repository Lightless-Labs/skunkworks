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
struct Candidate {
    cost: u64,
    marker_lines: usize,
    lines: Vec<Line>,
    consumed: Vec<usize>,
    rendered: Vec<String>,
}

#[derive(Clone)]
struct Line {
    indent: usize,
    pieces: Vec<String>,
    consumed: usize,
    marker_ended: bool,
    raw: bool,
}

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{00a0}')
}

fn better(a: &Candidate, b: &Candidate) -> bool {
    if a.cost != b.cost {
        return a.cost < b.cost;
    }
    if a.marker_lines != b.marker_lines {
        return a.marker_lines < b.marker_lines;
    }
    if a.lines.len() != b.lines.len() {
        return a.lines.len() < b.lines.len();
    }
    if a.consumed != b.consumed {
        return a.consumed > b.consumed;
    }
    a.rendered < b.rendered
}

fn line_string(line: &Line) -> String {
    let mut s = " ".repeat(line.indent);
    for (i, piece) in line.pieces.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(piece);
    }
    s
}

fn solve(
    words: &[Word],
    wi: usize,
    off: usize,
    first: bool,
    width: usize,
    first_indent: usize,
    rest_indent: usize,
    hyphen_cost: usize,
) -> Option<Candidate> {
    if wi >= words.len() {
        return Some(Candidate {
            cost: 0,
            marker_lines: 0,
            lines: Vec::new(),
            consumed: Vec::new(),
            rendered: Vec::new(),
        });
    }

    let indent = if first { first_indent } else { rest_indent };
    let capacity = width - indent;
    let mut best: Option<Candidate> = None;

    // Enumerate a legal line by consuming whole words, then optionally ending
    // within the current word at one of its markers.
    let mut pieces: Vec<String> = Vec::new();
    let mut pos_w = wi;
    let mut pos_off = off;
    let mut used = 0usize;
    let mut consumed_total = 0usize;

    loop {
        let word = &words[pos_w];
        let remain = word.chars.len() - pos_off;
        let gap = usize::from(!pieces.is_empty());
        if used + gap > capacity {
            break;
        }
        let available = capacity - used - gap;

        // Marker-ended options in this word.
        for &m in &word.markers {
            if m <= pos_off || m >= word.chars.len() {
                continue;
            }
            let prefix_len = m - pos_off;
            if prefix_len + 1 > available {
                continue;
            }
            let mut display: String = word.chars[pos_off..m].iter().collect();
            display.push('-');
            let mut next_pieces = pieces.clone();
            next_pieces.push(display);
            let line_used = used + gap + prefix_len + 1;
            let line = Line {
                indent,
                pieces: next_pieces,
                consumed: consumed_total + prefix_len,
                marker_ended: true,
                raw: false,
            };
            let rest = solve(
                words, pos_w, m, false, width, first_indent, rest_indent, hyphen_cost,
            );
            if let Some(mut tail) = rest {
                let line_cost = ((width - indent - line_used) as u64).pow(2)
                    + hyphen_cost as u64
                    + if line.pieces.len() == 1 { 3 } else { 0 };
                let mut cand = Candidate {
                    cost: line_cost + tail.cost,
                    marker_lines: 1 + tail.marker_lines,
                    lines: vec![line.clone()],
                    consumed: vec![line.consumed],
                    rendered: vec![line_string(&line)],
                };
                cand.lines.append(&mut tail.lines);
                cand.consumed.append(&mut tail.consumed);
                cand.rendered.append(&mut tail.rendered);
                if best.as_ref().is_none_or(|b| better(&cand, b)) {
                    best = Some(cand);
                }
            }
        }

        // Raw splitting is available only as the first and sole piece.
        if pieces.is_empty() && remain > capacity {
            let marker_fits = word.markers.iter().any(|&m| {
                m > pos_off && m < word.chars.len() && m - pos_off + 1 <= capacity
            });
            if !marker_fits {
                let display: String = word.chars[pos_off..pos_off + capacity].iter().collect();
                let line = Line {
                    indent,
                    pieces: vec![display],
                    consumed: capacity,
                    marker_ended: false,
                    raw: true,
                };
                if let Some(mut tail) = solve(
                    words,
                    pos_w,
                    pos_off + capacity,
                    false,
                    width,
                    first_indent,
                    rest_indent,
                    hyphen_cost,
                ) {
                    let line_cost = ((width - indent - capacity) as u64).pow(2)
                        + (2 * width + 1) as u64;
                    let mut cand = Candidate {
                        cost: line_cost + tail.cost,
                        marker_lines: tail.marker_lines,
                        lines: vec![line.clone()],
                        consumed: vec![capacity],
                        rendered: vec![line_string(&line)],
                    };
                    cand.lines.append(&mut tail.lines);
                    cand.consumed.append(&mut tail.consumed);
                    cand.rendered.append(&mut tail.rendered);
                    if best.as_ref().is_none_or(|b| better(&cand, b)) {
                        best = Some(cand);
                    }
                }
            }
        }

        if remain > available {
            break;
        }

        // Complete this word on the line.
        let text: String = word.chars[pos_off..].iter().collect();
        pieces.push(text);
        used += gap + remain;
        consumed_total += remain;

        let next_w = pos_w + 1;
        if next_w == words.len() {
            let line = Line {
                indent,
                pieces: pieces.clone(),
                consumed: consumed_total,
                marker_ended: false,
                raw: false,
            };
            let line_used = used;
            let gap_width = width - indent - line_used;
            let line_cost = ((gap_width as u64) * (gap_width as u64)) / 4;
            let cand = Candidate {
                cost: line_cost,
                marker_lines: 0,
                lines: vec![line.clone()],
                consumed: vec![consumed_total],
                rendered: vec![line_string(&line)],
            };
            if best.as_ref().is_none_or(|b| better(&cand, b)) {
                best = Some(cand);
            }
            break;
        }

        if let Some(mut tail) = solve(
            words,
            next_w,
            0,
            false,
            width,
            first_indent,
            rest_indent,
            hyphen_cost,
        ) {
            let line = Line {
                indent,
                pieces: pieces.clone(),
                consumed: consumed_total,
                marker_ended: false,
                raw: false,
            };
            let gap_width = width - indent - used;
            let line_cost = (gap_width as u64) * (gap_width as u64)
                + if pieces.len() == 1 { 3 } else { 0 };
            let mut cand = Candidate {
                cost: line_cost + tail.cost,
                marker_lines: tail.marker_lines,
                lines: vec![line.clone()],
                consumed: vec![consumed_total],
                rendered: vec![line_string(&line)],
            };
            cand.lines.append(&mut tail.lines);
            cand.consumed.append(&mut tail.consumed);
            cand.rendered.append(&mut tail.rendered);
            if best.as_ref().is_none_or(|b| better(&cand, b)) {
                best = Some(cand);
            }
        }

        pos_w = next_w;
        pos_off = 0;
        if pos_w >= words.len() {
            break;
        }
    }

    best
}

fn parse_words(chars: &[char]) -> Vec<Word> {
    let mut words = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        while i < chars.len() && is_separator(chars[i]) {
            i += 1;
        }
        if i == chars.len() {
            break;
        }
        let mut spelling = Vec::new();
        let mut markers = Vec::new();
        while i < chars.len() && !is_separator(chars[i]) {
            if chars[i] == '~' {
                markers.push(spelling.len());
            } else {
                spelling.push(chars[i]);
            }
            i += 1;
        }
        words.push(Word {
            chars: spelling,
            markers,
        });
    }
    words
}

fn render(candidate: Candidate, width: usize, justify: bool) -> Vec<String> {
    let count = candidate.lines.len();
    let mut out = Vec::with_capacity(count);
    for (idx, line) in candidate.lines.iter().enumerate() {
        let mut s = " ".repeat(line.indent);
        let gaps = line.pieces.len().saturating_sub(1);
        let base = if gaps > 0 && justify && idx + 1 < count && gaps > 0 {
            let used: usize = line.pieces.iter().map(|p| p.chars().count()).sum::<usize>()
                + line.indent
                + gaps;
            let extra = width - used;
            Some((extra / gaps, extra % gaps))
        } else {
            None
        };
        for (i, piece) in line.pieces.iter().enumerate() {
            if i > 0 {
                s.push(' ');
                if let Some((each, rem)) = base {
                    s.push_str(&" ".repeat(each));
                    let start = idx % gaps;
                    let distance = (i - 1 + gaps - start) % gaps;
                    if distance < rem {
                        s.push(' ');
                    }
                }
            }
            s.push_str(piece);
        }
        out.push(s);
    }
    out
}

pub fn wrap(input: &str, options: &WrapOptions) -> Result<Vec<String>, WrapError> {
    if options.width == 0 {
        return Err(WrapError::WidthZero);
    }
    if options.width > 256 {
        return Err(WrapError::WidthTooLarge { width: options.width });
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
    if original.len() > 512 {
        return Err(WrapError::InputTooLong {
            chars: original.len(),
        });
    }
    for &(index, ch) in &original {
        if ch.is_control() && !matches!(ch, '\t' | '\r' | '\n') {
            return Err(WrapError::InvalidControl { index });
        }
    }
    for i in 0..original.len() {
        if original[i].1 != '~' {
            continue;
        }
        let invalid_side = |j: isize| {
            if j < 0 || j as usize >= original.len() {
                return true;
            }
            let c = original[j as usize].1;
            c == '~' || is_separator(c)
        };
        if invalid_side(i as isize - 1) || invalid_side(i as isize + 1) {
            return Err(WrapError::InvalidMarker {
                index: original[i].0,
            });
        }
    }

    let mut normalized = Vec::new();
    let mut i = 0;
    while i < original.len() {
        let c = original[i].1;
        if c == '\r' {
            normalized.push('\n');
            if i + 1 < original.len() && original[i + 1].1 == '\n' {
                i += 2;
            } else {
                i += 1;
            }
        } else {
            normalized.push(c);
            i += 1;
        }
    }

    if normalized.is_empty() {
        return Ok(Vec::new());
    }

    let mut result = Vec::new();
    let mut start = 0;
    let mut p = 0;
    while p < normalized.len() {
        if normalized[p] == '\n' {
            let mut end = p;
            while end < normalized.len() && normalized[end] == '\n' {
                end += 1;
            }
            if end - p >= 2 {
                let segment = &normalized[start..p];
                let words = parse_words(segment);
                if !words.is_empty() {
                    if let Some(candidate) = solve(
                        &words,
                        0,
                        0,
                        true,
                        options.width,
                        options.first_indent,
                        options.rest_indent,
                        options.hyphen_cost,
                    ) {
                        result.extend(render(candidate, options.width, options.justify));
                    }
                }
                result.extend(std::iter::repeat_n(String::new(), end - p - 1));
                start = end;
            }
            p = end;
        } else {
            p += 1;
        }
    }

    let segment = &normalized[start..];
    let words = parse_words(segment);
    if !words.is_empty() {
        if let Some(candidate) = solve(
            &words,
            0,
            0,
            true,
            options.width,
            options.first_indent,
            options.rest_indent,
            options.hyphen_cost,
        ) {
            result.extend(render(candidate, options.width, options.justify));
        }
    }
    Ok(result)
}

fn main() {}