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
    marker_end: bool,
    raw: bool,
    displayed: usize,
}

#[derive(Clone)]
struct Solution {
    cost: u64,
    marker_lines: usize,
    lines: Vec<Line>,
    consumed_counts: Vec<usize>,
    raw_strings: Vec<String>,
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
    if a.consumed_counts != b.consumed_counts {
        return a.consumed_counts > b.consumed_counts;
    }
    a.raw_strings < b.raw_strings
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

fn solve_paragraph(words: &[Word], options: &WrapOptions) -> Vec<Line> {
    use std::collections::HashMap;

    fn rec(
        words: &[Word],
        opts: &WrapOptions,
        wi: usize,
        off: usize,
        first: bool,
        memo: &mut HashMap<(usize, usize, bool), Option<Solution>>,
    ) -> Option<Solution> {
        if wi >= words.len() {
            return Some(Solution {
                cost: 0,
                marker_lines: 0,
                lines: Vec::new(),
                consumed_counts: Vec::new(),
                raw_strings: Vec::new(),
            });
        }

        if let Some(cached) = memo.get(&(wi, off, first)) {
            return cached.clone();
        }

        let indent = if first {
            opts.first_indent
        } else {
            opts.rest_indent
        };
        let cap = opts.width - indent;
        let mut best: Option<Solution> = None;
        let mut candidates: Vec<(usize, usize, bool, bool, Vec<String>, usize, usize)> = Vec::new();

        let mut used = 0usize;
        let mut cur_word = wi;
        let mut cur_off = off;
        let mut pieces: Vec<String> = Vec::new();
        let mut consumed = 0usize;

        loop {
            if cur_word >= words.len() {
                break;
            }

            let word = &words[cur_word];
            let remain = word.chars.len() - cur_off;
            let space = usize::from(!pieces.is_empty());

            if used + space + remain <= cap {
                let piece: String = word.chars[cur_off..].iter().collect();
                if !pieces.is_empty() {
                    used += 1;
                }
                used += remain;
                consumed += remain;
                pieces.push(piece);
                candidates.push((
                    cur_word + 1,
                    0,
                    false,
                    false,
                    pieces.clone(),
                    used,
                    consumed,
                ));
                cur_word += 1;
                cur_off = 0;
                continue;
            }

            let prefix_capacity = cap.saturating_sub(used + space);
            if prefix_capacity > 0 {
                for &mark in &word.markers {
                    if mark > cur_off && mark < word.chars.len() {
                        let len = mark - cur_off;
                        if len + 1 <= prefix_capacity {
                            let mut ps = pieces.clone();
                            let mut prefix: String = word.chars[cur_off..mark].iter().collect();
                            prefix.push('-');
                            ps.push(prefix);
                            candidates.push((
                                cur_word,
                                mark,
                                true,
                                false,
                                ps,
                                used + space + len + 1,
                                consumed + len,
                            ));
                        }
                    }
                }
            }

            if pieces.is_empty() && remain > cap {
                let any_marker_fits = word.markers.iter().any(|&mark| {
                    mark > cur_off
                        && mark < word.chars.len()
                        && mark - cur_off + 1 <= cap
                });
                if !any_marker_fits {
                    let piece: String = word.chars[cur_off..cur_off + cap].iter().collect();
                    candidates.push((
                        cur_word,
                        cur_off + cap,
                        false,
                        true,
                        vec![piece],
                        cap,
                        cap,
                    ));
                }
            }
            break;
        }

        for (nwi, noff, marker, raw, pcs, displayed_content, consumed_count) in candidates {
            let is_final = nwi >= words.len();
            let gap = opts.width - (indent + displayed_content);
            let mut line_cost = (gap * gap) as u64;
            if is_final {
                line_cost /= 4;
            }
            if marker {
                line_cost += opts.hyphen_cost as u64;
            }
            if raw {
                line_cost += (2 * opts.width + 1) as u64;
            }
            if !is_final && pcs.len() == 1 {
                line_cost += 3;
            }

            if let Some(mut tail) = rec(words, opts, nwi, noff, false, memo) {
                let line = Line {
                    indent,
                    pieces: pcs,
                    marker_end: marker,
                    raw,
                    displayed: indent + displayed_content,
                };
                let raw_s = line_string(&line);
                tail.cost += line_cost;
                tail.marker_lines += usize::from(marker);
                tail.lines.insert(0, line);
                tail.consumed_counts.insert(0, consumed_count);
                tail.raw_strings.insert(0, raw_s);

                if best.as_ref().map_or(true, |b| better(&tail, b)) {
                    best = Some(tail);
                }
            }
        }

        memo.insert((wi, off, first), best.clone());
        best
    }

    rec(words, options, 0, 0, true, &mut HashMap::new())
        .map(|solution| solution.lines)
        .unwrap_or_default()
}

fn is_separator(c: char) -> bool {
    c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\u{00a0}'
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

fn render_line(line: &Line, options: &WrapOptions, paragraph_line_index: usize) -> String {
    let mut s = " ".repeat(line.indent);
    let gaps = line.pieces.len().saturating_sub(1);
    let justify = options.justify
        && !line.raw
        && !line.marker_end
        && gaps > 0
        && paragraph_line_index + 1 < usize::MAX;

    if justify {
        let extra = options.width.saturating_sub(line.displayed);
        let each = extra / gaps;
        let rem = extra % gaps;
        let start_gap = paragraph_line_index % gaps;

        for (i, piece) in line.pieces.iter().enumerate() {
            if i > 0 {
                let gap_index = i - 1;
                let bonus = usize::from((gap_index + gaps - start_gap) % gaps < rem);
                s.push_str(&" ".repeat(1 + each + bonus));
            }
            s.push_str(piece);
        }
    } else {
        for (i, piece) in line.pieces.iter().enumerate() {
            if i > 0 {
                s.push(' ');
            }
            s.push_str(piece);
        }
    }
    s
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
        if c.is_control() && c != '\t' && c != '\r' && c != '\n' {
            return Err(WrapError::InvalidControl { index });
        }
    }

    for i in 0..chars.len() {
        if chars[i].1 == '~' {
            let left_ok = i > 0 && chars[i - 1].1 != '~' && !is_separator(chars[i - 1].1);
            let right_ok =
                i + 1 < chars.len() && chars[i + 1].1 != '~' && !is_separator(chars[i + 1].1);
            if !left_ok || !right_ok {
                return Err(WrapError::InvalidMarker { index: chars[i].0 });
            }
        }
    }

    let mut normalized = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i].1;
        if c == '\r' {
            normalized.push('\n');
            i += if i + 1 < chars.len() && chars[i + 1].1 == '\n' {
                2
            } else {
                1
            };
        } else {
            normalized.push(c);
            i += 1;
        }
    }

    let mut out = Vec::new();
    let mut start = 0usize;
    let mut j = 0usize;

    while j < normalized.len() {
        if normalized[j] == '\n' {
            let run_start = j;
            while j < normalized.len() && normalized[j] == '\n' {
                j += 1;
            }
            let run_len = j - run_start;
            if run_len >= 2 {
                let words = parse_words(&normalized[start..run_start]);
                let lines = solve_paragraph(&words, options);
                let line_count = lines.len();
                for (line_index, line) in lines.iter().enumerate() {
                    let mut rendered = render_line(line, options, line_index);
                    if options.justify
                        && !line.raw
                        && !line.marker_end
                        && line.pieces.len() >= 2
                        && line_index + 1 == line_count
                    {
                        rendered = line_string(line);
                    }
                    out.push(rendered);
                }
                for _ in 1..run_len {
                    out.push(String::new());
                }
                start = j;
            }
        } else {
            j += 1;
        }
    }

    let words = parse_words(&normalized[start..]);
    let lines = solve_paragraph(&words, options);
    let line_count = lines.len();
    for (line_index, line) in lines.iter().enumerate() {
        let mut rendered = render_line(line, options, line_index);
        if options.justify
            && !line.raw
            && !line.marker_end
            && line.pieces.len() >= 2
            && line_index + 1 == line_count
        {
            rendered = line_string(line);
        }
        out.push(rendered);
    }

    Ok(out)
}

fn main() {}