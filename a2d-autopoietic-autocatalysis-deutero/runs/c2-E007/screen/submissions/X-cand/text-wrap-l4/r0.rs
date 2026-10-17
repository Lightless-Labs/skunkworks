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
    marker_end: bool,
    raw: bool,
    pieces_count: usize,
}

#[derive(Clone)]
struct Candidate {
    cost: u64,
    markers: usize,
    lines: Vec<Line>,
}

fn separators(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{00a0}')
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

fn consumed_vector(lines: &[Line]) -> Vec<usize> {
    lines.iter().map(|l| l.consumed).collect()
}

fn better(a: &Candidate, b: &Candidate) -> bool {
    if a.cost != b.cost {
        return a.cost < b.cost;
    }
    if a.markers != b.markers {
        return a.markers < b.markers;
    }
    if a.lines.len() != b.lines.len() {
        return a.lines.len() < b.lines.len();
    }
    let av = consumed_vector(&a.lines);
    let bv = consumed_vector(&b.lines);
    if av != bv {
        return av > bv;
    }
    let at: Vec<String> = a.lines.iter().map(line_text).collect();
    let bt: Vec<String> = b.lines.iter().map(line_text).collect();
    at < bt
}

fn add_line_cost(
    suffix: &Candidate,
    line: Line,
    width: usize,
    hyphen_cost: usize,
) -> Candidate {
    let displayed_width = line_text(&line).chars().count();
    let gap = width.saturating_sub(displayed_width) as u64;
    let nonfinal = !suffix.lines.is_empty();
    let mut cost = gap * gap;
    if !nonfinal {
        cost /= 4;
    }
    if line.marker_end {
        cost += hyphen_cost as u64;
    }
    if line.raw {
        cost += (2 * width + 1) as u64;
    }
    if nonfinal && line.pieces_count == 1 {
        cost += 3;
    }
    let mut lines = Vec::with_capacity(suffix.lines.len() + 1);
    lines.push(line);
    lines.extend(suffix.lines.iter().cloned());
    Candidate {
        cost: cost + suffix.cost,
        markers: suffix.markers + usize::from(lines[0].marker_end),
        lines,
    }
}

fn solve_paragraph(words: &[Word], options: &WrapOptions) -> Vec<Line> {
    #[derive(Clone, Copy, Hash, PartialEq, Eq)]
    struct State {
        word: usize,
        offset: usize,
        first: bool,
    }

    fn solve(
        state: State,
        words: &[Word],
        options: &WrapOptions,
        memo: &mut std::collections::HashMap<State, Option<Candidate>>,
    ) -> Option<Candidate> {
        if state.word == words.len() {
            return Some(Candidate {
                cost: 0,
                markers: 0,
                lines: Vec::new(),
            });
        }
        if let Some(cached) = memo.get(&state) {
            return cached.clone();
        }

        let indent = if state.first {
            options.first_indent
        } else {
            options.rest_indent
        };
        let capacity = options.width - indent;
        let mut best: Option<Candidate> = None;

        let mut pieces: Vec<String> = Vec::new();
        let mut used = 0usize;
        let mut consumed = 0usize;
        let mut wi = state.word;
        let mut off = state.offset;
        let mut can_continue = true;

        while wi < words.len() && can_continue {
            let word = &words[wi];
            let remaining = word.chars.len() - off;
            let gap = usize::from(!pieces.is_empty());
            if used + gap >= capacity {
                break;
            }
            let available = capacity - used - gap;

            // Every marker ending that fits is a legal line ending, even if
            // the whole remaining suffix could also fit on this line.
            for &m in word
                .markers
                .iter()
                .filter(|&&m| m > off && m - off + 1 <= available)
            {
                let prefix_len = m - off;
                let mut p: String = word.chars[off..m].iter().collect();
                p.push('-');
                let mut line_pieces = pieces.clone();
                line_pieces.push(p);
                let tail = solve(
                    State {
                        word: wi,
                        offset: m,
                        first: false,
                    },
                    words,
                    options,
                    memo,
                );
                if let Some(tail) = tail {
                    let line = Line {
                        indent,
                        pieces: line_pieces,
                        consumed: consumed + prefix_len,
                        marker_end: true,
                        raw: false,
                        pieces_count: pieces.len() + 1,
                    };
                    let cand = add_line_cost(&tail, line, options.width, options.hyphen_cost);
                    if best.as_ref().map_or(true, |b| better(&cand, b)) {
                        best = Some(cand);
                    }
                }
            }

            if remaining <= available {
                pieces.push(word.chars[off..].iter().collect::<String>());
                used += gap + remaining;
                consumed += remaining;
                wi += 1;
                off = 0;

                let tail = solve(
                    State {
                        word: wi,
                        offset: 0,
                        first: false,
                    },
                    words,
                    options,
                    memo,
                );
                if let Some(tail) = tail {
                    let line = Line {
                        indent,
                        pieces: pieces.clone(),
                        consumed,
                        marker_end: false,
                        raw: false,
                        pieces_count: pieces.len(),
                    };
                    let cand = add_line_cost(&tail, line, options.width, options.hyphen_cost);
                    if best.as_ref().map_or(true, |b| better(&cand, b)) {
                        best = Some(cand);
                    }
                }
                continue;
            }

            // A raw split is allowed only on an empty line when no remaining
            // marker prefix plus hyphen fits in the line capacity.
            if pieces.is_empty() {
                let marker_fits = word
                    .markers
                    .iter()
                    .any(|&m| m > off && m - off + 1 <= capacity);
                if !marker_fits && capacity > 0 {
                    let tail = solve(
                        State {
                            word: wi,
                            offset: off + capacity,
                            first: false,
                        },
                        words,
                        options,
                        memo,
                    );
                    if let Some(tail) = tail {
                        let line = Line {
                            indent,
                            pieces: vec![word.chars[off..off + capacity].iter().collect()],
                            consumed: capacity,
                            marker_end: false,
                            raw: true,
                            pieces_count: 1,
                        };
                        let cand = add_line_cost(&tail, line, options.width, options.hyphen_cost);
                        if best.as_ref().map_or(true, |b| better(&cand, b)) {
                            best = Some(cand);
                        }
                    }
                }
            }
            can_continue = false;
        }

        memo.insert(state, best.clone());
        best
    }

    let mut memo = std::collections::HashMap::new();
    solve(
        State {
            word: 0,
            offset: 0,
            first: true,
        },
        words,
        options,
        &mut memo,
    )
    .map(|c| c.lines)
    .unwrap_or_default()
}

fn parse_words(segment: &[char]) -> Vec<Word> {
    let mut words = Vec::new();
    let mut chars = Vec::new();
    let mut markers = Vec::new();

    let finish = |words: &mut Vec<Word>, chars: &mut Vec<char>, markers: &mut Vec<usize>| {
        if !chars.is_empty() {
            words.push(Word {
                chars: std::mem::take(chars),
                markers: std::mem::take(markers),
            });
        }
    };

    for &c in segment {
        if separators(c) {
            finish(&mut words, &mut chars, &mut markers);
        } else if c == '~' {
            markers.push(chars.len());
        } else {
            chars.push(c);
        }
    }
    finish(&mut words, &mut chars, &mut markers);
    words
}

fn render_lines(lines: &[Line], options: &WrapOptions) -> Vec<String> {
    lines
        .iter()
        .enumerate()
        .map(|(line_index, line)| {
            let mut s = " ".repeat(line.indent);
            let pre_width = line_text(line).chars().count();
            let extra = options.width.saturating_sub(pre_width);
            let gaps = line.pieces_count.saturating_sub(1);
            for (i, piece) in line.pieces.iter().enumerate() {
                if i > 0 {
                    let mut spaces = 1usize;
                    if options.justify
                        && line_index + 1 < lines.len()
                        && line.pieces_count >= 2
                    {
                        let base = extra / gaps;
                        let remainder = extra % gaps;
                        let start_gap = line_index % gaps;
                        let gap_index = i - 1;
                        let gets_remainder = (gap_index + gaps - start_gap) % gaps < remainder;
                        spaces += base + usize::from(gets_remainder);
                    }
                    s.push_str(&" ".repeat(spaces));
                }
                s.push_str(piece);
            }
            s
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

    let original: Vec<(usize, char)> = input.char_indices().collect();
    if original.len() > 512 {
        return Err(WrapError::InputTooLong {
            chars: original.len(),
        });
    }

    for &(index, c) in &original {
        if c.is_control() && !matches!(c, '\t' | '\r' | '\n') {
            return Err(WrapError::InvalidControl { index });
        }
    }

    for i in 0..original.len() {
        if original[i].1 != '~' {
            continue;
        }
        let left_ok = i > 0 && original[i - 1].1 != '~' && !separators(original[i - 1].1);
        let right_ok = i + 1 < original.len()
            && original[i + 1].1 != '~'
            && !separators(original[i + 1].1);
        if !left_ok || !right_ok {
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
            if i + 1 < original.len() && original[i + 1].1 == '\n' {
                i += 1;
            }
            normalized.push('\n');
        } else {
            normalized.push(c);
        }
        i += 1;
    }

    let mut output = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < normalized.len() {
        if normalized[i] == '\n' {
            let mut j = i;
            while j < normalized.len() && normalized[j] == '\n' {
                j += 1;
            }
            if j - i >= 2 {
                let words = parse_words(&normalized[start..i]);
                let lines = solve_paragraph(&words, options);
                output.extend(render_lines(&lines, options));
                output.extend(std::iter::repeat(String::new()).take(j - i - 1));
                start = j;
                i = j;
                continue;
            }
        }
        i += 1;
    }

    let words = parse_words(&normalized[start..]);
    let lines = solve_paragraph(&words, options);
    output.extend(render_lines(&lines, options));
    Ok(output)
}