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
    marker_ended: bool,
    raw: bool,
}

#[derive(Clone)]
struct Candidate {
    cost: u64,
    marker_lines: usize,
    lines: Vec<Line>,
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

    for (x, y) in a.lines.iter().zip(&b.lines) {
        if x.consumed != y.consumed {
            return x.consumed > y.consumed;
        }
    }

    let a_strings: Vec<String> = a.lines.iter().map(pre_string).collect();
    let b_strings: Vec<String> = b.lines.iter().map(pre_string).collect();
    a_strings < b_strings
}

fn pre_string(line: &Line) -> String {
    let mut s = " ".repeat(line.indent);
    for (i, piece) in line.pieces.iter().enumerate() {
        if i != 0 {
            s.push(' ');
        }
        s.push_str(piece);
    }
    s
}

fn optimize(words: &[Word], options: &WrapOptions) -> Vec<Line> {
    fn solve(
        wi: usize,
        offset: usize,
        first: bool,
        words: &[Word],
        options: &WrapOptions,
        memo: &mut Vec<Vec<[Option<Candidate>; 2]>>,
    ) -> Candidate {
        if wi == words.len() {
            return Candidate {
                cost: 0,
                marker_lines: 0,
                lines: Vec::new(),
            };
        }

        let fi = usize::from(first);
        if let Some(c) = &memo[wi][offset][fi] {
            return c.clone();
        }

        let indent = if first {
            options.first_indent
        } else {
            options.rest_indent
        };
        let cap = options.width - indent;
        let mut best: Option<Candidate> = None;

        let mut pieces = Vec::<String>::new();
        let mut shown = indent;
        let mut consumed = 0usize;
        let mut cur_wi = wi;
        let mut cur_off = offset;

        while cur_wi < words.len() {
            let word = &words[cur_wi];
            let remaining = word.chars.len() - cur_off;
            let gap = usize::from(!pieces.is_empty());
            if shown + gap >= options.width {
                break;
            }
            let available = options.width - shown - gap;
            let take = remaining.min(available);
            if take == 0 {
                break;
            }

            let prefix: String = word.chars[cur_off..cur_off + take].iter().collect();
            let new_shown = shown + gap + take;

            if take == remaining {
                pieces.push(prefix);
                let next = solve(cur_wi + 1, 0, false, words, options, memo);
                let line = Line {
                    indent,
                    pieces: pieces.clone(),
                    displayed: new_shown,
                    consumed: consumed + remaining,
                    marker_ended: false,
                    raw: false,
                };
                let candidate = prepend(line, next, options, false, false);
                keep_better(&mut best, candidate);

                shown = new_shown;
                consumed += remaining;
                cur_wi += 1;
                cur_off = 0;
                continue;
            }

            let end_offset = cur_off + take;
            for &marker in &word.markers {
                if marker > cur_off && marker < word.chars.len() && marker <= end_offset {
                    let plen = marker - cur_off;
                    if plen + 1 <= available {
                        let mut marker_piece: String =
                            word.chars[cur_off..marker].iter().collect();
                        marker_piece.push('-');
                        let mut marker_pieces = pieces.clone();
                        marker_pieces.push(marker_piece);
                        let next = solve(cur_wi, marker, false, words, options, memo);
                        let line = Line {
                            indent,
                            pieces: marker_pieces,
                            displayed: shown + gap + plen + 1,
                            consumed: consumed + plen,
                            marker_ended: true,
                            raw: false,
                        };
                        keep_better(
                            &mut best,
                            prepend(line, next, options, true, false),
                        );
                    }
                }
            }

            if pieces.is_empty() && remaining > cap {
                let any_marker_fits = word.markers.iter().any(|&m| {
                    m > cur_off
                        && m < word.chars.len()
                        && m - cur_off + 1 <= cap
                });
                if !any_marker_fits {
                    let raw_piece: String =
                        word.chars[cur_off..cur_off + cap].iter().collect();
                    let line = Line {
                        indent,
                        pieces: vec![raw_piece],
                        displayed: indent + cap,
                        consumed: cap,
                        marker_ended: false,
                        raw: true,
                    };
                    let next = solve(cur_wi, cur_off + cap, false, words, options, memo);
                    keep_better(&mut best, prepend(line, next, options, false, true));
                }
            }
            break;
        }

        let result = best.unwrap_or(Candidate {
            cost: u64::MAX / 4,
            marker_lines: usize::MAX / 4,
            lines: Vec::new(),
        });
        memo[wi][offset][fi] = Some(result.clone());
        result
    }

    let mut memo: Vec<Vec<[Option<Candidate>; 2]>> = words
        .iter()
        .map(|w| {
            (0..=w.chars.len())
                .map(|_| [None, None])
                .collect::<Vec<_>>()
        })
        .collect();

    solve(0, 0, true, words, options, &mut memo).lines
}

fn prepend(
    line: Line,
    mut tail: Candidate,
    options: &WrapOptions,
    marker: bool,
    raw: bool,
) -> Candidate {
    let gap = options.width - line.displayed;
    let is_final = tail.lines.is_empty();
    let mut cost = (gap as u64) * (gap as u64);
    if is_final {
        cost /= 4;
    }
    if marker {
        cost += options.hyphen_cost as u64;
        tail.marker_lines += 1;
    }
    if raw {
        cost += (2 * options.width + 1) as u64;
    }
    if !is_final && line.pieces.len() == 1 {
        cost += 3;
    }
    tail.cost += cost;
    tail.lines.insert(0, line);
    tail
}

fn keep_better(best: &mut Option<Candidate>, candidate: Candidate) {
    if best.as_ref().map_or(true, |old| better(&candidate, old)) {
        *best = Some(candidate);
    }
}

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{00a0}')
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
    }

    let mut normalized = Vec::<char>::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i].1;
        if c == '\r' {
            normalized.push('\n');
            if i + 1 < chars.len() && chars[i + 1].1 == '\n' {
                i += 1;
            }
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
            let count = p - start;
            if count >= 2 {
                layout_segment(&segment, options, &mut output);
                segment.clear();
                output.extend(std::iter::repeat(String::new()).take(count - 1));
            } else {
                segment.push('\n');
            }
        } else {
            segment.push(normalized[p]);
            p += 1;
        }
    }
    layout_segment(&segment, options, &mut output);
    Ok(output)
}

fn layout_segment(segment: &[char], options: &WrapOptions, output: &mut Vec<String>) {
    let mut words = Vec::<Word>::new();
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

    if words.is_empty() {
        return;
    }

    let mut lines = optimize(&words, options);
    let count = lines.len();
    for (line_index, line) in lines.iter_mut().enumerate() {
        let mut rendered = " ".repeat(line.indent);
        let gaps = line.pieces.len().saturating_sub(1);
        let extra = if options.justify && line_index + 1 < count && gaps > 0 {
            options.width.saturating_sub(line.displayed)
        } else {
            0
        };
        let base = if gaps > 0 { extra / gaps } else { 0 };
        let remainder = if gaps > 0 { extra % gaps } else { 0 };
        let start_gap = if gaps > 0 { line_index % gaps } else { 0 };

        for (idx, piece) in line.pieces.iter().enumerate() {
            if idx > 0 {
                let gap_idx = idx - 1;
                let bonus = if gaps > 0 && (gap_idx + gaps - start_gap) % gaps < remainder {
                    1
                } else {
                    0
                };
                rendered.push_str(&" ".repeat(1 + base + bonus));
            }
            rendered.push_str(piece);
        }
        output.push(rendered);
    }
}

fn main() {}