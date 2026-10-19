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
    displayed: usize,
    consumed: usize,
}

#[derive(Clone)]
struct Plan {
    cost: u64,
    marker_lines: usize,
    lines: Vec<Line>,
    consumed: Vec<usize>,
    raw_lines: Vec<String>,
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

fn better(a: &Plan, b: &Plan) -> bool {
    if a.cost != b.cost {
        return a.cost < b.cost;
    }
    if a.marker_lines != b.marker_lines {
        return a.marker_lines < b.marker_lines;
    }
    if a.lines.len() != b.lines.len() {
        return a.lines.len() < b.lines.len();
    }
    for (x, y) in a.consumed.iter().zip(&b.consumed) {
        if x != y {
            return x > y;
        }
    }
    for (x, y) in a.raw_lines.iter().zip(&b.raw_lines) {
        if x != y {
            return x < y;
        }
    }
    false
}

fn prepend(line: Line, tail: Plan, opts: &WrapOptions, marker: bool, raw: bool) -> Plan {
    let gap = opts.width - line.displayed;
    let line_cost = if tail.lines.is_empty() {
        (gap * gap / 4) as u64
    } else {
        (gap * gap) as u64
    };
    let extra = if marker {
        opts.hyphen_cost as u64
    } else if raw {
        (2 * opts.width + 1) as u64
    } else {
        0
    };
    let singleton_penalty = if !tail.lines.is_empty() && line.pieces.len() == 1 {
        3
    } else {
        0
    };

    let mut lines = Vec::with_capacity(tail.lines.len() + 1);
    lines.push(line.clone());
    lines.extend(tail.lines);

    let mut consumed = Vec::with_capacity(tail.consumed.len() + 1);
    consumed.push(line.consumed);
    consumed.extend(tail.consumed);

    let mut raw_lines = Vec::with_capacity(tail.raw_lines.len() + 1);
    raw_lines.push(line_string(&line));
    raw_lines.extend(tail.raw_lines);

    Plan {
        cost: tail.cost + line_cost + extra + singleton_penalty,
        marker_lines: tail.marker_lines + usize::from(marker),
        lines,
        consumed,
        raw_lines,
    }
}

fn solve_paragraph(words: &[Word], opts: &WrapOptions) -> Vec<String> {
    let mut memo: Vec<Vec<[Option<Plan>; 2]>> = words
        .iter()
        .map(|w| (0..=w.chars.len()).map(|_| [None, None]).collect())
        .collect();

    fn solve(
        wi: usize,
        off: usize,
        first: bool,
        words: &[Word],
        opts: &WrapOptions,
        memo: &mut [Vec<[Option<Plan>; 2]>],
    ) -> Plan {
        if wi == words.len() {
            return Plan {
                cost: 0,
                marker_lines: 0,
                lines: Vec::new(),
                consumed: Vec::new(),
                raw_lines: Vec::new(),
            };
        }

        let fi = usize::from(!first);
        if let Some(plan) = &memo[wi][off][fi] {
            return plan.clone();
        }

        let indent = if first {
            opts.first_indent
        } else {
            opts.rest_indent
        };
        let cap = opts.width - indent;
        let mut candidates = Vec::new();

        let mut pieces = Vec::<String>::new();
        let mut used = 0usize;
        let mut end_w = wi;
        let mut end_off = off;

        loop {
            let word = &words[end_w];
            let remaining = word.chars.len() - end_off;
            let prefix_cost = used + usize::from(!pieces.is_empty());

            if remaining + prefix_cost <= cap {
                let mut ps = pieces.clone();
                ps.push(word.chars[end_off..].iter().collect());
                let displayed_content = prefix_cost + remaining;
                let suffix = solve(end_w + 1, 0, false, words, opts, memo);
                let line = Line {
                    indent,
                    pieces: ps,
                    displayed: indent + displayed_content,
                    consumed: remaining,
                };
                candidates.push(prepend(line, suffix, opts, false, false));
            }

            for &marker_pos in word.markers.iter().filter(|&&m| m > end_off) {
                let prefix_len = marker_pos - end_off;
                if prefix_cost + prefix_len + 1 <= cap {
                    let mut ps = pieces.clone();
                    let mut text: String = word.chars[end_off..marker_pos].iter().collect();
                    text.push('-');
                    ps.push(text);

                    let suffix = solve(end_w, marker_pos, false, words, opts, memo);
                    let line = Line {
                        indent,
                        pieces: ps,
                        displayed: indent + prefix_cost + prefix_len + 1,
                        consumed: prefix_len,
                    };
                    candidates.push(prepend(line, suffix, opts, true, false));
                }
            }

            if pieces.is_empty() && remaining > cap {
                let marker_fits = word
                    .markers
                    .iter()
                    .any(|&m| m > end_off && m - end_off + 1 <= cap);

                if !marker_fits {
                    let text: String = word.chars[end_off..end_off + cap].iter().collect();
                    let suffix = solve(end_w, end_off + cap, false, words, opts, memo);
                    let line = Line {
                        indent,
                        pieces: vec![text],
                        displayed: opts.width,
                        consumed: cap,
                    };
                    candidates.push(prepend(line, suffix, opts, false, true));
                }
            }

            let next_w = end_w + 1;
            if next_w >= words.len() {
                break;
            }

            let new_used = prefix_cost + remaining;
            if new_used + 1 + words[next_w].chars.len() > cap {
                break;
            }

            pieces.push(word.chars[end_off..].iter().collect());
            used = new_used;
            end_w = next_w;
            end_off = 0;
        }

        let best = candidates
            .into_iter()
            .reduce(|a, b| if better(&a, &b) { a } else { b })
            .expect("raw splitting guarantees a legal continuation");

        memo[wi][off][fi] = Some(best.clone());
        best
    }

    let plan = solve(0, 0, true, words, opts, &mut memo);
    let line_count = plan.lines.len();
    let mut output = Vec::with_capacity(line_count);

    for (i, line) in plan.lines.iter().enumerate() {
        let mut rendered = " ".repeat(line.indent);

        if opts.justify && i + 1 < line_count && line.pieces.len() >= 2 {
            let gaps = line.pieces.len() - 1;
            let extra = opts.width - line.displayed;
            let each = extra / gaps;
            let rem = extra % gaps;
            let start = i % gaps;

            for (j, piece) in line.pieces.iter().enumerate() {
                if j > 0 {
                    let gap_index = j - 1;
                    let receives_extra =
                        (gap_index + gaps - start) % gaps < rem;
                    rendered.push_str(&" ".repeat(1 + each + usize::from(receives_extra)));
                }
                rendered.push_str(piece);
            }
        } else {
            for (j, piece) in line.pieces.iter().enumerate() {
                if j > 0 {
                    rendered.push(' ');
                }
                rendered.push_str(piece);
            }
        }

        output.push(rendered);
    }

    output
}

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{00A0}')
}

fn append_segment(segment: &[char], options: &WrapOptions, out: &mut Vec<String>) {
    let mut words = Vec::<Word>::new();
    let mut chars = Vec::<char>::new();
    let mut markers = Vec::<usize>::new();

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

    if !words.is_empty() {
        out.extend(solve_paragraph(&words, options));
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
            let valid_neighbor = |j: isize| {
                if j < 0 || j as usize >= chars.len() {
                    return false;
                }
                let c = chars[j as usize].1;
                c != '~' && !is_separator(c)
            };

            if !valid_neighbor(i as isize - 1) || !valid_neighbor(i as isize + 1) {
                return Err(WrapError::InvalidMarker { index: chars[i].0 });
            }
        }
    }

    let mut normalized = Vec::<(char, usize)>::new();
    let mut i = 0;
    while i < chars.len() {
        let (index, c) = chars[i];
        if c == '\r' && i + 1 < chars.len() && chars[i + 1].1 == '\n' {
            normalized.push(('\n', index));
            i += 2;
        } else {
            normalized.push((if c == '\r' { '\n' } else { c }, index));
            i += 1;
        }
    }

    let mut result = Vec::new();
    let mut segment = Vec::<char>::new();
    let mut j = 0;

    while j < normalized.len() {
        if normalized[j].0 == '\n' {
            let mut k = j;
            while k < normalized.len() && normalized[k].0 == '\n' {
                k += 1;
            }

            if k - j >= 2 {
                append_segment(&segment, options, &mut result);
                segment.clear();
                for _ in 1..(k - j) {
                    result.push(String::new());
                }
                j = k;
                continue;
            }
        }

        segment.push(normalized[j].0);
        j += 1;
    }

    append_segment(&segment, options, &mut result);
    Ok(result)
}