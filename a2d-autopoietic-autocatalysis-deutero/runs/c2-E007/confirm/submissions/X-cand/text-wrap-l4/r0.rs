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
}

#[derive(Clone)]
struct Solution {
    cost: u64,
    marker_lines: usize,
    lines: Vec<Line>,
    consumed: Vec<usize>,
}

impl Solution {
    fn empty() -> Self {
        Self {
            cost: 0,
            marker_lines: 0,
            lines: Vec::new(),
            consumed: Vec::new(),
        }
    }
}

fn render_prejustified(line: &Line) -> String {
    let mut out = " ".repeat(line.indent);
    for (i, piece) in line.pieces.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(piece);
    }
    out
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
    if a.consumed != b.consumed {
        return a.consumed > b.consumed;
    }
    let a_strings: Vec<String> = a.lines.iter().map(render_prejustified).collect();
    let b_strings: Vec<String> = b.lines.iter().map(render_prejustified).collect();
    a_strings < b_strings
}

fn make_line(
    indent: usize,
    pieces: &[String],
    consumed: usize,
    marker_ended: bool,
    raw: bool,
) -> Line {
    Line {
        indent,
        pieces: pieces.to_vec(),
        consumed,
        marker_ended,
        raw,
    }
}

fn extend_solution(
    suffix: &Solution,
    line: Line,
    width: usize,
    used: usize,
    options: &WrapOptions,
) -> Solution {
    let gap = width.saturating_sub(used) as u64;
    let final_line = suffix.lines.is_empty();
    let mut cost = if final_line {
        gap * gap / 4
    } else {
        gap * gap
    };

    if line.marker_ended {
        cost += options.hyphen_cost as u64;
    }
    if line.raw {
        cost += (2 * width + 1) as u64;
    }
    if !final_line && line.pieces.len() == 1 {
        cost += 3;
    }

    let mut result = suffix.clone();
    result.cost += cost;
    if line.marker_ended {
        result.marker_lines += 1;
    }
    result.lines.insert(0, line.clone());
    result.consumed.insert(0, line.consumed);
    result
}

fn solve(words: &[Word], options: &WrapOptions) -> Solution {
    let mut best: Vec<Vec<Option<Solution>>> = words
        .iter()
        .map(|w| vec![None; w.chars.len() + 1])
        .collect();

    let mut states: Vec<(usize, usize)> = Vec::new();
    for (wi, word) in words.iter().enumerate() {
        for off in 0..=word.chars.len() {
            states.push((wi, off));
        }
    }

    for &(wi, off) in states.iter().rev() {
        let first = off == 0 && wi == 0;
        let indent = if first {
            options.first_indent
        } else {
            options.rest_indent
        };
        let capacity = options.width - indent;
        let mut candidates = Vec::new();

        let mut pieces: Vec<String> = Vec::new();
        let mut displayed = 0usize;
        let mut consumed_here = 0usize;
        let mut cursor_word = wi;
        let mut cursor_off = off;

        loop {
            if cursor_word >= words.len() {
                break;
            }

            let word = &words[cursor_word];
            let remaining = word.chars.len() - cursor_off;
            let available =
                capacity.saturating_sub(displayed + usize::from(!pieces.is_empty()));

            // A marker endpoint is legal even when the complete suffix fits.
            for &marker in &word.markers {
                if marker <= cursor_off || marker >= word.chars.len() {
                    continue;
                }
                let prefix_len = marker - cursor_off;
                if prefix_len + 1 > available {
                    continue;
                }

                let mut marker_pieces = pieces.clone();
                let mut prefix: String = word.chars[cursor_off..marker].iter().collect();
                prefix.push('-');
                marker_pieces.push(prefix);

                if let Some(suffix) = best[cursor_word][marker].clone() {
                    let consumed = consumed_here + prefix_len;
                    let line = make_line(indent, &marker_pieces, consumed, true, false);
                    let used = indent
                        + marker_pieces.iter().map(|s| s.chars().count()).sum::<usize>()
                        + marker_pieces.len().saturating_sub(1);
                    candidates.push(extend_solution(
                        &suffix,
                        line,
                        options.width,
                        used,
                        options,
                    ));
                }
            }

            // Completed-word endpoint.
            if remaining <= available {
                let text: String = word.chars[cursor_off..].iter().collect();
                let add = remaining + usize::from(!pieces.is_empty());
                pieces.push(text);
                displayed += add;
                consumed_here += remaining;

                let suffix = if cursor_word + 1 == words.len() {
                    Some(Solution::empty())
                } else {
                    best[cursor_word + 1][0].clone()
                };
                if let Some(suffix) = suffix {
                    let line = make_line(indent, &pieces, consumed_here, false, false);
                    candidates.push(extend_solution(
                        &suffix,
                        line,
                        options.width,
                        indent + displayed,
                        options,
                    ));
                }

                cursor_word += 1;
                cursor_off = 0;
                if cursor_word >= words.len() || displayed + 1 >= capacity {
                    break;
                }
                continue;
            }

            // Raw split is legal only on an empty line and only when no marker
            // prefix with a hyphen fits in the available capacity.
            if pieces.is_empty() && remaining > capacity {
                let any_marker_fits = word.markers.iter().any(|&marker| {
                    marker > cursor_off
                        && marker < word.chars.len()
                        && marker - cursor_off + 1 <= capacity
                });
                if !any_marker_fits && capacity > 0 {
                    let next_off = cursor_off + capacity;
                    if let Some(suffix) = best[cursor_word][next_off].clone() {
                        let raw_text: String =
                            word.chars[cursor_off..next_off].iter().collect();
                        let line = make_line(indent, &[raw_text], capacity, false, true);
                        candidates.push(extend_solution(
                            &suffix,
                            line,
                            options.width,
                            options.width,
                            options,
                        ));
                    }
                }
            }
            break;
        }

        if let Some(solution) = candidates.into_iter().reduce(|a, b| {
            if better(&a, &b) {
                a
            } else {
                b
            }
        }) {
            best[wi][off] = Some(solution);
        }
    }

    best[0][0].clone().unwrap_or_else(Solution::empty)
}

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{00a0}')
}

fn parse_words(chars: &[char]) -> Vec<Word> {
    let mut words = Vec::new();
    let mut spelling = Vec::new();
    let mut markers = Vec::new();

    let flush = |words: &mut Vec<Word>,
                 spelling: &mut Vec<char>,
                 markers: &mut Vec<usize>| {
        if !spelling.is_empty() {
            words.push(Word {
                chars: std::mem::take(spelling),
                markers: std::mem::take(markers),
            });
        }
    };

    for &c in chars {
        if is_separator(c) {
            flush(&mut words, &mut spelling, &mut markers);
        } else if c == '~' {
            markers.push(spelling.len());
        } else {
            spelling.push(c);
        }
    }
    flush(&mut words, &mut spelling, &mut markers);
    words
}

fn render_lines(solution: &Solution, options: &WrapOptions) -> Vec<String> {
    let mut result = Vec::new();
    for (line_index, line) in solution.lines.iter().enumerate() {
        let mut rendered = render_prejustified(line);
        let is_final = line_index + 1 == solution.lines.len();
        let gaps = line.pieces.len().saturating_sub(1);

        if options.justify && !is_final && gaps > 0 {
            let used = rendered.chars().count();
            let extra = options.width - used;
            let each = extra / gaps;
            let remainder = extra % gaps;
            let start = line_index % gaps;
            let mut rebuilt = " ".repeat(line.indent);
            for (i, piece) in line.pieces.iter().enumerate() {
                if i > 0 {
                    let gap_index = i - 1;
                    let bonus =
                        usize::from((gap_index + gaps - start) % gaps < remainder);
                    rebuilt.push_str(&" ".repeat(1 + each + bonus));
                }
                rebuilt.push_str(piece);
            }
            rendered = rebuilt;
        }
        result.push(rendered);
    }
    result
}

fn append_paragraph(
    chars: &[char],
    options: &WrapOptions,
    output: &mut Vec<String>,
) {
    let words = parse_words(chars);
    if !words.is_empty() {
        let solution = solve(&words, options);
        output.extend(render_lines(&solution, options));
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
        let invalid = i == 0
            || i + 1 == original.len()
            || original[i - 1].1 == '~'
            || original[i + 1].1 == '~'
            || is_separator(original[i - 1].1)
            || is_separator(original[i + 1].1);
        if invalid {
            return Err(WrapError::InvalidMarker {
                index: original[i].0,
            });
        }
    }

    let mut normalized = Vec::new();
    let mut i = 0;
    while i < original.len() {
        let c = original[i].1;
        if c == '\r' && i + 1 < original.len() && original[i + 1].1 == '\n' {
            normalized.push('\n');
            i += 2;
        } else {
            normalized.push(if c == '\r' { '\n' } else { c });
            i += 1;
        }
    }

    let mut output = Vec::new();
    let mut current = Vec::new();
    let mut p = 0;
    while p < normalized.len() {
        if normalized[p] == '\n' {
            let start = p;
            while p < normalized.len() && normalized[p] == '\n' {
                p += 1;
            }
            let run = p - start;

            if run >= 2 {
                append_paragraph(&current, options, &mut output);
                current.clear();
                output.extend((0..run - 1).map(|_| String::new()));
            } else {
                current.push('\n');
            }
        } else {
            current.push(normalized[p]);
            p += 1;
        }
    }
    append_paragraph(&current, options, &mut output);

    Ok(output)
}