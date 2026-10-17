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
    marker: bool,
    raw: bool,
    gap: usize,
    full: String,
}

#[derive(Clone)]
struct Plan {
    cost: u64,
    markers: usize,
    lines: Vec<Line>,
    consumed: Vec<usize>,
    fulls: Vec<String>,
}

fn better(a: &Plan, b: &Plan) -> bool {
    if a.cost != b.cost {
        return a.cost < b.cost;
    }
    if a.markers != b.markers {
        return a.markers < b.markers;
    }
    if a.lines.len() != b.lines.len() {
        return a.lines.len() < b.lines.len();
    }
    if a.consumed != b.consumed {
        return a.consumed > b.consumed;
    }
    a.fulls < b.fulls
}

fn is_sep(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{00a0}')
}

fn solve(
    words: &[Word],
    wi: usize,
    off: usize,
    first: bool,
    opts: &WrapOptions,
    memo: &mut std::collections::HashMap<(usize, usize, bool), Option<Plan>>,
) -> Option<Plan> {
    if wi == words.len() {
        return Some(Plan {
            cost: 0,
            markers: 0,
            lines: Vec::new(),
            consumed: Vec::new(),
            fulls: Vec::new(),
        });
    }
    let key = (wi, off, first);
    if let Some(v) = memo.get(&key) {
        return v.clone();
    }

    let indent = if first { opts.first_indent } else { opts.rest_indent };
    let cap = opts.width - indent;
    let mut best: Option<Plan> = None;

    let mut choices: Vec<(usize, usize, bool, bool)> = Vec::new();
    // (end word index, next offset, marker-ended, raw)
    let mut used = 0usize;
    let mut pieces = 0usize;
    let mut cur_w = wi;
    let mut cur_off = off;

    loop {
        if cur_w >= words.len() {
            break;
        }
        let word = &words[cur_w];
        let remaining = word.chars.len() - cur_off;
        let room = cap.saturating_sub(used + usize::from(pieces > 0));
        if remaining <= room {
            used += usize::from(pieces > 0) + remaining;
            pieces += 1;
            cur_w += 1;
            cur_off = 0;
            choices.push((cur_w, 0, false, false));
            continue;
        }

        let available = room;
        if available > 1 {
            for &m in &word.markers {
                if m > cur_off && m - cur_off + 1 <= available {
                    choices.push((cur_w, m, true, false));
                }
            }
        }

        if pieces == 0 && remaining > cap {
            let marker_fits = word
                .markers
                .iter()
                .any(|&m| m > cur_off && m - cur_off + 1 <= cap);
            if !marker_fits && cap > 0 {
                choices.push((cur_w, cur_off + cap, false, true));
            }
        }
        break;
    }

    for (next_w, next_off, marker_end, raw_end) in choices {
        let mut line_pieces = Vec::new();
        let mut line_used = 0usize;
        let mut count_pieces = 0usize;
        let mut w = wi;
        let mut o = off;
        let mut valid = true;

        while w < words.len() {
            let word = &words[w];
            if w == next_w && next_off > 0 {
                let end = next_off;
                if end <= o || end > word.chars.len() {
                    valid = false;
                    break;
                }
                let s: String = word.chars[o..end].iter().collect();
                line_used += usize::from(count_pieces > 0) + s.chars().count()
                    + usize::from(marker_end);
                line_pieces.push(s);
                count_pieces += 1;
                break;
            }
            if w >= next_w {
                break;
            }
            let s: String = word.chars[o..].iter().collect();
            line_used += usize::from(count_pieces > 0) + s.chars().count();
            line_pieces.push(s);
            count_pieces += 1;
            w += 1;
            o = 0;
        }
        if !valid || count_pieces == 0 || line_used > cap {
            continue;
        }

        let continuation = if marker_end || raw_end {
            Some((next_w, next_off, false))
        } else if next_w < words.len() {
            Some((next_w, 0, false))
        } else {
            None
        };
        let tail = match continuation {
            Some((nw, no, nf)) => match solve(words, nw, no, nf, opts, memo) {
                Some(p) => p,
                None => continue,
            },
            None => Plan {
                cost: 0,
                markers: 0,
                lines: Vec::new(),
                consumed: Vec::new(),
                fulls: Vec::new(),
            },
        };

        let gap = opts.width - indent - line_used;
        let is_final = continuation.is_none();
        let mut cost = if is_final {
            (gap * gap / 4) as u64
        } else {
            (gap * gap) as u64
        };
        if marker_end {
            cost += opts.hyphen_cost as u64;
        }
        if raw_end {
            cost += (2 * opts.width + 1) as u64;
        }
        if !is_final && count_pieces == 1 {
            cost += 3;
        }

        let mut pieces_render = line_pieces;
        if marker_end {
            if let Some(last) = pieces_render.last_mut() {
                last.push('-');
            }
        }
        let mut rendered = " ".repeat(indent);
        for (i, p) in pieces_render.iter().enumerate() {
            if i > 0 {
                rendered.push(' ');
            }
            rendered.push_str(p);
        }
        let line = Line {
            indent,
            pieces: pieces_render,
            consumed: line_used - usize::from(marker_end),
            marker: marker_end,
            raw: raw_end,
            gap,
            full: rendered.clone(),
        };

        let mut candidate = Plan {
            cost: cost + tail.cost,
            markers: usize::from(marker_end) + tail.markers,
            lines: vec![line],
            consumed: vec![line_used - usize::from(marker_end)],
            fulls: vec![rendered],
        };
        candidate.lines.extend(tail.lines);
        candidate.consumed.extend(tail.consumed);
        candidate.fulls.extend(tail.fulls);

        if best.as_ref().map_or(true, |b| better(&candidate, b)) {
            best = Some(candidate);
        }
    }

    memo.insert(key, best.clone());
    best
}

fn parse_segment(s: &str) -> Vec<Word> {
    let mut words = Vec::new();
    let mut chars = Vec::new();
    let mut markers = Vec::new();
    for c in s.chars() {
        if is_sep(c) {
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

fn render_line(line: &Line, width: usize, justify: bool, line_index: usize, final_line: bool) -> String {
    if !justify || final_line || line.pieces.len() < 2 {
        return line.full.clone();
    }
    let gaps = line.pieces.len() - 1;
    let extra = line.gap;
    let each = extra / gaps;
    let rem = extra % gaps;
    let mut out = " ".repeat(line.indent);
    for (i, piece) in line.pieces.iter().enumerate() {
        if i > 0 {
            let gap_index = i - 1;
            let add = each + usize::from((gap_index + gaps - (line_index % gaps)) % gaps < rem);
            out.push_str(&" ".repeat(1 + add));
        }
        out.push_str(piece);
    }
    let _ = width;
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
    let count = input.chars().count();
    if count > 512 {
        return Err(WrapError::InputTooLong { chars: count });
    }
    for (index, c) in input.char_indices() {
        if c.is_control() && !matches!(c, '\t' | '\r' | '\n') {
            return Err(WrapError::InvalidControl { index });
        }
    }

    let indexed: Vec<(usize, char)> = input.char_indices().collect();
    for i in 0..indexed.len() {
        if indexed[i].1 == '~' {
            let bad = i == 0
                || i + 1 == indexed.len()
                || indexed[i - 1].1 == '~'
                || indexed[i + 1].1 == '~'
                || is_sep(indexed[i - 1].1)
                || is_sep(indexed[i + 1].1);
            if bad {
                return Err(WrapError::InvalidMarker {
                    index: indexed[i].0,
                });
            }
        }
    }

    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let chars: Vec<char> = normalized.chars().collect();
    let mut segments: Vec<String> = Vec::new();
    let mut boundaries = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '\n' {
            let mut j = i;
            while j < chars.len() && chars[j] == '\n' {
                j += 1;
            }
            if j - i >= 2 {
                segments.push(chars[start..i].iter().collect());
                boundaries.push(j - i - 1);
                start = j;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    segments.push(chars[start..].iter().collect());

    let mut output = Vec::new();
    for (si, segment) in segments.iter().enumerate() {
        let words = parse_segment(segment);
        if !words.is_empty() {
            let mut memo = std::collections::HashMap::new();
            if let Some(plan) = solve(&words, 0, 0, true, options, &mut memo) {
                for (li, line) in plan.lines.iter().enumerate() {
                    output.push(render_line(
                        line,
                        options.width,
                        options.justify,
                        li,
                        li + 1 == plan.lines.len(),
                    ));
                }
            }
        }
        if si < boundaries.len() {
            for _ in 0..boundaries[si] {
                output.push(String::new());
            }
        }
    }
    Ok(output)
}