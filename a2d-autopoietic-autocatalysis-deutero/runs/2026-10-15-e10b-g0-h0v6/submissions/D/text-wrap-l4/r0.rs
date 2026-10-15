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
    marker_offsets: Vec<usize>,
}

#[derive(Clone)]
struct Line {
    indent: usize,
    text: String,
    pieces: usize,
    ended_at_marker: bool,
    raw_split: bool,
    consumed: usize,
    gap: usize,
}

#[derive(Clone)]
struct Layout {
    cost: u64,
    marker_lines: usize,
    lines: Vec<Line>,
}

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{00A0}')
}

fn spelling_count(line: &Line) -> usize {
    line.consumed
}

fn layout_better(a: &Layout, b: &Layout) -> bool {
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
        let xs = spelling_count(x);
        let ys = spelling_count(y);
        if xs != ys {
            return xs > ys;
        }
    }
    for (x, y) in a.lines.iter().zip(&b.lines) {
        if x.text != y.text {
            return x.text < y.text;
        }
    }
    false
}

fn build_line(
    words: &[Word],
    start_word: usize,
    start_offset: usize,
    end_word: usize,
    end_offset: usize,
    marker_end: bool,
    raw: bool,
    indent: usize,
) -> Line {
    let mut text = " ".repeat(indent);
    let mut pieces = 0;
    let mut consumed = 0;

    for wi in start_word..=end_word {
        let w = &words[wi];
        let from = if wi == start_word { start_offset } else { 0 };
        let to = if wi == end_word { end_offset } else { w.chars.len() };
        if from < to {
            if pieces > 0 {
                text.push(' ');
            }
            for &c in &w.chars[from..to] {
                text.push(c);
            }
            pieces += 1;
            consumed += to - from;
        }
    }
    if marker_end {
        text.push('-');
    }
    Line {
        indent,
        text,
        pieces,
        ended_at_marker: marker_end,
        raw_split: raw,
        consumed,
        gap: 0,
    }
}

fn optimize_paragraph(words: &[Word], options: &WrapOptions) -> Vec<Line> {
    if words.is_empty() {
        return Vec::new();
    }

    let n = words.len();
    let width = options.width;
    let mut memo: Vec<Vec<Option<Layout>>> = words
        .iter()
        .map(|w| vec![None; w.chars.len() + 1])
        .collect();
    let mut visiting = vec![vec![false; words[0].chars.len() + 1]; n];

    fn solve(
        wi: usize,
        off: usize,
        first: bool,
        words: &[Word],
        options: &WrapOptions,
        memo: &mut [Vec<Option<Layout>>],
        visiting: &mut [Vec<bool>],
    ) -> Layout {
        let width = options.width;
        let indent = if first { options.first_indent } else { options.rest_indent };
        let cap = width - indent;

        if wi >= words.len() {
            return Layout { cost: 0, marker_lines: 0, lines: Vec::new() };
        }

        // Memoization is separated by first-line status via a local key encoding.
        // The recursive state graph is acyclic because every line consumes spelling.
        if visiting[wi][off] {
            return Layout { cost: u64::MAX / 4, marker_lines: usize::MAX / 4, lines: Vec::new() };
        }
        visiting[wi][off] = true;

        let mut best: Option<Layout> = None;
        let mut consider = |line: Line, next_w: usize, next_o: usize, marker: bool, raw: bool| {
            let tail = if next_w >= words.len() {
                Layout { cost: 0, marker_lines: 0, lines: Vec::new() }
            } else {
                solve(next_w, next_o, false, words, options, memo, visiting)
            };
            if tail.cost >= u64::MAX / 8 {
                return;
            }
            let displayed = line.text.chars().count();
            let gap = width - displayed;
            let final_line = next_w >= words.len();
            let mut line_cost = (gap as u64) * (gap as u64);
            if final_line {
                line_cost /= 4;
            }
            if marker {
                line_cost += options.hyphen_cost as u64;
            }
            if raw {
                line_cost += (2 * width + 1) as u64;
            }
            if !final_line && line.pieces == 1 {
                line_cost += 3;
            }
            let mut full = line;
            full.gap = gap;
            let mut candidate = Layout {
                cost: line_cost + tail.cost,
                marker_lines: tail.marker_lines + usize::from(marker),
                lines: Vec::with_capacity(tail.lines.len() + 1),
            };
            candidate.lines.push(full);
            candidate.lines.extend(tail.lines);
            if best.as_ref().map_or(true, |b| layout_better(&candidate, b)) {
                best = Some(candidate);
            }
        };

        let start_word = wi;
        let start_off = off;
        let mut used = 0usize;
        let mut end_word = wi;

        loop {
            let word = &words[end_word];
            let remaining = word.chars.len() - if end_word == start_word { start_off } else { 0 };
            let sep = usize::from(end_word != start_word);
            let available = cap.saturating_sub(used + sep);
            let from = if end_word == start_word { start_off } else { 0 };

            if remaining <= available {
                used += sep + remaining;
                let line = build_line(words, start_word, start_off, end_word, word.chars.len(), false, false, indent);
                consider(line, end_word + 1, 0, false, false);
                end_word += 1;
                if end_word >= words.len() {
                    break;
                }
                continue;
            }

            // Marker breaks in this word.
            let fitting_markers: Vec<usize> = word.marker_offsets
                .iter()
                .copied()
                .filter(|&m| m > from && m < word.chars.len() && m - from + 1 <= available)
                .collect();
            for m in &fitting_markers {
                let line = build_line(words, start_word, start_off, end_word, *m, true, false, indent);
                consider(line, end_word, *m, true, false);
            }

            // Raw split is allowed only when the line contains no prior piece and
            // no remaining marker break can fit.
            if end_word == start_word && used == 0 && remaining > cap {
                let any_marker_fits = word.marker_offsets.iter().any(|&m| {
                    m > from && m < word.chars.len() && m - from + 1 <= cap
                });
                if !any_marker_fits {
                    let raw_end = from + cap;
                    let line = build_line(words, start_word, start_off, end_word, raw_end, false, true, indent);
                    consider(line, end_word, raw_end, false, true);
                }
            }
            break;
        }

        visiting[wi][off] = false;
        best.unwrap_or(Layout { cost: u64::MAX / 4, marker_lines: usize::MAX / 4, lines: Vec::new() })
    }

    // Distinct visiting tables are needed for first and subsequent line states.
    let mut visit = words.iter().map(|w| vec![false; w.chars.len() + 1]).collect::<Vec<_>>();
    let mut result = solve(0, 0, true, words, options, &mut memo, &mut visit);
    if result.cost >= u64::MAX / 8 {
        return Vec::new();
    }
    result.lines
}

pub fn wrap(input: &str, options: &WrapOptions) -> Result<Vec<String>, WrapError> {
    if options.width == 0 {
        return Err(WrapError::WidthZero);
    }
    if options.width > 256 {
        return Err(WrapError::WidthTooLarge { width: options.width });
    }
    if options.first_indent >= options.width {
        return Err(WrapError::FirstIndentTooWide { indent: options.first_indent });
    }
    if options.rest_indent >= options.width {
        return Err(WrapError::RestIndentTooWide { indent: options.rest_indent });
    }
    if options.hyphen_cost > 1_000_000 {
        return Err(WrapError::HyphenCostTooLarge { cost: options.hyphen_cost });
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
        let valid_side = |j: isize| -> bool {
            if j < 0 || j as usize >= chars.len() {
                return false;
            }
            let c = chars[j as usize].1;
            c != '~' && !is_separator(c)
        };
        if !valid_side(i as isize - 1) || !valid_side(i as isize + 1) {
            return Err(WrapError::InvalidMarker { index: chars[i].0 });
        }
    }

    let mut normalized = String::new();
    let mut iter = input.chars().peekable();
    while let Some(c) = iter.next() {
        if c == '\r' {
            if iter.peek() == Some(&'\n') {
                iter.next();
            }
            normalized.push('\n');
        } else {
            normalized.push(c);
        }
    }
    let scalars: Vec<char> = normalized.chars().collect();

    let mut segments: Vec<Vec<char>> = vec![Vec::new()];
    let mut boundaries_after: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < scalars.len() {
        if scalars[i] == '\n' {
            let mut j = i;
            while j < scalars.len() && scalars[j] == '\n' {
                j += 1;
            }
            if j - i >= 2 {
                boundaries_after.push(j - i - 1);
                segments.push(Vec::new());
            } else {
                segments.last_mut().unwrap().push('\n');
            }
            i = j;
        } else {
            segments.last_mut().unwrap().push(scalars[i]);
            i += 1;
        }
    }

    let mut output = Vec::new();
    for (si, segment) in segments.iter().enumerate() {
        let mut words = Vec::new();
        let mut current_chars = Vec::new();
        let mut markers = Vec::new();
        for &c in segment {
            if is_separator(c) {
                if !current_chars.is_empty() {
                    words.push(Word { chars: std::mem::take(&mut current_chars), marker_offsets: std::mem::take(&mut markers) });
                }
            } else if c == '~' {
                markers.push(current_chars.len());
            } else {
                current_chars.push(c);
            }
        }
        if !current_chars.is_empty() {
            words.push(Word { chars: current_chars, marker_offsets: markers });
        }

        let lines = optimize_paragraph(&words, options);
        let line_count = lines.len();
        for (li, line) in lines.iter().enumerate() {
            let mut rendered = line.text.clone();
            if options.justify && li + 1 < line_count && line.pieces >= 2 {
                let e = line.gap;
                let gaps = line.pieces - 1;
                let base = e / gaps;
                let extra = e % gaps;
                let start = li % gaps;
                let chars: Vec<char> = rendered.chars().collect();
                let mut justified = String::new();
                let mut piece_seen = 0usize;
                let mut pos = line.indent;
                while pos < chars.len() {
                    if chars[pos] == ' ' {
                        while pos < chars.len() && chars[pos] == ' ' {
                            justified.push(' ');
                            pos += 1;
                        }
                        let gap_index = piece_seen.saturating_sub(1);
                        let add = base + usize::from((gap_index + gaps - start) % gaps < extra);
                        justified.push_str(&" ".repeat(add));
                    } else {
                        justified.push(chars[pos]);
                        pos += 1;
                        if pos == chars.len() || (pos < chars.len() && chars[pos] != ' ') {
                            // Scalar boundaries do not reveal pieces; count starts below.
                        }
                    }
                }
                // Rebuild using stored single-space separators to avoid treating indentation as gaps.
                let content = rendered.chars().skip(line.indent).collect::<String>();
                let mut split = content.split(' ');
                let mut rebuilt = " ".repeat(line.indent);
                for (p, part) in split.by_ref().enumerate() {
                    if p > 0 {
                        let gap_index = p - 1;
                        let add = 1 + base + usize::from((gap_index + gaps - start) % gaps < extra);
                        rebuilt.push_str(&" ".repeat(add));
                    }
                    rebuilt.push_str(part);
                }
                rendered = rebuilt;
            }
            output.push(rendered);
        }
        if si < boundaries_after.len() {
            for _ in 0..boundaries_after[si] {
                output.push(String::new());
            }
        }
    }
    Ok(output)
}