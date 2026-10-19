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
    pieces: usize,
    marker: bool,
}

#[derive(Clone)]
struct Layout {
    cost: u64,
    marker_count: usize,
    lines: Vec<Line>,
    consumed: Vec<usize>,
    texts: Vec<String>,
}

fn separators(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{00A0}')
}

fn better(a: &Layout, b: &Layout) -> bool {
    if a.cost != b.cost {
        return a.cost < b.cost;
    }
    if a.marker_count != b.marker_count {
        return a.marker_count < b.marker_count;
    }
    if a.lines.len() != b.lines.len() {
        return a.lines.len() < b.lines.len();
    }
    if a.consumed != b.consumed {
        return a.consumed > b.consumed;
    }
    a.texts < b.texts
}

fn layout_paragraph(words: &[Word], options: &WrapOptions) -> Vec<Line> {
    let mut states: Vec<Vec<Option<Layout>>> = words
        .iter()
        .map(|w| vec![None; w.chars.len() + 1])
        .collect();
    let mut best_final: Option<Layout> = None;

    fn search(
        words: &[Word],
        options: &WrapOptions,
        wi: usize,
        off: usize,
        first: bool,
        prefix: Layout,
        states: &mut [Vec<Option<Layout>>],
        best_final: &mut Option<Layout>,
    ) {
        if wi == words.len() {
            let replace = best_final
                .as_ref()
                .map_or(true, |old| better(&prefix, old));
            if replace {
                *best_final = Some(prefix);
            }
            return;
        }

        if let Some(old) = &states[wi][off] {
            if !better(&prefix, old) {
                return;
            }
        }
        states[wi][off] = Some(prefix.clone());

        let indent = if first {
            options.first_indent
        } else {
            options.rest_indent
        };
        let cap = options.width - indent;
        let word = &words[wi];
        let remaining = word.chars.len() - off;

        let mut choices: Vec<(usize, usize, bool, bool)> = Vec::new();
        let mut used = 0usize;
        let mut pos = wi;
        let mut start = off;
        let mut pieces = 0usize;
        let mut display = String::new();
        let mut consumed = 0usize;

        loop {
            let w = &words[pos];
            let suffix_len = w.chars.len() - start;
            if pieces > 0 {
                if used + 1 > cap {
                    break;
                }
                used += 1;
                display.push(' ');
            }
            let available = cap - used;

            if suffix_len <= available {
                for &c in &w.chars[start..] {
                    display.push(c);
                }
                used += suffix_len;
                consumed += suffix_len;
                pieces += 1;
                choices.push((pos + 1, 0, false, false));
            } else {
                for &c in &w.chars[start..start + available.min(suffix_len)] {
                    display.push(c);
                }

                // Marker endings are available at any marker after the current offset.
                for &mark in &w.markers {
                    if mark > start && mark < w.chars.len() {
                        let prefix_len = mark - start;
                        if prefix_len + 1 <= available {
                            choices.push((pos, mark, true, false));
                        }
                    }
                }

                // A raw split is legal only on an otherwise empty line and only
                // when none of the remaining marker prefixes can fit.
                if pieces == 0 && suffix_len > cap {
                    let marker_fits = w.markers.iter().any(|&mark| {
                        mark > start && mark < w.chars.len() && mark - start + 1 <= cap
                    });
                    if !marker_fits {
                        choices.push((pos, start + cap, false, true));
                    }
                }
                break;
            }

            if pos + 1 >= words.len() {
                break;
            }
            pos += 1;
            start = 0;
        }

        // Rebuild each candidate line from its endpoint, preserving exact pieces.
        for (end_w, end_off, is_marker, is_raw) in choices {
            let mut line_text = String::new();
            for _ in 0..indent {
                line_text.push(' ');
            }

            let mut line_pieces = 0usize;
            let mut line_consumed = 0usize;
            let mut cursor_w = wi;
            let mut cursor_off = off;
            let mut ended = false;

            while cursor_w < words.len() && !ended {
                let w = &words[cursor_w];
                if line_pieces > 0 {
                    line_text.push(' ');
                }
                line_pieces += 1;

                if is_marker && cursor_w == end_w {
                    for &c in &w.chars[cursor_off..end_off] {
                        line_text.push(c);
                    }
                    line_text.push('-');
                    line_consumed += end_off - cursor_off;
                    ended = true;
                } else if is_raw && cursor_w == end_w {
                    for &c in &w.chars[cursor_off..end_off] {
                        line_text.push(c);
                    }
                    line_consumed += end_off - cursor_off;
                    ended = true;
                } else {
                    for &c in &w.chars[cursor_off..] {
                        line_text.push(c);
                    }
                    line_consumed += w.chars.len() - cursor_off;
                    if cursor_w + 1 == end_w {
                        ended = true;
                    } else {
                        cursor_w += 1;
                        cursor_off = 0;
                    }
                }
            }

            if line_consumed == 0 {
                continue;
            }

            let is_final = if is_marker || is_raw {
                false
            } else {
                end_w >= words.len()
            };
            let line_width = line_text.chars().count();
            let gap = options.width - line_width;
            let square = (gap as u64) * (gap as u64);
            let mut line_cost = if is_final { square / 4 } else { square };
            if is_marker {
                line_cost += options.hyphen_cost as u64;
            }
            if is_raw {
                line_cost += (2 * options.width + 1) as u64;
            }
            if !is_final && line_pieces == 1 {
                line_cost += 3;
            }

            let mut next = prefix.clone();
            next.cost += line_cost;
            next.marker_count += usize::from(is_marker);
            next.consumed.push(line_consumed);
            next.texts.push(line_text.clone());
            next.lines.push(Line {
                text: line_text,
                consumed: line_consumed,
                pieces: line_pieces,
                marker: is_marker,
            });

            if is_marker || is_raw {
                search(
                    words,
                    options,
                    end_w,
                    end_off,
                    false,
                    next,
                    states,
                    best_final,
                );
            } else {
                search(
                    words,
                    options,
                    end_w,
                    0,
                    false,
                    next,
                    states,
                    best_final,
                );
            }
        }
    }

    let initial = Layout {
        cost: 0,
        marker_count: 0,
        lines: Vec::new(),
        consumed: Vec::new(),
        texts: Vec::new(),
    };
    search(
        words,
        options,
        0,
        0,
        true,
        initial,
        &mut states,
        &mut best_final,
    );

    best_final.map(|x| x.lines).unwrap_or_default()
}

fn parse_words(segment: &[char]) -> Vec<Word> {
    let mut words = Vec::new();
    let mut chars = Vec::new();
    let mut markers = Vec::new();

    for &c in segment {
        if separators(c) {
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

fn render_lines(mut lines: Vec<Line>, options: &WrapOptions) -> Vec<String> {
    if options.justify {
        let width = options.width;
        let last = lines.len().saturating_sub(1);
        for (i, line) in lines.iter_mut().enumerate() {
            if i == last || line.pieces < 2 {
                continue;
            }
            let gaps = line.pieces - 1;
            let used = line.text.chars().count();
            let extra = width - used;
            let base = extra / gaps;
            let rem = extra % gaps;
            let start = i % gaps;

            let mut parts = line.text.split(' ');
            let mut result = parts.next().unwrap_or("").to_string();
            for gap in 0..gaps {
                let add = base + usize::from((gap + gaps - start) % gaps < rem);
                for _ in 0..(1 + add) {
                    result.push(' ');
                }
                result.push_str(parts.next().unwrap_or(""));
            }
            line.text = result;
        }
    }
    lines.into_iter().map(|l| l.text).collect()
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

    let mut byte_indices = Vec::with_capacity(chars.len());
    let mut byte = 0usize;
    for &c in &chars {
        byte_indices.push(byte);
        byte += c.len_utf8();
        if c.is_control() && !matches!(c, '\t' | '\r' | '\n') {
            return Err(WrapError::InvalidControl {
                index: *byte_indices.last().unwrap(),
            });
        }
    }

    for i in 0..chars.len() {
        if chars[i] == '~'
            && (i == 0
                || i + 1 == chars.len()
                || chars[i - 1] == '~'
                || chars[i + 1] == '~'
                || separators(chars[i - 1])
                || separators(chars[i + 1]))
        {
            return Err(WrapError::InvalidMarker {
                index: byte_indices[i],
            });
        }
    }

    let mut normalized = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\r' {
            if i + 1 < chars.len() && chars[i + 1] == '\n' {
                normalized.push('\n');
                i += 2;
            } else {
                normalized.push('\n');
                i += 1;
            }
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
            let mut j = i;
            while j < normalized.len() && normalized[j] == '\n' {
                j += 1;
            }
            if j - i >= 2 {
                let words = parse_words(&segment);
                if !words.is_empty() {
                    output.extend(render_lines(layout_paragraph(&words, options), options));
                }
                segment.clear();
                for _ in 1..(j - i) {
                    output.push(String::new());
                }
                i = j;
                continue;
            }
        }
        segment.push(normalized[i]);
        i += 1;
    }

    let words = parse_words(&segment);
    if !words.is_empty() {
        output.extend(render_lines(layout_paragraph(&words, options), options));
    }

    Ok(output)
}