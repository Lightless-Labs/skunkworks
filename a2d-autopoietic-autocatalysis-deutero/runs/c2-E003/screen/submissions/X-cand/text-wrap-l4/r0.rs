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
    text: String,
    pieces: usize,
    gap: usize,
    consumed: usize,
    marker_ended: bool,
}

#[derive(Clone)]
struct Layout {
    cost: u64,
    marker_lines: usize,
    lines: Vec<Line>,
}

fn separators(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{00a0}')
}

fn scalar_len(s: &str) -> usize {
    s.chars().count()
}

fn better(a: &Layout, b: &Layout) -> bool {
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

fn line_cost(gap: usize, final_line: bool) -> u64 {
    let sq = (gap * gap) as u64;
    if final_line { sq / 4 } else { sq }
}

fn solve(
    words: &[Word],
    wi: usize,
    off: usize,
    first: bool,
    opts: &WrapOptions,
    memo: &mut Vec<Vec<Option<Layout>>>,
) -> Layout {
    if wi >= words.len() {
        return Layout { cost: 0, marker_lines: 0, lines: Vec::new() };
    }
    if let Some(v) = &memo[wi][off] {
        return v.clone();
    }

    let indent = if first { opts.first_indent } else { opts.rest_indent };
    let cap = opts.width - indent;
    let word = &words[wi];
    let remaining = word.chars.len() - off;
    let mut candidates: Vec<(usize, usize, bool, bool)> = Vec::new();

    // End after any completed word, possibly after adding later complete words.
    let mut used = 0usize;
    let mut idx = wi;
    while idx < words.len() {
        let start = if idx == wi { off } else { 0 };
        let len = words[idx].chars.len() - start;
        let add = len + if idx == wi { 0 } else { 1 };
        if used + add > cap { break; }
        used += add;
        candidates.push((idx + 1, 0, false, false));
        idx += 1;
    }

    // Marker endings in the current word, and in subsequent words after complete pieces.
    let mut prefix_used = 0usize;
    for j in wi..words.len() {
        let start = if j == wi { off } else { 0 };
        let sep = if j == wi { 0 } else { 1 };
        if prefix_used + sep >= cap { break; }
        let base = prefix_used + sep;
        for &m in &words[j].markers {
            if m <= start || m >= words[j].chars.len() { continue; }
            let count = m - start;
            if base + count + 1 <= cap {
                candidates.push((j, m, true, false));
            }
        }
        let full = words[j].chars.len() - start;
        if base + full > cap { break; }
        prefix_used = base + full;
    }

    // Raw split is available only at the start of an otherwise empty line,
    // and only if no marker prefix can fit.
    if remaining > cap {
        let any_marker_fits = word.markers.iter().any(|&m| {
            m > off && m < word.chars.len() && m - off + 1 <= cap
        });
        if !any_marker_fits {
            candidates.push((wi, off + cap, false, true));
        }
    }

    let mut best: Option<Layout> = None;
    for (next_wi, next_off, marker, raw) in candidates {
        let mut rendered = String::new();
        rendered.extend(word.chars[off..if marker || raw { next_off } else { word.chars.len() }].iter());
        let mut pieces = 1usize;
        let mut consumed = if marker || raw {
            next_off - off
        } else {
            word.chars.len() - off
        };

        if !marker && !raw {
            for j in wi + 1..next_wi {
                rendered.push(' ');
                rendered.extend(words[j].chars.iter());
                pieces += 1;
                consumed += words[j].chars.len();
            }
        }
        if marker {
            rendered.push('-');
        }
        let full_text = format!("{}{}", " ".repeat(indent), rendered);
        let displayed = scalar_len(&full_text);
        let gap = opts.width - displayed;

        let tail = solve(words, next_wi, next_off, false, opts, memo);
        let final_line = tail.lines.is_empty();
        let mut cost = line_cost(gap, final_line) + tail.cost;
        if marker { cost += opts.hyphen_cost as u64; }
        if raw { cost += (2 * opts.width + 1) as u64; }
        if !final_line && pieces == 1 { cost += 3; }

        let mut lines = vec![Line {
            indent,
            text: full_text,
            pieces,
            gap,
            consumed,
            marker_ended: marker,
        }];
        lines.extend(tail.lines);
        let candidate = Layout {
            cost,
            marker_lines: tail.marker_lines + usize::from(marker),
            lines,
        };
        if best.as_ref().map_or(true, |b| better(&candidate, b)) {
            best = Some(candidate);
        }
    }

    let result = best.unwrap_or_else(|| {
        // The raw-split rule ensures a continuation; this fallback is unreachable
        // for validated options.
        Layout { cost: 0, marker_lines: 0, lines: Vec::new() }
    });
    memo[wi][off] = Some(result.clone());
    result
}

fn paragraph(words: &[Word], opts: &WrapOptions) -> Vec<String> {
    if words.is_empty() {
        return Vec::new();
    }
    let mut memo: Vec<Vec<Option<Layout>>> = words
        .iter()
        .map(|w| vec![None; w.chars.len() + 1])
        .collect();
    let mut layout = solve(words, 0, 0, true, opts, &mut memo);
    let line_count = layout.lines.len();

    for (i, line) in layout.lines.iter_mut().enumerate() {
        if opts.justify && i + 1 < line_count && line.pieces >= 2 {
            let mut gaps = Vec::new();
            let mut in_word = false;
            for (pos, c) in line.text.char_indices() {
                if c == ' ' {
                    // Spaces in the indentation are excluded.
                    if scalar_len(&line.text[..pos]) >= line.indent {
                        gaps.push(pos);
                    }
                    in_word = false;
                } else {
                    in_word = true;
                }
            }
            let _ = in_word;
            // Rebuild based on pieces, retaining the indentation separately.
            let content = line.text.chars().skip(line.indent).collect::<String>();
            let parts: Vec<&str> = content.split(' ').collect();
            let g = parts.len().saturating_sub(1);
            if g > 0 {
                let extra = line.gap;
                let each = extra / g;
                let rem = extra % g;
                let mut rebuilt = " ".repeat(line.indent);
                for (k, part) in parts.iter().enumerate() {
                    if k > 0 {
                        let gap_index = k - 1;
                        let start = (i % g);
                        let cyclic_distance = (gap_index + g - start) % g;
                        let bonus = usize::from(cyclic_distance < rem);
                        rebuilt.push_str(&" ".repeat(1 + each + bonus));
                    }
                    rebuilt.push_str(part);
                }
                line.text = rebuilt;
            }
        }
    }

    layout.lines.into_iter().map(|l| l.text).collect()
}

pub fn wrap(input: &str, options: &WrapOptions) -> Result<Vec<String>, WrapError> {
    if options.width == 0 {
        return Err(WrapError::WidthZero);
    }
    if options.width > 256 {
        return Err(WrapError::WidthTooLarge { width: options.width });
    }
    if options.first_indent >= options.width {
        return Err(WrapError::FirstIndentTooWide { indent: options.first_indent });
    }
    if options.rest_indent >= options.width {
        return Err(WrapError::RestIndentTooWide { indent: options.rest_indent });
    }
    if options.hyphen_cost > 1_000_000 {
        return Err(WrapError::HyphenCostTooLarge { cost: options.hyphen_cost });
    }

    let chars = input.chars().count();
    if chars > 512 {
        return Err(WrapError::InputTooLong { chars });
    }

    for (index, c) in input.char_indices() {
        if c.is_control() && !matches!(c, '\t' | '\r' | '\n') {
            return Err(WrapError::InvalidControl { index });
        }
    }

    let original: Vec<(usize, char)> = input.char_indices().collect();
    for i in 0..original.len() {
        if original[i].1 == '~' {
            let valid_side = |j: isize| -> bool {
                if j < 0 || j as usize >= original.len() { return false; }
                let c = original[j as usize].1;
                c != '~' && !separators(c)
            };
            if !valid_side(i as isize - 1) || !valid_side(i as isize + 1) {
                return Err(WrapError::InvalidMarker { index: original[i].0 });
            }
        }
    }

    let mut normalized = Vec::new();
    let mut it = input.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\r' {
            if it.peek() == Some(&'\n') { it.next(); }
            normalized.push('\n');
        } else {
            normalized.push(c);
        }
    }

    let mut output = Vec::new();
    let mut segment: Vec<char> = Vec::new();
    let mut i = 0usize;

    while i < normalized.len() {
        if normalized[i] == '\n' {
            let mut j = i;
            while j < normalized.len() && normalized[j] == '\n' { j += 1; }
            if j - i >= 2 {
                let words = parse_words(&segment);
                output.extend(paragraph(&words, options));
                segment.clear();
                output.extend(std::iter::repeat(String::new()).take(j - i - 1));
                i = j;
                continue;
            }
        }
        segment.push(normalized[i]);
        i += 1;
    }
    let words = parse_words(&segment);
    output.extend(paragraph(&words, options));
    Ok(output)
}

fn parse_words(chars: &[char]) -> Vec<Word> {
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
        if separators(c) {
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