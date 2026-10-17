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
enum Ending {
    Complete { words: usize },
    Marker { offset: usize },
    Raw { offset: usize },
}

#[derive(Clone)]
struct LineChoice {
    ending: Ending,
    consumed: usize,
    pieces: usize,
    gap: usize,
    marker_lines: usize,
    line: String,
}

#[derive(Clone)]
struct Plan {
    cost: u64,
    marker_lines: usize,
    lines: Vec<LineChoice>,
    consumed: Vec<usize>,
    rendered: Vec<String>,
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
    if a.consumed != b.consumed {
        return a.consumed > b.consumed;
    }
    a.rendered < b.rendered
}

fn parse_words(segment: &[char]) -> Vec<Word> {
    let mut words = Vec::new();
    let mut chars = Vec::new();
    let mut markers = Vec::new();

    let flush = |words: &mut Vec<Word>,
                 chars: &mut Vec<char>,
                 markers: &mut Vec<usize>| {
        if !chars.is_empty() {
            words.push(Word {
                chars: std::mem::take(chars),
                markers: std::mem::take(markers),
            });
        }
    };

    for &ch in segment {
        if matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{00a0}') {
            flush(&mut words, &mut chars, &mut markers);
        } else if ch == '~' {
            markers.push(chars.len());
        } else {
            chars.push(ch);
        }
    }
    flush(&mut words, &mut chars, &mut markers);
    words
}

fn optimize(words: &[Word], options: &WrapOptions) -> Plan {
    let mut memo: Vec<Vec<Option<Plan>>> = words
        .iter()
        .map(|w| vec![None; w.chars.len() + 1])
        .collect();
    let mut done: Vec<Vec<bool>> = words
        .iter()
        .map(|w| vec![false; w.chars.len() + 1])
        .collect();

    fn solve(
        wi: usize,
        off: usize,
        first: bool,
        words: &[Word],
        options: &WrapOptions,
        memo: &mut [Vec<Option<Plan>>],
        done: &mut [Vec<bool>],
    ) -> Plan {
        if wi == words.len() {
            return Plan {
                cost: 0,
                marker_lines: 0,
                lines: Vec::new(),
                consumed: Vec::new(),
                rendered: Vec::new(),
            };
        }
        if !first && done[wi][off] {
            return memo[wi][off].as_ref().unwrap().clone();
        }

        let indent = if first {
            options.first_indent
        } else {
            options.rest_indent
        };
        let capacity = options.width - indent;
        let start_word = wi;
        let start_off = off;

        let mut candidates: Vec<(Ending, usize, usize, String)> = Vec::new();
        let mut display = String::new();
        let mut used = 0usize;
        let mut consumed = 0usize;
        let mut pieces = 0usize;
        let mut cursor_word = wi;
        let mut cursor_off = off;

        while cursor_word < words.len() {
            let word = &words[cursor_word];
            let remaining = word.chars.len() - cursor_off;
            let separator = if pieces == 0 { 0 } else { 1 };
            if used + separator >= capacity {
                break;
            }
            let available = capacity - used - separator;

            let mut prefix = String::new();
            for &ch in &word.chars[cursor_off..] {
                prefix.push(ch);
            }

            let take = remaining.min(available);
            if take > 0 {
                if separator > 0 {
                    display.push(' ');
                    used += 1;
                }
                display.extend(word.chars[cursor_off..cursor_off + take].iter());
                used += take;
                consumed += take;
                pieces += 1;

                if take == remaining {
                    candidates.push((
                        Ending::Complete {
                            words: cursor_word + 1,
                        },
                        consumed,
                        pieces,
                        display.clone(),
                    ));
                    cursor_word += 1;
                    cursor_off = 0;
                    if cursor_word == words.len() {
                        break;
                    }
                    if used + 1 >= capacity {
                        break;
                    }
                } else {
                    // Marker-ending choices within the current suffix.
                    for &marker in &word.markers {
                        if marker > cursor_off && marker < cursor_off + take {
                            let prefix_len = marker - cursor_off;
                            let base_used = used - take;
                            let prior_sep = if pieces > 1 { 1 } else { 0 };
                            let line_chars = base_used + prior_sep + prefix_len + 1;
                            if line_chars <= capacity {
                                let mut text = String::new();
                                if pieces > 1 {
                                    text.push_str(&display[..display.len() - take]);
                                    text.push(' ');
                                }
                                text.extend(word.chars[cursor_off..marker].iter());
                                text.push('-');
                                candidates.push((
                                    Ending::Marker { offset: marker },
                                    consumed - take + prefix_len,
                                    pieces,
                                    text,
                                ));
                            }
                        }
                    }
                    break;
                }
            } else {
                break;
            }
        }

        // Add marker choices from the starting word, including markers beyond
        // the prefix that would otherwise fit on the line.
        let word = &words[start_word];
        for &marker in &word.markers {
            if marker > start_off && marker < word.chars.len() {
                let prefix_len = marker - start_off;
                if prefix_len + 1 <= capacity {
                    let mut text = String::new();
                    text.extend(word.chars[start_off..marker].iter());
                    text.push('-');
                    candidates.push((
                        Ending::Marker { offset: marker },
                        prefix_len,
                        1,
                        text,
                    ));
                }
            }
        }

        // Raw splitting is available only on an otherwise empty line and only
        // when no remaining marker prefix plus hyphen fits.
        if pieces == 0 {
            let word = &words[wi];
            let remaining = word.chars.len() - off;
            let marker_fits = word.markers.iter().any(|&m| {
                m > off && m < word.chars.len() && (m - off + 1) <= capacity
            });
            if remaining > capacity && !marker_fits {
                let mut text = String::new();
                text.extend(word.chars[off..off + capacity].iter());
                candidates.push((
                    Ending::Raw {
                        offset: off + capacity,
                    },
                    capacity,
                    1,
                    text,
                ));
            }
        }

        let mut best: Option<Plan> = None;
        for (ending, line_consumed, line_pieces, body) in candidates {
            let (next_wi, next_off, marker, raw) = match ending {
                Ending::Complete { words } => (words, 0, false, false),
                Ending::Marker { offset } => (wi, offset, true, false),
                Ending::Raw { offset } => (wi, offset, false, true),
            };
            let final_line = next_wi == words.len();
            let content_width = body.chars().count() + indent;
            let gap = options.width - content_width;
            let mut cost = (gap * gap) as u64;
            if final_line {
                cost /= 4;
            }
            if marker {
                cost += options.hyphen_cost as u64;
            }
            if raw {
                cost += (2 * options.width + 1) as u64;
            }
            if !final_line && line_pieces == 1 {
                cost += 3;
            }

            let mut line = " ".repeat(indent);
            line.push_str(&body);

            let tail = solve(
                next_wi,
                next_off,
                false,
                words,
                options,
                memo,
                done,
            );
            let mut plan = Plan {
                cost: cost + tail.cost,
                marker_lines: usize::from(marker) + tail.marker_lines,
                lines: Vec::new(),
                consumed: Vec::new(),
                rendered: Vec::new(),
            };
            plan.lines.push(LineChoice {
                ending: match ending {
                    Ending::Complete { words } => Ending::Complete { words },
                    Ending::Marker { offset } => Ending::Marker { offset },
                    Ending::Raw { offset } => Ending::Raw { offset },
                },
                consumed: line_consumed,
                pieces: line_pieces,
                gap,
                marker_lines: usize::from(marker),
                line: line.clone(),
            });
            plan.lines.extend(tail.lines);
            plan.consumed.push(line_consumed);
            plan.consumed.extend(tail.consumed);
            plan.rendered.push(line);
            plan.rendered.extend(tail.rendered);

            if best.as_ref().is_none_or(|current| better(&plan, current)) {
                best = Some(plan);
            }
        }

        let result = best.unwrap_or_else(|| {
            // Capacity is positive, so the raw split rule always supplies a
            // continuation for an overlong word.
            Plan {
                cost: 0,
                marker_lines: 0,
                lines: Vec::new(),
                consumed: Vec::new(),
                rendered: Vec::new(),
            }
        });

        if !first {
            done[wi][off] = true;
            memo[wi][off] = Some(result.clone());
        }
        result
    }

    solve(0, 0, true, words, options, &mut memo, &mut done)
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

    for &(index, ch) in &chars {
        if ch.is_control() && !matches!(ch, '\t' | '\r' | '\n') {
            return Err(WrapError::InvalidControl { index });
        }
    }

    let is_separator = |ch: char| matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{00a0}');
    for i in 0..chars.len() {
        if chars[i].1 == '~'
            && (i == 0
                || i + 1 == chars.len()
                || chars[i - 1].1 == '~'
                || is_separator(chars[i - 1].1)
                || is_separator(chars[i + 1].1))
        {
            return Err(WrapError::InvalidMarker { index: chars[i].0 });
        }
    }

    let mut normalized = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i].1;
        if ch == '\r' {
            if i + 1 < chars.len() && chars[i + 1].1 == '\n' {
                normalized.push('\n');
                i += 2;
            } else {
                normalized.push('\n');
                i += 1;
            }
        } else {
            normalized.push(ch);
            i += 1;
        }
    }

    let mut output = Vec::new();
    let mut segment = Vec::new();
    let mut i = 0;
    while i < normalized.len() {
        if normalized[i] == '\n' {
            let mut j = i;
            while j < normalized.len() && normalized[j] == '\n' {
                j += 1;
            }
            if j - i >= 2 {
                let words = parse_words(&segment);
                if !words.is_empty() {
                    let plan = optimize(&words, options);
                    let mut rendered = plan.rendered;
                    if options.justify && rendered.len() > 1 {
                        for line_idx in 0..rendered.len() - 1 {
                            let pieces = plan.lines[line_idx].pieces;
                            if pieces < 2 {
                                continue;
                            }
                            let indent = if line_idx == 0 {
                                options.first_indent
                            } else {
                                options.rest_indent
                            };
                            let body = rendered[line_idx][indent..].to_string();
                            let gaps = pieces - 1;
                            let extra = options.width - rendered[line_idx].chars().count();
                            let each = extra / gaps;
                            let rem = extra % gaps;
                            let start = line_idx % gaps;
                            let mut rebuilt = String::new();
                            rebuilt.push_str(&" ".repeat(indent));
                            let mut seen = 0usize;
                            for ch in body.chars() {
                                if ch == ' ' {
                                    rebuilt.push(' ');
                                    rebuilt.push_str(&" ".repeat(each));
                                    let distance = (seen + gaps - start) % gaps;
                                    if distance < rem {
                                        rebuilt.push(' ');
                                    }
                                    seen += 1;
                                } else {
                                    rebuilt.push(ch);
                                }
                            }
                            rendered[line_idx] = rebuilt;
                        }
                    }
                    output.extend(rendered);
                }
                output.extend(std::iter::repeat(String::new()).take(j - i - 1));
                segment.clear();
                i = j;
            } else {
                segment.push('\n');
                i += 1;
            }
        } else {
            segment.push(normalized[i]);
            i += 1;
        }
    }

    let words = parse_words(&segment);
    if !words.is_empty() {
        let plan = optimize(&words, options);
        let mut rendered = plan.rendered;
        if options.justify && rendered.len() > 1 {
            for line_idx in 0..rendered.len() - 1 {
                let pieces = plan.lines[line_idx].pieces;
                if pieces < 2 {
                    continue;
                }
                let indent = if line_idx == 0 {
                    options.first_indent
                } else {
                    options.rest_indent
                };
                let body = rendered[line_idx][indent..].to_string();
                let gaps = pieces - 1;
                let extra = options.width - rendered[line_idx].chars().count();
                let each = extra / gaps;
                let rem = extra % gaps;
                let start = line_idx % gaps;
                let mut rebuilt = String::new();
                rebuilt.push_str(&" ".repeat(indent));
                let mut seen = 0usize;
                for ch in body.chars() {
                    if ch == ' ' {
                        rebuilt.push(' ');
                        rebuilt.push_str(&" ".repeat(each));
                        let distance = (seen + gaps - start) % gaps;
                        if distance < rem {
                            rebuilt.push(' ');
                        }
                        seen += 1;
                    } else {
                        rebuilt.push(ch);
                    }
                }
                rendered[line_idx] = rebuilt;
            }
        }
        output.extend(rendered);
    }

    Ok(output)
}