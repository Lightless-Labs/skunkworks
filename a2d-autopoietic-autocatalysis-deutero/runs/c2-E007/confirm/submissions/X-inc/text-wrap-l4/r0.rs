use std::cmp::Ordering;
use std::collections::HashMap;

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
struct Piece {
    text: String,
    words: usize,
}

#[derive(Clone)]
struct Line {
    pieces: Vec<Piece>,
    consumed: usize,
    gap: usize,
    marker_end: bool,
    raw_split: bool,
    indent: usize,
}

#[derive(Clone)]
struct Plan {
    cost: u64,
    marker_count: usize,
    lines: Vec<Line>,
    consumed: Vec<usize>,
    pre_rendered: Vec<String>,
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct State {
    word: usize,
    offset: usize,
    first: bool,
}

fn plan_cmp(a: &Plan, b: &Plan) -> Ordering {
    a.cost
        .cmp(&b.cost)
        .then_with(|| a.marker_count.cmp(&b.marker_count))
        .then_with(|| a.lines.len().cmp(&b.lines.len()))
        .then_with(|| {
            for (x, y) in a.consumed.iter().zip(&b.consumed) {
                match y.cmp(x) {
                    Ordering::Equal => {}
                    order => return order,
                }
            }
            a.consumed.len().cmp(&b.consumed.len())
        })
        .then_with(|| a.pre_rendered.cmp(&b.pre_rendered))
}

fn render_pre(line: &Line) -> String {
    let mut out = " ".repeat(line.indent);
    for (i, piece) in line.pieces.iter().enumerate() {
        if i != 0 {
            out.push(' ');
        }
        out.push_str(&piece.text);
    }
    out
}

fn solve(
    state: State,
    words: &[Word],
    opt: &WrapOptions,
    memo: &mut HashMap<State, Option<Plan>>,
) -> Option<Plan> {
    if state.word >= words.len() {
        return Some(Plan {
            cost: 0,
            marker_count: 0,
            lines: Vec::new(),
            consumed: Vec::new(),
            pre_rendered: Vec::new(),
        });
    }
    if let Some(v) = memo.get(&state) {
        return v.clone();
    }

    let indent = if state.first {
        opt.first_indent
    } else {
        opt.rest_indent
    };
    let cap = opt.width - indent;
    let mut best: Option<Plan> = None;
    let mut pieces = Vec::<Piece>::new();
    let mut used = 0usize;
    let mut w = state.word;
    let mut start = state.offset;

    loop {
        let word = &words[w];
        let remaining = word.chars.len() - start;
        let separator = usize::from(!pieces.is_empty());
        let avail = cap.saturating_sub(used + separator);

        // End the line after any completed word.
        if remaining <= avail {
            let text: String = word.chars[start..].iter().collect();
            pieces.push(Piece { text, words: 1 });
            let displayed = used + separator + remaining;
            let next = State {
                word: w + 1,
                offset: 0,
                first: false,
            };
            consider_line(
                &mut best,
                state,
                next,
                words,
                opt,
                memo,
                pieces.clone(),
                displayed,
                false,
                false,
                remaining + separator,
            );
            pieces.pop();
        }

        // Marker-ended lines at any marker strictly inside this suffix.
        for &mark in &word.markers {
            if mark <= start || mark >= word.chars.len() {
                continue;
            }
            let prefix_len = mark - start;
            let total = used + separator + prefix_len + 1;
            if total <= cap {
                let mut text: String = word.chars[start..mark].iter().collect();
                text.push('-');
                pieces.push(Piece { text, words: 1 });
                let next = State {
                    word: w,
                    offset: mark,
                    first: false,
                };
                consider_line(
                    &mut best,
                    state,
                    next,
                    words,
                    opt,
                    memo,
                    pieces.clone(),
                    total,
                    true,
                    false,
                    prefix_len + separator,
                );
                pieces.pop();
            }
        }

        // Raw split is allowed only on an otherwise empty line and only if
        // no remaining marker prefix plus hyphen fits.
        if pieces.is_empty() && remaining > cap {
            let marker_fits = word.markers.iter().any(|&mark| {
                mark > start && mark < word.chars.len() && (mark - start) + 1 <= cap
            });
            if !marker_fits {
                let text: String = word.chars[start..start + cap].iter().collect();
                pieces.push(Piece { text, words: 1 });
                let next = if start + cap == word.chars.len() {
                    State {
                        word: w + 1,
                        offset: 0,
                        first: false,
                    }
                } else {
                    State {
                        word: w,
                        offset: start + cap,
                        first: false,
                    }
                };
                consider_line(
                    &mut best,
                    state,
                    next,
                    words,
                    opt,
                    memo,
                    pieces.clone(),
                    cap,
                    false,
                    true,
                    cap,
                );
                pieces.pop();
            }
        }

        // Continue by placing the complete word and then another word.
        if remaining > avail || w + 1 >= words.len() {
            break;
        }
        let text: String = word.chars[start..].iter().collect();
        pieces.push(Piece { text, words: 1 });
        used += separator + remaining;
        w += 1;
        start = 0;
        if used >= cap {
            break;
        }
    }

    memo.insert(state, best.clone());
    best
}

#[allow(clippy::too_many_arguments)]
fn consider_line(
    best: &mut Option<Plan>,
    _state: State,
    next: State,
    words: &[Word],
    opt: &WrapOptions,
    memo: &mut HashMap<State, Option<Plan>>,
    pieces: Vec<Piece>,
    displayed: usize,
    marker_end: bool,
    raw_split: bool,
    consumed: usize,
) {
    let indent = if _state.first {
        opt.first_indent
    } else {
        opt.rest_indent
    };
    let line_width = indent + displayed;
    let gap = opt.width - line_width;
    let suffix = match solve(next, words, opt, memo) {
        Some(p) => p,
        None => return,
    };
    let is_final = suffix.lines.is_empty();
    let mut cost = suffix.cost + if is_final {
        (gap * gap / 4) as u64
    } else {
        (gap * gap) as u64
    };
    if marker_end {
        cost += opt.hyphen_cost as u64;
    }
    if raw_split {
        cost += (2 * opt.width + 1) as u64;
    }
    if !is_final && pieces.len() == 1 {
        cost += 3;
    }

    let mut line = Line {
        pieces,
        consumed,
        gap,
        marker_end,
        raw_split,
        indent,
    };
    let mut candidate = Plan {
        cost,
        marker_count: suffix.marker_count + usize::from(marker_end),
        lines: Vec::with_capacity(suffix.lines.len() + 1),
        consumed: Vec::with_capacity(suffix.consumed.len() + 1),
        pre_rendered: Vec::with_capacity(suffix.pre_rendered.len() + 1),
    };
    candidate.lines.push(line.clone());
    candidate.lines.extend(suffix.lines);
    candidate.consumed.push(consumed);
    candidate.consumed.extend(suffix.consumed);
    candidate.pre_rendered.push(render_pre(&line));
    candidate.pre_rendered.extend(suffix.pre_rendered);

    if best.as_ref().map_or(true, |old| plan_cmp(&candidate, old) == Ordering::Less) {
        *best = Some(candidate);
    }
}

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{00a0}')
}

fn make_words(chars: &[char]) -> Vec<Word> {
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

fn solve_paragraph(words: &[Word], opt: &WrapOptions) -> Vec<String> {
    if words.is_empty() {
        return Vec::new();
    }
    let mut memo = HashMap::new();
    let plan = solve(
        State {
            word: 0,
            offset: 0,
            first: true,
        },
        words,
        opt,
        &mut memo,
    )
    .expect("raw splits guarantee a legal layout");

    let mut output = Vec::with_capacity(plan.lines.len());
    for (line_index, line) in plan.lines.iter().enumerate() {
        let mut rendered = " ".repeat(line.indent);
        let gap_count = line.pieces.len().saturating_sub(1);
        let justify = opt.justify
            && line_index + 1 < plan.lines.len()
            && line.pieces.len() >= 2;
        let extra = if justify { line.gap } else { 0 };
        let each = if justify && gap_count > 0 {
            extra / gap_count
        } else {
            0
        };
        let rem = if justify && gap_count > 0 {
            extra % gap_count
        } else {
            0
        };
        for (i, piece) in line.pieces.iter().enumerate() {
            if i > 0 {
                rendered.push(' ');
                if justify {
                    let rotation = line_index % gap_count;
                    let cyclic_index = (i - 1 + gap_count - rotation) % gap_count;
                    rendered.push_str(&" ".repeat(each + usize::from(cyclic_index < rem)));
                }
            }
            rendered.push_str(&piece.text);
        }
        output.push(rendered);
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

    let chars: Vec<char> = input.chars().collect();
    if chars.len() > 512 {
        return Err(WrapError::InputTooLong { chars: chars.len() });
    }

    let mut byte_index = 0;
    for &c in &chars {
        if c.is_control() && !matches!(c, '\t' | '\r' | '\n') {
            return Err(WrapError::InvalidControl { index: byte_index });
        }
        byte_index += c.len_utf8();
    }

    byte_index = 0;
    let mut marker_error = None;
    for (i, &c) in chars.iter().enumerate() {
        if c == '~' {
            let valid_left = i > 0 && !is_separator(chars[i - 1]) && chars[i - 1] != '~';
            let valid_right =
                i + 1 < chars.len() && !is_separator(chars[i + 1]) && chars[i + 1] != '~';
            if (!valid_left || !valid_right) && marker_error.is_none() {
                marker_error = Some(byte_index);
            }
        }
        byte_index += c.len_utf8();
    }
    if let Some(index) = marker_error {
        return Err(WrapError::InvalidMarker { index });
    }

    // Normalize CRLF pairs to a single LF, and any remaining CR to LF.
    let mut normalized = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\r' {
            normalized.push('\n');
            if i + 1 < chars.len() && chars[i + 1] == '\n' {
                i += 1;
            }
        } else {
            normalized.push(chars[i]);
        }
        i += 1;
    }

    let mut output = Vec::new();
    let mut segment_start = 0;
    let mut pos = 0;
    while pos < normalized.len() {
        if normalized[pos] == '\n' {
            let run_start = pos;
            while pos < normalized.len() && normalized[pos] == '\n' {
                pos += 1;
            }
            if pos - run_start >= 2 {
                let words = make_words(&normalized[segment_start..run_start]);
                output.extend(solve_paragraph(&words, options));
                output.extend(std::iter::repeat(String::new()).take(pos - run_start - 1));
                segment_start = pos;
            }
        } else {
            pos += 1;
        }
    }
    let words = make_words(&normalized[segment_start..]);
    output.extend(solve_paragraph(&words, options));
    Ok(output)
}