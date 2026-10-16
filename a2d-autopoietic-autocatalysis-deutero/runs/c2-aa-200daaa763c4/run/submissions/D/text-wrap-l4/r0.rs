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
    spelling_count: usize,
    marker_ended: bool,
    raw_split: bool,
}

#[derive(Clone)]
struct Solution {
    cost: u64,
    marker_count: usize,
    lines: Vec<Line>,
}

#[derive(Clone)]
struct Candidate {
    next_word: usize,
    next_offset: usize,
    line: Line,
    line_cost: u64,
}

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{00a0}')
}

fn checked_indexed_chars(input: &str) -> Vec<(usize, char)> {
    input.char_indices().collect()
}

fn square(n: usize) -> u64 {
    (n as u64) * (n as u64)
}

fn compare_solutions(a: &Solution, b: &Solution) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    match a.cost.cmp(&b.cost) {
        Ordering::Equal => {}
        other => return other,
    }
    match a.marker_count.cmp(&b.marker_count) {
        Ordering::Equal => {}
        other => return other,
    }
    match a.lines.len().cmp(&b.lines.len()) {
        Ordering::Equal => {}
        other => return other,
    }

    let av: Vec<usize> = a.lines.iter().map(|line| line.spelling_count).collect();
    let bv: Vec<usize> = b.lines.iter().map(|line| line.spelling_count).collect();
    match bv.cmp(&av) {
        Ordering::Equal => {}
        other => return other,
    }

    let astr: Vec<String> = a.lines.iter().map(render_prejustify).collect();
    let bstr: Vec<String> = b.lines.iter().map(render_prejustify).collect();
    astr.cmp(&bstr)
}

fn render_prejustify(line: &Line) -> String {
    let mut out = " ".repeat(line.indent);
    for (i, piece) in line.pieces.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(piece);
    }
    out
}

fn line_gap(width: usize, indent: usize, pieces: &[String]) -> usize {
    let content: usize = pieces.iter().map(|piece| piece.chars().count()).sum();
    let gaps = pieces.len().saturating_sub(1);
    width - indent - content - gaps
}

fn solve_paragraph(words: &[Word], options: &WrapOptions) -> Vec<Line> {
    use std::collections::HashMap;

    fn recurse(
        wi: usize,
        offset: usize,
        first: bool,
        words: &[Word],
        options: &WrapOptions,
        memo: &mut HashMap<(usize, usize, bool), Solution>,
    ) -> Solution {
        if wi >= words.len() {
            return Solution {
                cost: 0,
                marker_count: 0,
                lines: Vec::new(),
            };
        }
        if let Some(found) = memo.get(&(wi, offset, first)) {
            return found.clone();
        }

        let indent = if first {
            options.first_indent
        } else {
            options.rest_indent
        };
        let capacity = options.width - indent;
        let mut candidates = Vec::new();
        let mut pieces = Vec::new();
        let mut total_spelling = 0usize;
        let mut used = 0usize;
        let mut current_word = wi;
        let mut current_offset = offset;
        let mut at_line_start = true;

        loop {
            if current_word >= words.len() {
                if !pieces.is_empty() {
                    let gap = options.width - (indent + used);
                    let line = Line {
                        indent,
                        pieces: pieces.clone(),
                        spelling_count: total_spelling,
                        marker_ended: false,
                        raw_split: false,
                    };
                    candidates.push(Candidate {
                        next_word: words.len(),
                        next_offset: 0,
                        line,
                        line_cost: (gap as u64) * (gap as u64) / 4
                            + if pieces.len() == 1 { 0 } else { 0 },
                    });
                }
                break;
            }

            let word = &words[current_word];
            let start = current_offset;
            let remaining = word.chars.len() - start;
            let leading_gap = if pieces.is_empty() { 0 } else { 1 };
            let available = capacity.saturating_sub(used + leading_gap);

            // Add the whole remaining suffix if it fits, then continue to later words.
            if remaining <= available {
                let suffix: String = word.chars[start..].iter().collect();
                pieces.push(suffix);
                used += leading_gap + remaining;
                total_spelling += remaining;
                current_word += 1;
                current_offset = 0;
                at_line_start = false;

                let gap = options.width - (indent + used);
                let line = Line {
                    indent,
                    pieces: pieces.clone(),
                    spelling_count: total_spelling,
                    marker_ended: false,
                    raw_split: false,
                };
                candidates.push(Candidate {
                    next_word: current_word,
                    next_offset: 0,
                    line,
                    line_cost: (gap as u64) * (gap as u64),
                });
                continue;
            }

            // Marker breaks in this word.
            let mut marker_fit = false;
            for &mark in word.markers.iter().filter(|&&m| m > start && m < word.chars.len()) {
                let prefix_len = mark - start;
                if prefix_len == 0 || prefix_len + 1 > available {
                    continue;
                }
                marker_fit = true;
                let mut prefix: String = word.chars[start..mark].iter().collect();
                prefix.push('-');
                let mut marker_pieces = pieces.clone();
                marker_pieces.push(prefix);
                let marker_used = used + leading_gap + prefix_len + 1;
                let gap = options.width - (indent + marker_used);
                let line = Line {
                    indent,
                    pieces: marker_pieces,
                    spelling_count: total_spelling + prefix_len,
                    marker_ended: true,
                    raw_split: false,
                };
                candidates.push(Candidate {
                    next_word: current_word,
                    next_offset: mark,
                    line,
                    line_cost: (gap as u64) * (gap as u64) + options.hyphen_cost as u64,
                });
            }

            // Raw split is allowed only on an otherwise empty line, and only if
            // no marker prefix plus hyphen fits in the entire capacity.
            if pieces.is_empty() && remaining > capacity {
                let any_marker_fits = word
                    .markers
                    .iter()
                    .filter(|&&m| m > start && m < word.chars.len())
                    .any(|&m| {
                        let prefix_len = m - start;
                        prefix_len > 0 && prefix_len + 1 <= capacity
                    });
                if !any_marker_fits {
                    let take = capacity;
                    let prefix: String = word.chars[start..start + take].iter().collect();
                    let raw_pieces = vec![prefix];
                    let gap = options.width - (indent + take);
                    let line = Line {
                        indent,
                        pieces: raw_pieces,
                        spelling_count: take,
                        marker_ended: false,
                        raw_split: true,
                    };
                    candidates.push(Candidate {
                        next_word: current_word,
                        next_offset: start + take,
                        line,
                        line_cost: (gap as u64) * (gap as u64) + (2 * options.width + 1) as u64,
                    });
                }
            }

            break;
        }

        let mut best: Option<Solution> = None;
        for candidate in candidates {
            let final_line = candidate.next_word >= words.len();
            let mut own_cost = candidate.line_cost;
            if !final_line && candidate.line.pieces.len() == 1 {
                own_cost += 3;
            }
            // A paragraph's final line uses quarter-gap cost, including lines
            // that complete the final word.
            if final_line {
                let gap = line_gap(options.width, indent, &candidate.line.pieces);
                own_cost = (gap as u64) * (gap as u64) / 4;
                if candidate.line.marker_ended {
                    own_cost += options.hyphen_cost as u64;
                }
                if candidate.line.raw_split {
                    own_cost += (2 * options.width + 1) as u64;
                }
            }

            let tail = if final_line {
                Solution {
                    cost: 0,
                    marker_count: 0,
                    lines: Vec::new(),
                }
            } else {
                recurse(
                    candidate.next_word,
                    candidate.next_offset,
                    false,
                    words,
                    options,
                    memo,
                )
            };

            let mut lines = vec![candidate.line.clone()];
            lines.extend(tail.lines.clone());
            let proposal = Solution {
                cost: own_cost + tail.cost,
                marker_count: usize::from(candidate.line.marker_ended) + tail.marker_count,
                lines,
            };
            if best
                .as_ref()
                .map_or(true, |current| compare_solutions(&proposal, current).is_lt())
            {
                best = Some(proposal);
            }
        }

        let result = best.unwrap_or_else(|| Solution {
            cost: u64::MAX / 4,
            marker_count: usize::MAX / 4,
            lines: Vec::new(),
        });
        memo.insert((wi, offset, first), result.clone());
        result
    }

    let mut memo = HashMap::new();
    recurse(0, 0, true, words, options, &mut memo).lines
}

fn parse_segment(chars: &[char]) -> Vec<Word> {
    let mut words = Vec::new();
    let mut spelling = Vec::new();
    let mut markers = Vec::new();

    let flush = |words: &mut Vec<Word>, spelling: &mut Vec<char>, markers: &mut Vec<usize>| {
        if !spelling.is_empty() {
            words.push(Word {
                chars: std::mem::take(spelling),
                markers: std::mem::take(markers),
            });
        } else {
            markers.clear();
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

fn render_lines(lines: &[Line], options: &WrapOptions) -> Vec<String> {
    let mut result = Vec::with_capacity(lines.len());
    for (line_index, line) in lines.iter().enumerate() {
        let mut out = " ".repeat(line.indent);
        let gap_count = line.pieces.len().saturating_sub(1);
        let extra = if options.justify
            && line_index + 1 < lines.len()
            && gap_count > 0
        {
            line_gap(options.width, line.indent, &line.pieces)
        } else {
            0
        };

        let base_extra = if gap_count > 0 { extra / gap_count } else { 0 };
        let remainder = if gap_count > 0 { extra % gap_count } else { 0 };

        for (i, piece) in line.pieces.iter().enumerate() {
            if i > 0 {
                out.push(' ');
                if options.justify && line_index + 1 < lines.len() {
                    let gap = i - 1;
                    let start = line_index % gap_count;
                    let gets_one = (gap + gap_count - start) % gap_count < remainder;
                    out.push_str(&" ".repeat(base_extra + usize::from(gets_one)));
                }
            }
            out.push_str(piece);
        }
        result.push(out);
    }
    result
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

    let indexed = checked_indexed_chars(input);
    if indexed.len() > 512 {
        return Err(WrapError::InputTooLong {
            chars: indexed.len(),
        });
    }

    for &(index, c) in &indexed {
        if c.is_control() && !matches!(c, '\t' | '\r' | '\n') {
            return Err(WrapError::InvalidControl { index });
        }
    }

    for i in 0..indexed.len() {
        if indexed[i].1 != '~' {
            continue;
        }
        let valid_neighbor = |j: isize| -> bool {
            if j < 0 || j as usize >= indexed.len() {
                return false;
            }
            let c = indexed[j as usize].1;
            c != '~' && !is_separator(c)
        };
        if !valid_neighbor(i as isize - 1) || !valid_neighbor(i as isize + 1) {
            return Err(WrapError::InvalidMarker {
                index: indexed[i].0,
            });
        }
    }

    let mut normalized = Vec::new();
    let mut i = 0;
    while i < indexed.len() {
        let c = indexed[i].1;
        if c == '\r' {
            if i + 1 < indexed.len() && indexed[i + 1].1 == '\n' {
                normalized.push('\n');
                i += 2;
            } else {
                normalized.push('\n');
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

    let mut output = Vec::new();
    let mut segment = Vec::new();
    let mut cursor = 0;
    while cursor < normalized.len() {
        if normalized[cursor] == '\n' {
            let start = cursor;
            while cursor < normalized.len() && normalized[cursor] == '\n' {
                cursor += 1;
            }
            let run = cursor - start;
            if run >= 2 {
                let words = parse_segment(&segment);
                let lines = solve_paragraph(&words, options);
                output.extend(render_lines(&lines, options));
                segment.clear();
                output.extend(std::iter::repeat(String::new()).take(run - 1));
            } else {
                segment.push('\n');
            }
        } else {
            segment.push(normalized[cursor]);
            cursor += 1;
        }
    }
    let words = parse_segment(&segment);
    let lines = solve_paragraph(&words, options);
    output.extend(render_lines(&lines, options));

    Ok(output)
}