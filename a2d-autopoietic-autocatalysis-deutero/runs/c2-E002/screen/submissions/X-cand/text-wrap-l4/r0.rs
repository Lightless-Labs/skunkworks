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
    text: String,
    consumed: usize,
    marker_ended: bool,
    pieces: usize,
    gap: usize,
}

#[derive(Clone)]
struct Plan {
    cost: u64,
    marker_lines: usize,
    lines: Vec<Line>,
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
    for (x, y) in a.lines.iter().zip(&b.lines) {
        if x.consumed != y.consumed {
            return x.consumed > y.consumed;
        }
    }
    for (x, y) in a.lines.iter().zip(&b.lines) {
        if x.text != y.text {
            return x.text < y.text;
        }
    }
    false
}

fn add_line_cost(
    tail: &Plan,
    gap: usize,
    pieces: usize,
    marker: bool,
    raw: bool,
    hyphen_cost: usize,
    width: usize,
) -> Plan {
    let square = (gap as u64) * (gap as u64);
    let mut cost = tail.cost + if tail.lines.is_empty() { square / 4 } else { square };
    if marker {
        cost += hyphen_cost as u64;
    }
    if raw {
        cost += (2 * width + 1) as u64;
    }
    if !tail.lines.is_empty() && pieces == 1 {
        cost += 3;
    }
    Plan {
        cost,
        marker_lines: tail.marker_lines + usize::from(marker),
        lines: tail.lines.clone(),
    }
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

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{00a0}')
}

fn solve_paragraph(words: &[Word], options: &WrapOptions) -> Vec<Line> {
    fn solve(
        words: &[Word],
        wi: usize,
        offset: usize,
        first: bool,
        options: &WrapOptions,
        memo: &mut std::collections::HashMap<(usize, usize, bool), Plan>,
    ) -> Plan {
        if wi >= words.len() {
            return Plan {
                cost: 0,
                marker_lines: 0,
                lines: Vec::new(),
            };
        }
        if let Some(p) = memo.get(&(wi, offset, first)) {
            return p.clone();
        }

        let indent = if first {
            options.first_indent
        } else {
            options.rest_indent
        };
        let capacity = options.width - indent;
        let mut candidates = Vec::new();
        let mut rendered = String::new();
        rendered.extend(std::iter::repeat(' ').take(indent));
        let mut consumed = 0usize;
        let mut pieces = 0usize;
        let mut cur_wi = wi;
        let mut cur_offset = offset;

        loop {
            let word = &words[cur_wi];
            let remaining = word.chars.len() - cur_offset;
            if pieces > 0 {
                if rendered.chars().count() - indent + 1 >= capacity {
                    break;
                }
                rendered.push(' ');
            }
            let available = capacity - (rendered.chars().count() - indent);
            if available == 0 {
                break;
            }

            let full_fits = remaining <= available;
            if full_fits {
                rendered.extend(word.chars[cur_offset..].iter());
                consumed += remaining;
                pieces += 1;
                let next_wi = cur_wi + 1;
                let tail = solve(words, next_wi, 0, false, options, memo);
                let gap = options.width - indent - (rendered.chars().count() - indent);
                let mut plan = add_line_cost(
                    &tail,
                    gap,
                    pieces,
                    false,
                    false,
                    options.hyphen_cost,
                    options.width,
                );
                let mut text = rendered.clone();
                plan.lines.insert(
                    0,
                    Line {
                        text: text.clone(),
                        consumed,
                        marker_ended: false,
                        pieces,
                        gap,
                    },
                );
                candidates.push(plan);

                if next_wi >= words.len() {
                    break;
                }
                cur_wi = next_wi;
                cur_offset = 0;
                continue;
            }

            // Marked breaks within this word.
            for &mark in &word.markers {
                if mark <= cur_offset || mark >= word.chars.len() {
                    continue;
                }
                let prefix_len = mark - cur_offset;
                let need = prefix_len + 1;
                if need <= available {
                    let mut marked_text = rendered.clone();
                    marked_text.extend(word.chars[cur_offset..mark].iter());
                    marked_text.push('-');
                    let tail = solve(words, cur_wi, mark, false, options, memo);
                    let line_gap = options.width
                        - indent
                        - (marked_text.chars().count() - indent);
                    let mut plan = add_line_cost(
                        &tail,
                        line_gap,
                        pieces + 1,
                        true,
                        false,
                        options.hyphen_cost,
                        options.width,
                    );
                    plan.lines.insert(
                        0,
                        Line {
                            text: marked_text,
                            consumed: consumed + prefix_len,
                            marker_ended: true,
                            pieces: pieces + 1,
                            gap: line_gap,
                        },
                    );
                    candidates.push(plan);
                }
            }

            // Raw split is only legal as the first and only piece on this line,
            // and only when no remaining marker break fits.
            if pieces == 0 && remaining > capacity {
                let marker_fits = word.markers.iter().any(|&mark| {
                    mark > cur_offset
                        && mark < word.chars.len()
                        && mark - cur_offset + 1 <= capacity
                });
                if !marker_fits {
                    let mut raw_text = rendered.clone();
                    raw_text.extend(word.chars[cur_offset..cur_offset + capacity].iter());
                    let tail = solve(words, cur_wi, cur_offset + capacity, false, options, memo);
                    let line_gap = 0;
                    let mut plan = add_line_cost(
                        &tail,
                        line_gap,
                        1,
                        false,
                        true,
                        options.hyphen_cost,
                        options.width,
                    );
                    plan.lines.insert(
                        0,
                        Line {
                            text: raw_text,
                            consumed: capacity,
                            marker_ended: false,
                            pieces: 1,
                            gap: line_gap,
                        },
                    );
                    candidates.push(plan);
                }
            }
            break;
        }

        let best = candidates
            .into_iter()
            .reduce(|a, b| if better(&a, &b) { a } else { b })
            .unwrap_or(Plan {
                cost: u64::MAX / 4,
                marker_lines: usize::MAX / 4,
                lines: Vec::new(),
            });
        memo.insert((wi, offset, first), best.clone());
        best
    }

    if words.is_empty() {
        return Vec::new();
    }
    solve(words, 0, 0, true, options, &mut std::collections::HashMap::new()).lines
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

    let mut byte_index = 0usize;
    for &c in &chars {
        if c.is_control() && !matches!(c, '\t' | '\r' | '\n') {
            return Err(WrapError::InvalidControl { index: byte_index });
        }
        byte_index += c.len_utf8();
    }

    let mut byte_offsets = Vec::with_capacity(chars.len());
    let mut index = 0usize;
    for &c in &chars {
        byte_offsets.push(index);
        index += c.len_utf8();
    }
    for i in 0..chars.len() {
        if chars[i] == '~'
            && (i == 0
                || i + 1 == chars.len()
                || chars[i - 1] == '~'
                || chars[i + 1] == '~'
                || is_separator(chars[i - 1])
                || is_separator(chars[i + 1]))
        {
            return Err(WrapError::InvalidMarker {
                index: byte_offsets[i],
            });
        }
    }

    let mut normalized = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\r' && i + 1 < chars.len() && chars[i + 1] == '\n' {
            normalized.push('\n');
            i += 2;
        } else if chars[i] == '\r' {
            normalized.push('\n');
            i += 1;
        } else {
            normalized.push(chars[i]);
            i += 1;
        }
    }

    let mut output = Vec::new();
    let mut segment = Vec::new();
    let mut i = 0;
    while i < normalized.len() {
        if normalized[i] == '\n' {
            let start = i;
            while i < normalized.len() && normalized[i] == '\n' {
                i += 1;
            }
            if i - start >= 2 {
                let words = parse_words(&segment);
                let mut lines = solve_paragraph(&words, options);
                render_lines(&mut lines, options);
                output.extend(lines.into_iter().map(|line| line.text));
                output.extend(std::iter::repeat(String::new()).take(i - start - 1));
                segment.clear();
                continue;
            }
        }
        segment.push(normalized[i]);
        i += 1;
    }
    let words = parse_words(&segment);
    let mut lines = solve_paragraph(&words, options);
    render_lines(&mut lines, options);
    output.extend(lines.into_iter().map(|line| line.text));
    Ok(output)
}

fn render_lines(lines: &mut [Line], options: &WrapOptions) {
    if !options.justify {
        return;
    }
    for i in 0..lines.len().saturating_sub(1) {
        if lines[i].pieces < 2 {
            continue;
        }
        let gap_count = lines[i].pieces - 1;
        if gap_count == 0 {
            continue;
        }
        let extra = lines[i].gap;
        let base = extra / gap_count;
        let rem = extra % gap_count;
        let mut chars: Vec<char> = lines[i].text.chars().collect();
        let indent = if i == 0 {
            options.first_indent
        } else {
            options.rest_indent
        };
        let content: Vec<char> = chars.drain(indent..).collect();
        let mut rebuilt = String::new();
        rebuilt.extend(std::iter::repeat(' ').take(indent));
        let mut gap_index = 0usize;
        let mut j = 0usize;
        while j < content.len() {
            rebuilt.push(content[j]);
            if content[j] == ' ' {
                let mut count = 1 + base;
                let start = i % gap_count;
                if (gap_index + gap_count - start) % gap_count < rem {
                    count += 1;
                }
                rebuilt.extend(std::iter::repeat(' ').take(count - 1));
                gap_index += 1;
            }
            j += 1;
        }
        lines[i].text = rebuilt;
    }
}