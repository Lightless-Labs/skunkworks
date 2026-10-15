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
    consumed: usize,
    marker_ended: bool,
    raw: bool,
    gap: usize,
}

#[derive(Clone)]
struct Solution {
    cost: u64,
    marker_lines: usize,
    lines: Vec<Line>,
}

fn is_sep(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{00a0}')
}

fn spelling_slice(chars: &[char], start: usize, end: usize) -> String {
    chars[start..end].iter().collect()
}

fn line_text(line: &Line) -> String {
    let mut s = " ".repeat(line.indent);
    for (i, piece) in line.pieces.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(piece);
    }
    s
}

fn better(a: &Solution, b: &Solution) -> bool {
    if a.cost != b.cost {
        return a.cost < b.cost;
    }
    if a.marker_lines != b.marker_lines {
        return a.marker_lines < b.marker_lines;
    }
    if a.lines.len() != b.lines.len() {
        return a.lines.len() < b.lines.len();
    }
    for (x, y) in a.lines.iter().zip(&b.lines) {
        if x.consumed != y.consumed {
            return x.consumed > y.consumed;
        }
    }
    for (x, y) in a.lines.iter().zip(&b.lines) {
        let xs = line_text(x);
        let ys = line_text(y);
        if xs != ys {
            return xs < ys;
        }
    }
    false
}

fn append_line_cost(
    suffix: &Solution,
    mut line: Line,
    options: &WrapOptions,
    final_line: bool,
) -> Solution {
    let mut result = suffix.clone();
    let square = (line.gap as u64) * (line.gap as u64);
    result.cost += if final_line { square / 4 } else { square };
    if line.marker_ended {
        result.cost += options.hyphen_cost as u64;
        result.marker_lines += 1;
    }
    if line.raw {
        result.cost += (2 * options.width + 1) as u64;
    }
    if !final_line && line.pieces.len() == 1 {
        result.cost += 3;
    }
    line.gap = line.gap;
    result.lines.insert(0, line);
    result
}

fn solve_from(
    words: &[Word],
    wi: usize,
    offset: usize,
    first: bool,
    options: &WrapOptions,
    memo: &mut Vec<Vec<[Option<Solution>; 2]>>,
) -> Option<Solution> {
    if wi >= words.len() {
        return Some(Solution {
            cost: 0,
            marker_lines: 0,
            lines: Vec::new(),
        });
    }
    let first_idx = if first { 1 } else { 0 };
    if let Some(cached) = &memo[wi][offset][first_idx] {
        return Some(cached.clone());
    }

    let indent = if first {
        options.first_indent
    } else {
        options.rest_indent
    };
    let capacity = options.width - indent;
    let mut best: Option<Solution> = None;

    // Enumerate line endings, adding whole words or a prefix of the current word.
    let mut pieces = Vec::<String>::new();
    let mut used = 0usize;
    let mut cur_wi = wi;
    let mut cur_off = offset;

    loop {
        let word = &words[cur_wi];
        let remaining = word.chars.len() - cur_off;
        let separator = if pieces.is_empty() { 0 } else { 1 };
        if used + separator >= capacity {
            break;
        }
        let room = capacity - used - separator;

        // A completed word may end this line.
        if remaining <= room {
            let piece = spelling_slice(&word.chars, cur_off, word.chars.len());
            pieces.push(piece);
            used += separator + remaining;
            let next_wi = cur_wi + 1;
            if let Some(tail) = solve_from(words, next_wi, 0, false, options, memo) {
                let line = Line {
                    indent,
                    pieces: pieces.clone(),
                    consumed: used,
                    marker_ended: false,
                    raw: false,
                    gap: options.width - indent - used,
                };
                let candidate = append_line_cost(&tail, line, options, tail.lines.is_empty());
                if best.as_ref().map_or(true, |b| better(&candidate, b)) {
                    best = Some(candidate);
                }
            }
            pieces.pop();

            // Continue with another whole word if it fits.
            used += separator + remaining;
            cur_wi += 1;
            cur_off = 0;
            if cur_wi >= words.len() {
                break;
            }
            continue;
        }

        // Marker-ended prefix choices in this word.
        for &m in &word.markers {
            if m <= cur_off || m >= word.chars.len() {
                continue;
            }
            let prefix_len = m - cur_off;
            if prefix_len + 1 > room {
                continue;
            }
            let mut piece = spelling_slice(&word.chars, cur_off, m);
            piece.push('-');
            let mut line_pieces = pieces.clone();
            line_pieces.push(piece);
            let next_offset = m;
            if let Some(tail) = solve_from(words, cur_wi, next_offset, false, options, memo) {
                let displayed = used + separator + prefix_len + 1;
                let line = Line {
                    indent,
                    pieces: line_pieces,
                    consumed: used + separator + prefix_len,
                    marker_ended: true,
                    raw: false,
                    gap: options.width - indent - displayed,
                };
                let candidate = append_line_cost(&tail, line, options, false);
                if best.as_ref().map_or(true, |b| better(&candidate, b)) {
                    best = Some(candidate);
                }
            }
        }

        // Raw split is allowed only on an otherwise empty line, and only if
        // no remaining marker prefix plus hyphen fits.
        if pieces.is_empty() && remaining > capacity {
            let marker_fits = word.markers.iter().any(|&m| {
                m > cur_off && m < word.chars.len() && (m - cur_off) + 1 <= capacity
            });
            if !marker_fits {
                let end = cur_off + capacity;
                let piece = spelling_slice(&word.chars, cur_off, end);
                if let Some(tail) = solve_from(words, cur_wi, end, false, options, memo) {
                    let line = Line {
                        indent,
                        pieces: vec![piece],
                        consumed: capacity,
                        marker_ended: false,
                        raw: true,
                        gap: 0,
                    };
                    let candidate = append_line_cost(&tail, line, options, false);
                    if best.as_ref().map_or(true, |b| better(&candidate, b)) {
                        best = Some(candidate);
                    }
                }
            }
        }
        break;
    }

    memo[wi][offset][first_idx] = best.clone();
    best
}

fn layout_paragraph(words: &[Word], options: &WrapOptions) -> Vec<String> {
    if words.is_empty() {
        return Vec::new();
    }
    let mut memo: Vec<Vec<[Option<Solution>; 2]>> = words
        .iter()
        .map(|w| vec![[None, None]; w.chars.len() + 1])
        .collect();
    let solution = solve_from(words, 0, 0, true, options, &mut memo)
        .expect("raw splitting ensures a legal layout");
    let final_index = solution.lines.len().saturating_sub(1);
    solution
        .lines
        .iter()
        .enumerate()
        .map(|(line_index, line)| {
            let mut rendered = line_text(line);
            if options.justify
                && line_index != final_index
                && line.pieces.len() >= 2
            {
                let gaps = line.pieces.len() - 1;
                let current_width = line.indent + line.pieces.iter().map(|p| p.chars().count()).sum::<usize>() + gaps;
                let extra = options.width - current_width;
                let each = extra / gaps;
                let remainder = extra % gaps;
                let start_gap = line_index % gaps;
                let mut out = " ".repeat(line.indent);
                for (i, piece) in line.pieces.iter().enumerate() {
                    if i > 0 {
                        let gap_idx = i - 1;
                        let bonus = (0..remainder).any(|r| (start_gap + r) % gaps == gap_idx);
                        out.push_str(&" ".repeat(1 + each + usize::from(bonus)));
                    }
                    out.push_str(piece);
                }
                rendered = out;
            }
            rendered
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
    if chars.len() > 512 {
        return Err(WrapError::InputTooLong { chars: chars.len() });
    }

    for &(index, c) in &chars {
        if c.is_control() && !matches!(c, '\t' | '\r' | '\n') {
            return Err(WrapError::InvalidControl { index });
        }
    }

    for i in 0..chars.len() {
        if chars[i].1 != '~' {
            continue;
        }
        let invalid = i == 0
            || i + 1 == chars.len()
            || chars[i - 1].1 == '~'
            || chars[i + 1].1 == '~'
            || is_sep(chars[i - 1].1)
            || is_sep(chars[i + 1].1);
        if invalid {
            return Err(WrapError::InvalidMarker { index: chars[i].0 });
        }
    }

    let mut normalized = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i].1;
        if c == '\r' {
            if i + 1 < chars.len() && chars[i + 1].1 == '\n' {
                i += 1;
            }
            normalized.push('\n');
        } else {
            normalized.push(c);
        }
        i += 1;
    }

    let mut output = Vec::new();
    let mut segment = Vec::new();
    let mut p = 0;
    while p < normalized.len() {
        if normalized[p] == '\n' {
            let start = p;
            while p < normalized.len() && normalized[p] == '\n' {
                p += 1;
            }
            let run = p - start;
            if run >= 2 {
                let words = parse_words(&segment);
                output.extend(layout_paragraph(&words, options));
                segment.clear();
                output.extend(std::iter::repeat(String::new()).take(run - 1));
            } else {
                segment.push('\n');
            }
        } else {
            segment.push(normalized[p]);
            p += 1;
        }
    }
    let words = parse_words(&segment);
    output.extend(layout_paragraph(&words, options));
    Ok(output)
}

fn parse_words(segment: &[char]) -> Vec<Word> {
    let mut words = Vec::new();
    let mut i = 0;
    while i < segment.len() {
        while i < segment.len() && is_sep(segment[i]) {
            i += 1;
        }
        if i >= segment.len() {
            break;
        }
        let mut raw = Vec::new();
        while i < segment.len() && !is_sep(segment[i]) {
            raw.push(segment[i]);
            i += 1;
        }
        let mut chars = Vec::new();
        let mut markers = Vec::new();
        for c in raw {
            if c == '~' {
                markers.push(chars.len());
            } else {
                chars.push(c);
            }
        }
        words.push(Word { chars, markers });
    }
    words
}